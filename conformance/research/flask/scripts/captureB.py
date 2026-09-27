
"""Run B captures: a web-server-only instance (no bot thread) in a scratch cwd, so
`logs/octobot.log` is fully under test control, port 5051.

Same app object as production (`web_server.run_server()`); the bot thread is the only
thing omitted, and it only appends log lines."""
import sys, os, time, urllib.parse
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request, save, show, split, set_cookie, cap
import flash_cookie_repro as repro

PORT = 5051
LOGDIR = "/private/tmp/flaskB/logs"
LOGFILE = LOGDIR + "/octobot.log"

def G(*a, **k): return build("GET", *a, **k)
def P(path, form, **k): return build("POST", path, body=form.encode("latin-1"), **k)

def write_log(data, name=None):
    os.makedirs(LOGDIR, exist_ok=True)
    if os.path.exists(LOGFILE):
        os.unlink(LOGFILE)
    with open(LOGFILE, "wb") as f:
        f.write(data)
    return LOGFILE

def rm_log():
    if os.path.exists(LOGFILE):
        os.unlink(LOGFILE)

def form_body(d):
    return "&".join("%s=%s" % (k, urllib.parse.quote_plus(str(v), safe="")) for k, v in d.items())

print("=== pristine pages ===")
cap(PORT, "B01_get-index-pristine", G("/"), "GET / on a fresh process")
cap(PORT, "B02_get-config-pristine", G("/config"), "GET /config on a fresh process (no POST yet)")

print("=== group_log_entries edge cases ===")
L1 = (b"first line without any timestamp\n"
      b"    indented continuation of the first line\n"
      b"2024-01-02 03:04:05 - WARNING - first timestamped entry\n"
      b"second line of that entry\n"
      b"third line, & < > \" ' \xc2\xa3 \xc3\xa9\n"
      b"\n"
      b"2024-01-02 03:04:06 - ERROR - second timestamped entry\n"
      b"2024-01-02 03:04:07 - INFO - third entry, no trailing newline at EOF")
write_log(L1)
cap(PORT, "B03_get-logs-edgecase", G("/logs"), "first line untimestamped, multi-line entries, blank line, no final LF, HTML+non-ASCII")

L2 = b"2024-01-02 03:04:05 - X\r\ncontinued\r\n2024-01-02 03:04:06 - Y\r\n"
write_log(L2)
cap(PORT, "B04_get-logs-crlf", G("/logs"), "CRLF line endings")

write_log(b"")
cap(PORT, "B05_get-logs-empty", G("/logs"), "empty log file")

rm_log()
cap(PORT, "B06_get-logs-missing", G("/logs"), "log file does not exist")

write_log(b"\xff\xfe\x00invalid utf-8 \x80\n")
cap(PORT, "B07_get-logs-badutf8", G("/logs"), "log file is not valid UTF-8")

write_log(b"single untimestamped line, no newline")
cap(PORT, "B08_get-logs-single-untimestamped", G("/logs"), "one untimestamped line")

write_log(b"2024-13-99 99:99:99 - FAKE - matches the regex, not a real time\n"
          b" 2024-01-02 03:04:05 - leading space, does not match\n"
          b"2024-01-02 03:04:05\n")
cap(PORT, "B09_get-logs-regex-oddities", G("/logs"), "regex matches impossible dates; leading space does not match")

write_log(b"2024-01-02 03:04:05 start\n" + b"x" * 40 + b"\n")
cap(PORT, "B10_get-logs-long-line", G("/logs"), "long line 40 chars")

print("=== HTML escaping of config values ===")
special = {
    "api_key": "k&<>\"'",
    "acc_number": "A-00000000",
    "base_url": "http://127.0.0.1:9/x?y=1&z=<2>",
    "execution_time": "23:00",
    "switch_threshold": "7",
    "tariffs": "go,agile",
    "notification_urls": "http://a:9/?x=1&y=<2>\nsecond line \"quoted\" 'single' \u00a3\u00e9",
    "dry_run": "on",
    "one_off_run": "on",
    "batch_notifications": "true",
}
body = form_body(special)
print("  form body:", body)
r = cap(PORT, "B11_post-config-special", P("/config", body), "POST /config with &<>\"' and a newline in values")
ck = set_cookie(r)
print("  cookie:", ck)
cap(PORT, "B12_get-config-escaped", G("/config", headers=["Cookie: " + ck.split(";")[0]]),
    "GET /config with the flash cookie: escaped values + flash banner")
cap(PORT, "B13_get-config-escaped-noflash", G("/config"), "GET /config without the cookie (escaped values only)")

print("=== duplicate form fields / empty form ===")
dup = "api_key=first&api_key=second&execution_time=23%3A00&switch_threshold=2&tariffs=go"
cap(PORT, "B14_post-config-duplicate", P("/config", dup), "api_key given twice")
cap(PORT, "B15_get-config-after-duplicate", G("/config"), "GET /config after the duplicate POST")

print("=== session cookie acceptance ===")
now = int(time.time())
cases = [
    ("B16_get-config-cookie-nonflash", repro.cookie_value({"x": 1}, now), "valid cookie with no _flashes"),
    ("B17_get-config-cookie-expired", repro.flash_cookie([["error", "Expired"]], now - 32 * 24 * 3600), "signature timestamp 32 days old (> 31 day max_age)"),
    ("B18_get-config-cookie-wrongkey", repro.cookie_value({"_flashes": [{" t": ["error", "x"]}]}, now, secret_key=b"not-the-key"), "signed with a different key"),
    ("B19_get-config-cookie-badpayload", repro.signed_raw(b"this is not json", now), "valid signature over a non-JSON payload"),
    ("B20_get-config-cookie-empty", "", "session= (empty cookie value)"),
    ("B21_get-config-cookie-garbage", "garbage.value.sig", "unparseable cookie"),
    ("B22_get-config-cookie-permanent", repro.cookie_value({"_permanent": True, "_flashes": [{" t": ["error", "perm"]}]}, now), "cookie naming _permanent"),
]
for name, val, note in cases:
    cap(PORT, name, G("/config", headers=["Cookie: session=" + val]), note)
print("DONE B")
