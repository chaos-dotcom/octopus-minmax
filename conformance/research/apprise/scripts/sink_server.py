"""Raw capture sink for the Apprise byte-spec work.

Life: bounded. The process exits by itself after --max-seconds (default 900)
or on SIGTERM/SIGINT. It never daemonises and it never re-listens after exit.

Usage: python sink_server.py --config config.json --outdir captures/ [--max-seconds 900]
Config: {"listeners": [{"label":"json","proto":"http","port":18801, ...}]}
"""
import argparse
import json
import os
import signal
import socket
import ssl
import sys
import threading
import time

STOP = threading.Event()
LOCK = threading.Lock()
STATE = {"n": 0}


def log(msg):
    sys.stdout.write("%.3f %s\n" % (time.time(), msg))
    sys.stdout.flush()


def next_index():
    with LOCK:
        STATE["n"] += 1
        return STATE["n"]


def recv_request(sock, timeout=5.0):
    """Read exactly one HTTP request. Returns (head_bytes, body_bytes, chunked)."""
    sock.settimeout(timeout)
    buf = b""
    while b"\r\n\r\n" not in buf:
        try:
            chunk = sock.recv(65536)
        except socket.timeout:
            break
        if not chunk:
            break
        buf += chunk
    head, sep, rest = buf.partition(b"\r\n\r\n")
    if not sep:
        return buf, b"", False
    lines = head.split(b"\r\n")
    clen = None
    chunked = False
    for ln in lines[1:]:
        name, _, val = ln.partition(b":")
        n = name.strip().lower()
        if n == b"content-length":
            clen = int(val.strip())
        elif n == b"transfer-encoding" and b"chunked" in val.lower():
            chunked = True
    body = rest
    if chunked:
        # read until the terminating 0-length chunk is complete
        while b"0\r\n\r\n" not in body and not body.endswith(b"0\r\n"):
            try:
                chunk = sock.recv(65536)
            except socket.timeout:
                break
            if not chunk:
                break
            body += chunk
        return head, body, True
    if clen is not None:
        while len(body) < clen:
            try:
                chunk = sock.recv(65536)
            except socket.timeout:
                break
            if not chunk:
                break
            body += chunk
    return head, body, False


def http_conn(sock, addr, cfg, outdir):
    idx = next_index()
    label = cfg["label"]
    try:
        head, body, chunked = recv_request(sock)
    except Exception as e:  # pragma: no cover
        log("ERR recv %s %s" % (label, e))
        sock.close()
        return
    raw = head + (b"\r\n\r\n" if head else b"") + body
    base = os.path.join(outdir, "%s_%03d" % (label, idx))
    with open(base + ".request.txt", "wb") as f:
        f.write(raw)
    meta = {
        "label": label,
        "index": idx,
        "proto": cfg["proto"],
        "local_port": cfg["port"],
        "peer": addr[0] + ":" + str(addr[1]),
        "listener_host_header_expected": cfg.get("host_header"),
        "chunked": chunked,
        "request_bytes": len(raw),
        "head_lines": head.split(b"\r\n")[0].decode("latin-1") if head else "",
        "body_bytes": len(body),
        "raw_len_without_body": len(head) + 4,
        "captured_at": time.time(),
    }
    with open(base + ".meta.json", "w") as f:
        json.dump(meta, f, indent=2, sort_keys=True)
    # respond
    resp = cfg.get("response", {})
    status = resp.get("status", 200)
    reason = resp.get("reason", "OK")
    rbody = resp.get("body", "").encode("utf-8")
    hdrs = resp.get("headers", {})
    out = ("HTTP/1.1 %d %s\r\n" % (status, reason)).encode()
    hdrs = dict(hdrs)
    hdrs.setdefault("Content-Length", str(len(rbody)))
    hdrs.setdefault("Connection", "close")
    for k, v in hdrs.items():
        out += ("%s: %s\r\n" % (k, v)).encode()
    out += b"\r\n" + rbody
    try:
        sock.sendall(out)
    except Exception as e:
        log("ERR send %s %s" % (label, e))
    with open(base + ".response.txt", "wb") as f:
        f.write(out)
    log("HTTP %s idx=%d req=%dB body=%dB" % (label, idx, len(raw), len(body)))
    try:
        sock.close()
    except Exception:
        pass


SMTP_BANNER = b"220 capture.local ESMTP apprise-capture\r\n"


def smtp_conn(sock, addr, cfg, outdir):
    global STOP
    idx = next_index()
    label = cfg["label"]
    transcript = []          # list of (direction, bytes)
    sock.settimeout(cfg.get("timeout", 10.0))

    def send(data):
        transcript.append(("S", data))
        sock.sendall(data)

    def readline():
        buf = b""
        while not buf.endswith(b"\r\n"):
            ch = sock.recv(1)
            if not ch:
                break
            buf += ch
        return buf

    send(SMTP_BANNER)
    in_data = False
    try:
        while not STOP.is_set():
            line = readline()
            if not line:
                break
            transcript.append(("C", line))
            if in_data:
                if line.strip() == b".":
                    in_data = False
                    send(b"250 2.0.0 Ok: queued as CAPTURE\r\n")
                continue
            up = line.strip().upper()
            if up.startswith(b"EHLO") or up.startswith(b"HELO"):
                send(b"250-capture.local\r\n250-SIZE 10240000\r\n"
                     b"250-8BITMIME\r\n250 HELP\r\n")
            elif up.startswith(b"MAIL FROM") or up.startswith(b"RCPT TO"):
                send(b"250 2.1.0 Ok\r\n")
            elif up.startswith(b"DATA"):
                in_data = True
                send(b"354 End data with <CR><LF>.<CR><LF>\r\n")
            elif up.startswith(b"RSET"):
                send(b"250 2.0.0 Ok\r\n")
            elif up.startswith(b"QUIT"):
                send(b"221 2.0.0 Bye\r\n")
                break
            elif up.startswith(b"AUTH"):
                send(b"504 5.5.4 Unrecognized authentication type\r\n")
            elif up.startswith(b"STARTTLS"):
                send(b"454 4.7.0 TLS not available\r\n")
            else:
                send(b"250 2.0.0 Ok\r\n")
    except Exception as e:
        log("ERR smtp %s %s" % (label, e))
    base = os.path.join(outdir, "%s_%03d" % (label, idx))
    blob = b"".join(
        (b">>> " if d == "C" else b"<<< ") + data for d, data in transcript)
    with open(base + ".smtp.txt", "wb") as f:
        f.write(blob)
    meta = {
        "label": label, "index": idx, "proto": "smtp", "local_port": cfg["port"],
        "peer": addr[0] + ":" + str(addr[1]),
        "transcript_bytes": len(blob), "captured_at": time.time(),
        "lines": len(transcript),
    }
    with open(base + ".meta.json", "w") as f:
        json.dump(meta, f, indent=2, sort_keys=True)
    log("SMTP %s idx=%d lines=%d" % (label, idx, len(transcript)))
    try:
        sock.close()
    except Exception:
        pass


def serve(cfg, outdir, tls_ctx):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", cfg["port"]))
    srv.listen(16)
    srv.settimeout(1.0)
    log("LISTEN %s %s port=%d" % (cfg["label"], cfg["proto"], cfg["port"]))
    while not STOP.is_set():
        try:
            conn, addr = srv.accept()
        except socket.timeout:
            continue
        except OSError:
            break
        if cfg["proto"] == "https":
            try:
                conn = tls_ctx.wrap_socket(conn, server_side=True)
            except Exception as e:
                log("ERR tls %s %s" % (cfg["label"], e))
                try:
                    conn.close()
                except Exception:
                    pass
                continue
        t = threading.Thread(
            target=smtp_conn if cfg["proto"] == "smtp" else http_conn,
            args=(conn, addr, cfg, outdir), daemon=True)
        t.start()
    srv.close()
    log("CLOSED %s port=%d" % (cfg["label"], cfg["port"]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", required=True)
    ap.add_argument("--outdir", required=True)
    ap.add_argument("--max-seconds", type=float, default=900.0)
    a = ap.parse_args()
    cfgs = json.load(open(a.config))["listeners"]
    os.makedirs(a.outdir, exist_ok=True)
    tls_ctx = None
    if any(c["proto"] == "https" for c in cfgs):
        tls_ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        d = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                         "certs")
        tls_ctx.load_cert_chain(
            os.path.join(d, "srv.pem"), os.path.join(d, "srv.key"))

    def bye(signum, frame):
        STOP.set()
    signal.signal(signal.SIGTERM, bye)
    signal.signal(signal.SIGINT, bye)
    threading.Timer(a.max_seconds, STOP.set).start()
    threads = [threading.Thread(target=serve, args=(c, a.outdir, tls_ctx),
                                daemon=True) for c in cfgs]
    for t in threads:
        t.start()
    while not STOP.is_set():
        time.sleep(0.5)
    log("WATCHDOG STOP; exiting")
    sys.exit(0)


if __name__ == "__main__":
    main()
