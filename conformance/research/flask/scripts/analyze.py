#!/usr/bin/env python3
"""Post-process the raw captures: header order table, body hashes, and the
group_log_entries results extracted back out of the rendered logs.html."""
import hashlib, html, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"
OUT = ROOT / "analysis"
OUT.mkdir(exist_ok=True)
(OUT / "bodies").mkdir(exist_ok=True)

ENTRY_RE = re.compile(r'<div class="border-b border-gray-800 pb-2 mb-2 whitespace-pre-wrap">(.*?)</div>',
                      re.DOTALL)

rows = []
for p in sorted(CAP.glob("*.raw")):
    raw = p.read_bytes()
    head, sep, body = raw.partition(b"\r\n\r\n")
    if sep:
        lines = head.split(b"\r\n")
        status = lines[0].decode("latin-1")
        headers = [l.decode("latin-1") for l in lines[1:]]
    else:                      # HTTP/0.9 style: body only, no status line
        status = "NO STATUS LINE (HTTP/0.9 style response)"
        headers = []
        body = raw
    (OUT / "bodies" / (p.stem + ".body")).write_bytes(body)
    rows.append((p.stem, status, " | ".join(headers), str(len(body)),
                 hashlib.sha256(body).hexdigest()[:16]))

with open(OUT / "summary.tsv", "w") as f:
    f.write("capture\tstatus_line\theaders_in_order\tbody_bytes\tbody_sha256_16\n")
    for r in rows:
        f.write("\t".join(r) + "\n")

with open(OUT / "log-groups.txt", "w") as f:
    for p in sorted(CAP.glob("*logs*.raw")):
        body = (OUT / "bodies" / (p.stem + ".body")).read_bytes()
        text = body.decode("utf-8", "replace")
        entries = [html.unescape(m) for m in ENTRY_RE.findall(text)]
        f.write("### %s  -> %d entries\n" % (p.stem, len(entries)))
        for i, e in enumerate(entries):
            f.write("  [%d] %r\n" % (i, e))
        f.write("\n")
print("wrote", OUT / "summary.tsv")
print((OUT / "log-groups.txt").read_text())
