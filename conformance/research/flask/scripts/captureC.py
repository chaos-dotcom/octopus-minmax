
"""Run C: remaining probes against the production-shaped instance on port 5050."""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request, save, show, split, set_cookie, cap

PORT = 5050
def G(*a, **k): return build("GET", *a, **k)
def P(path, form, **k): return build("POST", path, body=form.encode("latin-1"), **k)

VALID = ("api_key=x&acc_number=A-00000000&base_url=http%3A%2F%2F127.0.0.1%3A9"
         "&execution_time=23%3A00&switch_threshold=2&tariffs=go%2Cagile%2Cflexible"
         "&notification_urls=&batch_notifications=false")

cap(PORT, "C01_head-config", build("HEAD", "/config"), "HEAD /config -> headers of the config page")
cap(PORT, "C02_get-config-query", G("/config?x=1&y=2"), "GET /config with a query string")
cap(PORT, "C03_get-root-lowercase-basic", build("GET", "/", auth="Basic " + __import__("base64").b64encode(b"admin:admin").decode()), "Basic token, note header name case")
cap(PORT, "C04_post-config-lowercase-auth-header",
    b"POST /config HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nauthorization: basic YWRtaW46YWRtaW4=\r\n"
    b"Content-Type: application/x-www-form-urlencoded\r\nContent-Length: " + str(len(VALID)).encode() + b"\r\n\r\n" + VALID.encode(),
    "lowercase header name and lowercase 'basic' scheme")
cap(PORT, "C05_post-config-json-ct", build("POST", "/config", body=b'{"a":1}', headers=["Content-Type: application/json"]),
    "POST /config with Content-Type: application/json")
cap(PORT, "C06_get-config-after-json", G("/config"), "config after the JSON POST (booleans reset)")
cap(PORT, "C07_post-config-multipart",
    b"POST /config HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n"
    b"Content-Type: multipart/form-data; boundary=XX\r\nContent-Length: 118\r\n\r\n"
    b"--XX\r\nContent-Disposition: form-data; name=\"execution_time\"\r\n\r\n23:00\r\n--XX--\r\n",
    "POST /config multipart/form-data")
cap(PORT, "C08_get-logs-head", build("HEAD", "/logs"), "HEAD /logs")
cap(PORT, "C09_get-config-trailing-dot", G("/config."), "GET /config. -> 404")
cap(PORT, "C10_get-static-dir", G("/static/"), "GET /static/ -> 404")
cap(PORT, "C11_get-empty-path", b"GET  HTTP/1.1\r\nHost: 127.0.0.1:5050\r\n\r\n", "GET with an empty target")
cap(PORT, "C12_bad-request-line", b"NOTAMETHOD / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\n\r\n", "unknown method")
cap(PORT, "C13_put-root", b"PUT / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nContent-Length: 0\r\n\r\n", "PUT / -> 405")
cap(PORT, "C14_trace-root", b"TRACE / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n", "TRACE /")
cap(PORT, "C15_no-host-header", b"GET / HTTP/1.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n", "GET / with no Host header")
cap(PORT, "C16_bad-host", b"GET / HTTP/1.1\r\nHost: a b c\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n", "bad Host header")
print("DONE C")
