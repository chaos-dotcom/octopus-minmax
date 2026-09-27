"""One apprise capture case: add a URL, notify(), collect the raw sink files.

Invoked as:  python capture_case.py  <case.json>  <captures_dir>  <out.json>
case.json:
  {"case": "...", "urls": ["json://..."], "body": "...", "title": "...",
   "log_level": "DEBUG", "extra": {...}}
Prints a JSON summary line prefixed by "RESULT ".
"""
import json
import logging
import os
import signal
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
HERE = os.path.dirname(os.path.abspath(__file__))
CAP = os.path.join(HERE, "..", "captures")

# A case may pick its own host->port map before urllib3 is patched, so that the
# same hostname can be captured on port 80 or port 443 in different cases.
_spec_file = json.load(open(sys.argv[1]))
if _spec_file.get("capture_map"):
    os.environ["APPRISE_CAPTURE_MAP"] = json.dumps(_spec_file["capture_map"])

import intercept  # noqa: E402  (patches urllib3 connect target)

signal.alarm(120)

import apprise  # noqa: E402
from apprise import Apprise, NotifyFormat, NotifyType  # noqa: E402

LOG = []


class Collector(logging.Handler):
    def emit(self, record):
        try:
            LOG.append({
                "level": record.levelname,
                "logger": record.name,
                "msg": record.getMessage(),
            })
        except Exception:
            pass


def main():
    spec = _spec_file
    out_path = sys.argv[3]
    logger = logging.getLogger("apprise")
    logger.setLevel(getattr(logging, spec.get("log_level", "DEBUG")))
    logger.addHandler(Collector())

    seen = set(os.listdir(CAP))
    ap = Apprise()
    add_results = []
    urls = spec["urls"]
    for u in urls:
        if spec.get("add_as_list"):
            add_results.append(("LIST", ap.add(list(u))))
            continue
        try:
            add_results.append((u, ap.add(u)))
        except Exception as e:
            add_results.append((u, "EXCEPTION:%s:%s" % (type(e).__name__, e)))
    # apprise caches; expose the parsed server list
    servers = []
    for s in ap:
        try:
            servers.append({"url": s.url(), "class": type(s).__name__})
        except Exception as e:
            servers.append({"url": "ERR:%s" % e, "class": type(s).__name__})

    ok = None
    exc = None
    t0 = time.time()
    try:
        ok = ap.notify(
            body=spec.get("body", ""),
            title=spec.get("title", ""),
            notify_type=spec.get("notify_type", "info"),
            body_format=spec.get("body_format", "text"),
        )
    except Exception as e:  # keep the capture useful even on a raise
        exc = "%s: %s" % (type(e).__name__, e)
    dt = time.time() - t0

    # wait for the sink to flush its files
    new = []
    for _ in range(20):
        time.sleep(0.15)
        cur = set(os.listdir(CAP))
        new = sorted(cur - seen)
        files = [f for f in new if not f.endswith(".meta.json")]
        if files and all(os.path.exists(os.path.join(CAP, f[:-12] + ".meta.json"))
                         for f in files if f.endswith((".request.txt", ".response.txt", ".smtp.txt"))):
            break

    summary = {
        "case": spec["case"],
        "urls": urls,
        "body_chars": len(spec.get("body", "")),
        "title": spec.get("title", ""),
        "add": [[str(a), (b if isinstance(b, str) else bool(b))]
                for a, b in add_results],
        "servers": servers,
        "notify_return": ok,
        "notify_exception": exc,
        "notify_seconds": round(dt, 4),
        "connect_redirects": list(intercept.LOG),
        "new_files": new,
        "log": LOG,
    }
    with open(out_path, "w") as f:
        json.dump(summary, f, indent=2, sort_keys=True)
    sys.stdout.write("RESULT " + json.dumps(
        {"case": spec["case"], "add": summary["add"],
         "notify_return": ok, "exception": exc,
         "new_files": new, "log_tail": LOG[-4:]}, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
