
import sys, os
sys.path.insert(0, "/Users/chaos/octopus-minmax/conformance/research/flask/scripts")
import wire, traceback
_orig = wire.request
def traced(port, data, **kw):
    first = data.split(b" ")[0]
    if first not in (b"GET", b"POST", b"HEAD", b"OPTIONS", b"PUT", b"DELETE", b"PATCH"):
        print("!!! BAD data arg passed to request():", repr(data[:80]), file=sys.stderr, flush=True)
        traceback.print_stack()
    return _orig(port, data, **kw)
wire.request = traced

import base64
def basic(u="admin", p="admin"):
    return "Basic " + base64.b64encode(f"{u}:{p}".encode()).decode()

def get(path, extra="", ver="HTTP/1.1", host="127.0.0.1:5050", auth=basic()):
    h = [f"GET {path} {ver}", f"Host: {host}"]
    if auth: h.append("Authorization: " + auth)
    h.append("Accept: */*"); h.append("User-Agent: raw-socket/1.0")
    if extra: h.extend(extra.split("\n"))
    return ("\r\n".join(h) + "\r\n\r\n").encode("latin-1")

r1 = wire.request(5050, get("/"))
print("A01 ok", len(r1))
r2 = wire.request(5050, get("/", auth=None))
print("A02 ok", len(r2))
r3 = wire.request(5050, get("/", auth=basic("admin","wrong")))
print("A03 ok", len(r3))
r4 = wire.request(5050, get("/", auth=basic("bob","admin")))
print("A04 ok", len(r4))
r5 = wire.request(5050, b"GET / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic\r\n\r\n")
print("A05 ->", repr(r5[:60]))
