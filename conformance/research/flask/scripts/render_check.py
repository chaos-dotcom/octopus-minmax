#!/usr/bin/env python3
"""Prove that each captured HTML body is exactly what Jinja2 renders from src/templates
with the documented context (no Flask-specific extras, no whitespace surprises).

Run with the app interpreter:
  /Users/chaos/octopus-minmax/.venv-py/bin/python scripts/render_check.py
"""
import pathlib, sys, re, html
from flask import Flask, render_template

ROOT = pathlib.Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"
SRC = pathlib.Path("/Users/chaos/octopus-minmax/src")

app = Flask("web_server", template_folder=str(SRC / "templates"))
app.secret_key = "octobot-tool"

def body_of(name):
    raw = (CAP / (name + ".raw")).read_bytes()
    return raw.partition(b"\r\n\r\n")[2]

BASE = dict(api_key="x", acc_number="A-00000000", base_url="http://127.0.0.1:9",
            execution_time="23:00", switch_threshold=2, tariffs="go,agile,flexible",
            one_off_run=True, one_off_executed=False, dry_run=False,
            notification_urls="", batch_notifications=False)

SPECIAL = dict(api_key="k&<>\"'", acc_number="A-00000000",
               base_url="http://127.0.0.1:9/x?y=1&z=<2>", execution_time="23:00",
               switch_threshold=7, tariffs="go,agile",
               notification_urls="http://a:9/?x=1&y=<2>\nsecond line \"quoted\" 'single' \u00a3\u00e9",
               one_off_run=True, one_off_executed=False, dry_run=True, batch_notifications=True)

ENTRY_RE = re.compile(r'<div class="border-b border-gray-800 pb-2 mb-2 whitespace-pre-wrap">(.*?)</div>', re.DOTALL)

def entries_from(name):
    return [html.unescape(m) for m in ENTRY_RE.findall(body_of(name).decode("utf-8"))]

CASES = [
    # capture, template, context, log-source capture (None unless logs.html), flashes
    ("A01_get-root-auth", "index.html", {}, None, []),
    ("B02_get-config-pristine", "config.html", {"config": BASE}, None, []),
    ("B13_get-config-escaped-noflash", "config.html", {"config": SPECIAL}, None, []),
    ("B12_get-config-escaped", "config.html", {"config": SPECIAL}, None,
     [("success", "Configuration updated successfully! (Will reset on container restart)")]),
    ("B03_get-logs-edgecase", "logs.html", {}, "B03_get-logs-edgecase", []),
    ("B05_get-logs-empty", "logs.html", {}, "B05_get-logs-empty", []),
    ("D01_get-logs-lonecr-and-odd-separators", "logs.html", {},
     "D01_get-logs-lonecr-and-odd-separators", []),
]

ok = True
for capname, tpl, ctx, log_from, flashes in CASES:
    with app.test_request_context("/"):
        from flask import flash
        for cat, msg in flashes:
            flash(msg, cat)
        ctx = dict(ctx)
        if tpl == "logs.html":
            ctx["log_entries"] = entries_from(log_from)
        rendered = render_template(tpl, **ctx).encode("utf-8")
    got = body_of(capname)
    same = rendered == got
    ok &= same
    print("%-42s %-12s rendered=%6d captured=%6d match=%s" %
          (capname, tpl, len(rendered), len(got), same))
    if not same:
        for i, (a, b) in enumerate(zip(rendered, got)):
            if a != b:
                print("   first diff at byte", i, repr(rendered[i-40:i+40]), "vs", repr(got[i-40:i+40]))
                break
        else:
            print("   one is a prefix of the other")
print("RENDER CHECK OK" if ok else "RENDER CHECK FAILED")
sys.exit(0 if ok else 1)
