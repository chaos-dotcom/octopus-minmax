
"""Run A captures: the app as production runs it (cwd = repo root, logs/octobot.log present).
Port 5050.  Raw sockets only."""
import sys, os, base64
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import request, save, show, split, set_cookie, basic

PORT = 5050
A = basic()

def get(path, extra="", ver="HTTP/1.1", host="127.0.0.1:5050", auth=A):
    h = [f"GET {path} {ver}", f"Host: {host}"]
    if auth: h.append("Authorization: " + auth)
    h.append("Accept: */*")
    h.append("User-Agent: raw-socket/1.0")
    if extra: h.extend(extra.split("\n"))
    return ("\r\n".join(h) + "\r\n\r\n").encode("latin-1")

def post(path, form, extra="", ver="HTTP/1.1", host="127.0.0.1:5050", auth=A, ct="application/x-www-form-urlencoded"):
    body = form.encode("latin-1")
    h = [f"POST {path} {ver}", f"Host: {host}"]
    if auth: h.append("Authorization: " + auth)
    h.append("Content-Type: " + ct)
    h.append(f"Content-Length: {len(body)}")
    h.append("User-Agent: raw-socket/1.0")
    if extra: h.extend(extra.split("\n"))
    return ("\r\n".join(h) + "\r\n\r\n").encode("latin-1") + body

def cap(name, req, note=""):
    for attempt in range(4):
        print("  cap %s attempt %d req=%r" % (name, attempt, req[:60]), file=sys.stderr, flush=True)
        r = request(PORT, req)
        if r.startswith(b"HTTP/1."):
            break
        print("  !! anomaly attempt %d: %r" % (attempt, r[:80]))
    else:
        print("  !! GIVING UP on", name)
    save(name, r, note)
    return r

VALID = ("api_key=x&acc_number=A-00000000&base_url=http%3A%2F%2F127.0.0.1%3A9"
         "&execution_time=23%3A00&switch_threshold=2&tariffs=go%2Cagile%2Cflexible"
         "&notification_urls=&batch_notifications=false")

# --- authentication on / ---------------------------------------------------
cap("A01_get-root-auth",       get("/"), "GET / with admin:admin")
cap("A02_get-root-noauth",     get("/", auth=None), "GET / no Authorization")
r = cap("A03_get-root-badauth",get("/", auth=basic("admin","wrong")), "GET / wrong password")
cap("A04_get-root-user-noauth",get("/", auth=basic("bob","admin")), "GET / wrong username")
cap("A05_get-root-empty-scheme",request(PORT, b"GET / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic\r\n\r\n"), "malformed Basic")
print("DONE4")
