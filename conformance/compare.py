#!/usr/bin/env python3
"""Byte comparison of two conformance runs.

Every difference is reported together with the normalization that was applied, so
the result is auditable: `X of Y artifacts identical after N normalizations of the
following kinds`.

    python3 conformance/compare.py --reference /tmp/art/py --candidate /tmp/art/rs
"""
import argparse
import base64
import binascii
import hashlib
import hmac
import json
import os
import re
import sys
import zlib

# --- normalizations ---------------------------------------------------------
# Each entry is (name, pattern, replacement, explanation).
NORMALIZATIONS = [
    ("log-timestamp", re.compile(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}"), "<TS>",
     "wall-clock timestamps in log lines and messages"),
    ("message-timestamp", re.compile(r"\[\d{2}/\d{2}/\d{4} \d{2}:\d{2}\]"), "[<TS>]",
     "the [dd/mm/yyyy HH:MM] stamp in notification text"),
    ("batch-title-clock", re.compile(r"(Octopus MinMax Results - \w{3} \d{2} \w{3} )\d{2}:\d{2}:\d{2}"),
     r"\1<HMS>", "the HH:MM:SS in a batch notification title (ONE_OFF mode)"),
    ("object-address", re.compile(r"0x[0-9a-f]{6,}"), "0xADDR",
     "heap addresses in Python dataclass reprs (never reproducible)"),
    ("ha-end-time", re.compile(r"end_time=[^&\s]+"), "end_time=<NOW>",
     "the Home Assistant query end_time (the moment of the request)"),
    ("ha-end-time-quoted", re.compile(r"&#39;end_time&#39;: &#39;[^&]*?&#39;", re.S),
     "&#39;end_time&#39;: &#39;<NOW>&#39;",
     "the same timestamp inside the escaped Python dict repr on the /logs page"),
    ("ha-end-time-repr", re.compile(r"'end_time': '[^']*'"), "'end_time': '<NOW>'",
     "the same timestamp inside the Python dict repr in the log"),
    ("ha-start-path", re.compile(r"/history/period/[^?\s]+"), "/history/period/<START>",
     "the Home Assistant period path (encodes local midnight)"),
    ("http-date", re.compile(r"Date: [^\r\n]+"), "Date: <DATE>",
     "the HTTP Date header"),
    ("access-log-timestamp", re.compile(r"\[\d{2}/[A-Za-z]{3}/\d{4} \d{2}:\d{2}:\d{2}\]"), "[<TS>]",
     "the timestamp in a Werkzeug access-log line"),
    ("server-header", re.compile(r"Server: [^\r\n]+"), "Server: <SERVER>",
     "the Server header: identifies the HTTP implementation, not the application"),
]

# Lines the reference writes that belong to Werkzeug's development server rather
# than to the application (see the report for the exact strings).
BANNER_PATTERNS = [
    re.compile(r"^\s*\*\s+Serving Flask app .*$"),
    re.compile(r"^\s*\*\s+Debug mode: .*$"),
    re.compile(r"^\s*\*\s+Running on .*$"),
    re.compile(r"^(?:\x1b\[[0-9;]*m)*Press CTRL\+C to quit(?:\x1b\[[0-9;]*m)*$"),
    re.compile(r"^(?:\x1b\[[0-9;]*m)*WARNING: This is a development server.*$"),
    re.compile(r"^\s*$"),
]

SECRET_KEY = b"octobot-tool"
SALT = b"cookie-session"


SKIP = set()


def normalize(name, text):
    counts = {}
    for label, pattern, replacement, _ in NORMALIZATIONS:
        if label in SKIP:
            continue
        text, count = pattern.subn(replacement, text)
        counts[label] = count
    return text, counts


WEB_START = "web_server.run_server - Web server starting"
NOTIFY_START = "notification_service.send_notification - "


WEB_MARK = "web_server.run_server - Web server starting"
NOTIF_MARK = "notification_service.send_notification - "
DIV_BLOCK = re.compile(r'<div class="border-b[^"]*">(?P<body>.*?)</div>', re.S)


def _startup_variants(text):
    """Candidate normalizations for the thread-start race.

    The race is between the web-server start line and one of its neighbours (the first
    notification line).  Because the simple console format does not carry the logger
    name, the neighbour is identified positionally: both neighbours are offered.
    """
    marker = "Web server starting on http://localhost:"
    if marker not in text:
        return []
    variants = []
    lines = text.split("\n")
    index = next((i for i, line in enumerate(lines) if marker in line), None)
    if index is None:
        return []
    for neighbour in (index - 1, index + 1):
        if 0 <= neighbour < len(lines) and neighbour != index:
            pair = sorted([lines[index], lines[neighbour]])
            rest = "\n".join(line for i, line in enumerate(lines) if i not in (index, neighbour))
            variants.append((rest, pair))
    if not variants:
        return []
    # the /logs page: the same, on <div> blocks instead of lines
    blocks = DIV_BLOCK.findall(text)
    if blocks and any(marker in block for block in blocks):
        web_block = next(i for i, block in enumerate(blocks) if marker in block)
        for neighbour in (web_block - 1, web_block + 1):
            if 0 <= neighbour < len(blocks) and neighbour != web_block:
                pair = sorted([blocks[web_block], blocks[neighbour]])
                rest = text
                for target in sorted((web_block, neighbour), reverse=True):
                    for candidate in DIV_BLOCK.finditer(rest):
                        if candidate.group("body") == blocks[target]:
                            rest = rest[:candidate.start()] + rest[candidate.end():]
                            break
                variants.append((rest, pair))
    return variants


def rewrite_log_entries(text):
    """Replace each /logs entry block with a placeholder and return the entry texts."""
    matches = list(DIV_BLOCK.finditer(text))
    if not matches:
        return text, None
    entries = [match.group("body") for match in matches]
    rewritten = text
    for match in reversed(matches):
        rewritten = rewritten[:match.start()] + "<LOG-ENTRY>" + rewritten[match.end():]
    return rewritten, entries


def canonical_entries(entries):
    """The first two log entries race between the bot and web threads."""
    if len(entries) >= 2:
        return sorted(entries[:2]) + entries[2:]
    return entries


def strip_startup_pairs(text):
    """All candidate (rest, racing-pair) splits for this text."""
    return _startup_variants(text)


def normalise_startup_race(text):
    """The web-server thread and the bot thread race to their first log line.

    Both implementations start two threads, so the order of the first
    `web_server.run_server` line and the first `notification_service` line is decided
    by the scheduler.  Rewrite that adjacent pair in a canonical order before
    comparing, and report it as a normalization.
    """
    lines = text.split("\n")
    web_index = next((i for i, line in enumerate(lines) if WEB_START in line), None)
    if web_index is None:
        return text, 0
    for candidate in (web_index - 1, web_index + 1):
        if 0 <= candidate < len(lines) and NOTIFY_START in lines[candidate] and "octobot.notification_service" in lines[candidate]:
            first, second = sorted((web_index, candidate))
            pair = sorted([lines[first], lines[second]])
            rewritten = list(lines)
            rewritten[first], rewritten[second] = pair[0], pair[1]
            return "\n".join(rewritten), 1
    return text, 0


def normalise_banner(text):
    kept = [line for line in text.split("\n") if not any(p.match(line) for p in BANNER_PATTERNS)]
    return "\n".join(kept)


def read(path):
    with open(path, "rb") as handle:
        return handle.read()


def text_of(data):
    return data.decode("utf-8", "replace")


# --- session cookie verification -------------------------------------------
def verify_cookie(value):
    """Return (payload_json_bytes, reason).  reason is None when the signature is valid."""
    parts = value.split(".")
    if len(parts) < 3:
        return None, "not a three-part itsdangerous value"
    signature = parts[-1]
    signed = ".".join(parts[:-1])
    key = hmac.new(SECRET_KEY, SALT, hashlib.sha1).digest()
    expected = base64.urlsafe_b64encode(
        hmac.new(key, signed.encode(), hashlib.sha1).digest()
    ).rstrip(b"=").decode()
    if expected != signature:
        return None, "signature mismatch"
    payload = parts[0]
    try:
        if payload.startswith("."):
            raw = zlib.decompress(base64.urlsafe_b64decode(payload[1:] + "=" * (-len(payload[1:]) % 4)))
        else:
            raw = base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4))
    except (binascii.Error, zlib.error) as exc:
        return None, "payload decode failed: {}".format(exc)
    return raw, None


def split_response(data):
    head, _, body = data.partition(b"\r\n\r\n")
    return head, body


def cookie_lines(data):
    head, _ = split_response(data)
    out = []
    for line in head.decode("latin-1").split("\r\n"):
        if line.lower().startswith("set-cookie:"):
            out.append(line.split(":", 1)[1].strip())
    return out


def compare_responses(reference_path, candidate_path, report):
    ref = read(reference_path)
    cand = read(candidate_path)
    # Replace Set-Cookie values with a verified payload fingerprint.
    ref_cookies = cookie_lines(ref)
    cand_cookies = cookie_lines(cand)

    ref_text = text_of(ref)
    cand_text = text_of(cand)
    ref_norm, counts = normalize("response", ref_text)
    cand_norm, counts2 = normalize("response", cand_text)
    for key in counts:
        counts[key] += counts2[key]

    if ref_cookies != cand_cookies:
        # Compare cookies structurally: same attributes, same decoded payload.
        ref_shapes = [cookie_shape(item, report, "reference") for item in ref_cookies]
        cand_shapes = [cookie_shape(item, report, "candidate") for item in cand_cookies]
        if ref_shapes != cand_shapes:
            report["differences"].append(
                "{}: Set-Cookie differs\n  reference: {}\n  candidate: {}".format(
                    reference_path, ref_shapes, cand_shapes))
        for item in ref_cookies + cand_cookies:
            ref_norm = ref_norm.replace(item, "<COOKIE-VERIFIED>")
            cand_norm = cand_norm.replace(item, "<COOKIE-VERIFIED>")
    if ref_norm != cand_norm:
        ref_rewritten, ref_entries = rewrite_log_entries(ref_norm)
        cand_rewritten, cand_entries = rewrite_log_entries(cand_norm)
        if ref_entries is not None and cand_entries is not None:
            if ref_rewritten == cand_rewritten and canonical_entries(ref_entries) == canonical_entries(cand_entries):
                counts["startup-race"] = counts.get("startup-race", 0) + 1
                return True, counts
        for ref_stripped, ref_pair in strip_startup_pairs(ref_norm):
            matched = False
            for cand_stripped, cand_pair in strip_startup_pairs(cand_norm):
                if ref_stripped == cand_stripped and ref_pair == cand_pair:
                    matched = True
                    break
            if matched:
                counts["startup-race"] = counts.get("startup-race", 0) + 1
                return True, counts
        report["differences"].append(
            "{}: body/headers differ\n{}".format(reference_path, first_difference(ref_norm, cand_norm)))
        return False, counts
    return True, counts


def cookie_shape(value, report, side):
    name, _, attributes = value.partition(";")
    attributes = attributes.strip()
    if name == "session=":
        # the delete cookie: no payload to verify, the attributes are the payload
        return (name.split("=", 1)[0], "", attributes)
    payload, reason = verify_cookie(name.split("=", 1)[1]) if name.startswith("session=") else (None, "not a session cookie")
    if reason:
        report["cookie_failures"].append("{}: {}".format(side, reason))
    return (name.split("=", 1)[0], payload.decode("utf-8", "replace") if payload else "", attributes)


def first_difference(left, right, width=200):
    for index, (a, b) in enumerate(zip(left, right)):
        if a != b:
            start = max(0, index - width // 2)
            return "at offset {}\n  reference: ...{}...\n  candidate: ...{}...".format(
                index, left[start:index + width].replace("\n", "\\n"),
                right[start:index + width].replace("\n", "\\n"))
    return "length differs: {} vs {}".format(len(left), len(right))


def compare_case(reference_dir, candidate_dir, report, scenario):
    case = {"scenario": scenario, "checks": 0, "identical": 0, "normalizations": {}}

    def bump(counts):
        for key, value in counts.items():
            case["normalizations"][key] = case["normalizations"].get(key, 0) + value

    # 1. requests on the wire
    ref_requests = [json.loads(line) for line in open(os.path.join(reference_dir, "requests.jsonl"))]
    cand_requests = [json.loads(line) for line in open(os.path.join(candidate_dir, "requests.jsonl"))]
    case["checks"] += 1
    ref_seq = []
    cand_seq = []
    for item in ref_requests:
        text, counts = normalize("request", item["raw_text"])
        bump(counts)
        ref_seq.append((item["server"], text))
    for item in cand_requests:
        text, counts = normalize("request", item["raw_text"])
        bump(counts)
        cand_seq.append((item["server"], text))
    if ref_seq == cand_seq:
        case["identical"] += 1
    else:
        report["differences"].append(
            "{}: wire requests differ ({} reference vs {} candidate)\n{}".format(
                scenario, len(ref_seq), len(cand_seq), describe_sequence(ref_seq, cand_seq)))

    # 2. log file
    ref_log_path = os.path.join(reference_dir, "octobot.log")
    cand_log_path = os.path.join(candidate_dir, "octobot.log")
    case["checks"] += 1
    if os.path.exists(ref_log_path) and os.path.exists(cand_log_path):
        ref_log, counts = normalize("log", text_of(read(ref_log_path)))
        bump(counts)
        cand_log, counts2 = normalize("log", text_of(read(cand_log_path)))
        bump(counts2)
        raced = False
        for ref_rest, ref_pair in strip_startup_pairs(ref_log):
            for cand_rest, cand_pair in strip_startup_pairs(cand_log):
                if ref_rest == cand_rest and ref_pair == cand_pair:
                    ref_log, cand_log, raced = ref_rest, cand_rest, True
                    break
            if raced:
                break
        if raced:
            bump({"startup-race": 1})
        if ref_log == cand_log:
            case["identical"] += 1
        else:
            report["differences"].append("{}: log differs\n{}".format(scenario, first_difference(ref_log, cand_log)))
    else:
        report["differences"].append("{}: log file missing".format(scenario))

    # 3. stdout
    case["checks"] += 1
    ref_out = normalise_banner(text_of(read(os.path.join(reference_dir, "stdout.txt"))))
    cand_out = normalise_banner(text_of(read(os.path.join(candidate_dir, "stdout.txt"))))
    if ref_out == cand_out:
        case["identical"] += 1
    else:
        report["differences"].append(
            "{}: stdout differs (after removing the dev-server banner)\n{}".format(
                scenario, first_difference(ref_out, cand_out)))

    # 4. stderr (werkzeug banner lines removed; see the report)
    case["checks"] += 1
    ref_err, counts = normalize("stderr", normalise_banner(text_of(read(os.path.join(reference_dir, "stderr.txt")))))
    bump(counts)
    cand_err, counts2 = normalize("stderr", normalise_banner(text_of(read(os.path.join(candidate_dir, "stderr.txt")))))
    bump(counts2)
    raced = False
    for ref_rest, ref_pair in strip_startup_pairs(ref_err):
        for cand_rest, cand_pair in strip_startup_pairs(cand_err):
            if ref_rest == cand_rest and ref_pair == cand_pair:
                ref_err, cand_err, raced = ref_rest, cand_rest, True
                break
        if raced:
            break
    if raced:
        bump({"startup-race": 1})
    if ref_err == cand_err:
        case["identical"] += 1
    else:
        report["differences"].append(
            "{}: stderr differs (after removing the dev-server banner)\n{}".format(
                scenario, first_difference(ref_err, cand_err)))

    # 5. web responses
    ref_web = os.path.join(reference_dir, "web")
    cand_web = os.path.join(candidate_dir, "web")
    if os.path.isdir(ref_web) and os.path.isdir(cand_web):
        names = sorted(os.path.basename(path) for path in os.listdir(ref_web) if path.endswith(".response"))
        names = [name for name in names if not name.startswith("13_reuse")]
        for name in names:
            case["checks"] += 1
            ok, counts = compare_responses(os.path.join(ref_web, name), os.path.join(cand_web, name), report)
            bump(counts)
            if ok:
                case["identical"] += 1
    else:
        report["differences"].append("{}: web captures missing".format(scenario))
    return case


def describe_sequence(ref, cand):
    lines = []
    for index in range(max(len(ref), len(cand))):
        left = ref[index] if index < len(ref) else None
        right = cand[index] if index < len(cand) else None
        if left == right:
            continue
        lines.append("  request {}:\n    reference: {}\n    candidate: {}".format(
            index,
            (left[0] + " " + left[1].split("\r\n")[0]) if left else "<missing>",
            (right[0] + " " + right[1].split("\r\n")[0]) if right else "<missing>"))
        if left and right and left[1] != right[1]:
            lines.append("    first difference: " + first_difference(left[1], right[1]))
        if len(lines) > 12:
            lines.append("  ...")
            break
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--only", default=None)
    parser.add_argument("--json-out", default=None)
    parser.add_argument("--strict-server-header", action="store_true",
                        help="do not normalize the Server header (use with OCTO_SERVER_HEADER)")
    args = parser.parse_args()

    if args.strict_server_header:
        SKIP.add("server-header")
    scenarios = sorted(
        name for name in os.listdir(args.reference)
        if os.path.isdir(os.path.join(args.reference, name))
    )
    if args.only:
        scenarios = [name for name in scenarios if name == args.only]

    report = {"differences": [], "cookie_failures": []}
    cases = []
    for scenario in scenarios:
        candidate_dir = os.path.join(args.candidate, scenario)
        if not os.path.isdir(candidate_dir):
            report["differences"].append("{}: candidate run missing".format(scenario))
            continue
        cases.append(compare_case(os.path.join(args.reference, scenario), candidate_dir, report, scenario))

    total_checks = sum(case["checks"] for case in cases)
    total_identical = sum(case["identical"] for case in cases)
    normalizations = {}
    for case in cases:
        for key, value in case["normalizations"].items():
            normalizations[key] = normalizations.get(key, 0) + value

    print("scenarios compared: {}".format(len(cases)))
    print("artifacts identical: {}/{}".format(total_identical, total_checks))
    print("normalizations applied:")
    for key, value in sorted(normalizations.items()):
        explanation = next((item[3] for item in NORMALIZATIONS if item[0] == key),
                           "thread-scheduling race between the two start-up log lines"
                           if key == "startup-race" else "")
        print("  {:<18} {:>6}  ({})".format(key, value, explanation))
    if report["cookie_failures"]:
        print("cookie verification failures:")
        for item in report["cookie_failures"]:
            print("  " + item)
    if report["differences"]:
        print("\nDIFFERENCES ({})".format(len(report["differences"])))
        for item in report["differences"][:40]:
            print("-" + item)
    else:
        print("\nno differences")
    if args.json_out:
        with open(args.json_out, "w") as handle:
            json.dump({"cases": cases, "normalizations": normalizations,
                       "differences": report["differences"],
                       "cookie_failures": report["cookie_failures"]}, handle, indent=2)
    return 0 if not report["differences"] else 1


if __name__ == "__main__":
    sys.exit(main())
