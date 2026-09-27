
"""Raw-socket HTTP capture helper. No HTTP library: we see the exact bytes."""
import socket, os, sys, pathlib, base64

HOST = "127.0.0.1"
CAP = pathlib.Path(__file__).resolve().parent.parent / "captures"


def build(method, path, headers=(), body=None, host="127.0.0.1:5050", ver="HTTP/1.1", auth="admin:admin"):
    """BUILD request bytes only. Never touches the network."""
    h = ["%s %s %s" % (method, path, ver)]
    if host is not None:
        h.append("Host: " + host)
    if auth is not None:
        if auth is True:
            h.append("Authorization: Basic " + base64.b64encode(b"admin:admin").decode())
        elif auth.startswith("Basic "):
            h.append("Authorization: " + auth)
        else:
            h.append("Authorization: Basic " + base64.b64encode(auth.encode("latin-1")).decode())
    if body is not None:
        h.append("Content-Type: application/x-www-form-urlencoded")
        h.append("Content-Length: %d" % len(body))
    h.append("Accept: */*")
    h.append("User-Agent: raw-socket/1.0")
    for x in headers:
        if x == "__noauth__":
            continue
        h.append(x)
    return ("\r\n".join(h) + "\r\n\r\n").encode("latin-1") + (body or b"")


def request(port, data, connect_timeout=5.0, read_timeout=5.0):
    """Send raw bytes, return the raw response bytes (no interpretation)."""
    if isinstance(data, str):
        data = data.encode("latin-1")
    assert not data.startswith(b"HTTP/"), "BUG: response bytes passed as a request"
    s = socket.create_connection((HOST, port), timeout=connect_timeout)
    if os.environ.get("WIRE_DEBUG"):
        print("WIRE SEND lport=%d %r" % (s.getsockname()[1], data[:200]), file=sys.stderr, flush=True)
    s.sendall(data)
    s.settimeout(read_timeout)
    buf = b""
    while b"\r\n\r\n" not in buf and b"\n\n" not in buf:
        try:
            b = s.recv(65536)
        except socket.timeout:
            break
        if not b:
            break
        buf += b
    head, sep, rest = buf.partition(b"\r\n\r\n")
    clen = None
    for line in head.split(b"\r\n")[1:]:
        if line.lower().startswith(b"content-length:"):
            clen = int(line.split(b":", 1)[1].strip())
    if clen is not None:
        while len(rest) < clen:
            try:
                b = s.recv(65536)
            except socket.timeout:
                break
            if not b:
                break
            rest += b
    else:
        while True:
            try:
                b = s.recv(65536)
            except socket.timeout:
                break
            if not b:
                break
            rest += b
    if os.environ.get("WIRE_DEBUG"):
        print("WIRE RECV lport=%d %r" % (s.getsockname()[1], (head + sep + rest)[:120]), file=sys.stderr, flush=True)
    s.close()
    return head + sep + rest


def split(resp):
    head, sep, body = resp.partition(b"\r\n\r\n")
    lines = head.split(b"\r\n")
    return lines[0], lines[1:], body


def save(name, resp, note=""):
    CAP.mkdir(parents=True, exist_ok=True)
    p = CAP / (name + ".raw")
    p.write_bytes(resp)
    print("[saved] %-46s %7d bytes  %s" % (p.name, len(resp), note))
    return p


def show(resp, maxbody=400):
    status, headers, body = split(resp)
    print("  status:", status.decode("latin-1"))
    for h in headers:
        print("  hdr   :", h.decode("latin-1"))
    print("  body  :", repr(body[:maxbody]))
    return status, headers, body


def set_cookie(resp):
    for h in split(resp)[1]:
        if h.lower().startswith(b"set-cookie:"):
            return h.split(b":", 1)[1].strip().decode("latin-1")
    return None


def cap(port, name, req, note=""):
    r = request(port, req)
    if not r.startswith(b"HTTP/1."):
        print("  !! ANOMALY for", name, repr(r[:80]))
    save(name, r, note)
    return r
