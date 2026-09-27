#!/usr/bin/env python3
"""Cross-check: Flask 3.1.0 / itsdangerous 2.2.0 in-process vs the pure-stdlib
reproduction in flash_cookie_repro.py, with the timestamp held fixed.

Run with the app's own interpreter:
  /Users/chaos/octopus-minmax/.venv-py/bin/python scripts/cookie_xcheck.py
"""
import sys, os, importlib.util
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

spec = importlib.util.spec_from_file_location(
    "repro", os.path.join(os.path.dirname(os.path.abspath(__file__)), "flash_cookie_repro.py"))
repro = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repro)

import itsdangerous.timed
from flask import Flask, flash, session

FIXED_TS = 1790536131
itsdangerous.timed.TimestampSigner.get_timestamp = lambda self: FIXED_TS

app = Flask(__name__)
app.secret_key = "octobot-tool"

def flask_flash_cookie(messages):
    """Get the real cookie value the way web_server.py does."""
    with app.test_request_context("/config"):
        for cat, msg in messages:
            flash(msg, cat)
        s = app.session_interface.get_signing_serializer(app)
        return s.dumps(dict(session))

cases = [
    [["success", "Configuration updated successfully! (Will reset on container restart)"]],
    [["error", "Execution time must be in HH:MM format (00:00 to 23:59)"]],
    [["error", "Switch threshold must be positive"]],
    [["error", "Execution time must be in HH:MM format (00:00 to 23:59)"],
     ["error", "Switch threshold must be a number"]],
    # NOT CAPTURED on the wire: no route can flash non-ASCII text. Synthetic only.
    [["error", "pounds \u00a3 euro \u20ac accent \u00e9 quote \" amp & lt < gt >"]],
]
ok = True
for messages in cases:
    real = flask_flash_cookie(messages)
    mine = repro.flash_cookie(messages, FIXED_TS)
    same = real == mine
    ok &= same
    print("case %-60r match=%s" % (messages[0][1][:56], same))
    print("   real: %s" % real)
    print("   mine: %s" % mine)

# also cross-check a non-flash session dict (json tag + sort_keys + ensure_ascii)
import itsdangerous
s = itsdangerous.URLSafeTimedSerializer(["octobot-tool"], salt="cookie-session",
        serializer=None, signer_kwargs={"key_derivation": "hmac",
                                        "digest_method": __import__("hashlib").sha1})
from flask.json.tag import TaggedJSONSerializer
s.serializer = TaggedJSONSerializer()
# TaggedJSONSerializer tags a tuple as {" t": [...]}; repro expects the tagged form.
for d in ({"b": 1, "a": 2}, {"x": "caf\u00e9 \u00a3"}, {"tup": (1, 2)}):
    expect = {"tup": {" t": [1, 2]}} if "tup" in d else d
    real = flask_flash_cookie([]) if False else None
    with app.test_request_context("/"):
        real = app.session_interface.get_signing_serializer(app).dumps(d)
    mine = repro.cookie_value(expect, FIXED_TS)
    same = real == mine
    ok &= same
    print("dict %-30r match=%s" % (d, same))
    if not same:
        print("   real:", real); print("   mine:", mine)

print("XCHECK OK" if ok else "XCHECK FAILED")
sys.exit(0 if ok else 1)
