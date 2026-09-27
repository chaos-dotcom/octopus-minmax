
"""Minimal raw HTTP/1.1 server with byte-exact request capture and keep-alive.

Used by the conformance harness to record the exact bytes an implementation puts on
the wire, and to answer with deterministic fixture responses.
"""
import base64, json, socket, threading, time


class Request:
    def __init__(self, method, target, version, header_lines, body):
        self.method = method
        self.target = target
        self.version = version
        self.header_lines = header_lines  # list of (name, value) verbatim
        self.body = body

    def header(self, name):
        low = name.lower()
        for n, v in self.header_lines:
            if n.lower() == low:
                return v
        return None

    def raw(self):
        head = "{} {} {}\r\n".format(self.method, self.target, self.version)
        for n, v in self.header_lines:
            head += "{}: {}\r\n".format(n, v)
        return (head + "\r\n").encode("latin-1") + self.body


class RawServer:
    """Threaded HTTP/1.1 server. handler(req) -> (status, headers, body_bytes)."""

    def __init__(self, port, handler, name, recorder=None):
        self.port = port
        self.handler = handler
        self.name = name
        self.recorder = recorder
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", port))
        self.sock.listen(64)
        self.sock.settimeout(0.4)
        self._stop = threading.Event()
        self.requests = []          # list of dicts (ordered captures)
        self._lock = threading.Lock()
        self._threads = []
        self._accept_thread = threading.Thread(target=self._accept_loop, daemon=True)
        self._accept_thread.start()

    # -- lifecycle -----------------------------------------------------------
    def stop(self):
        self._stop.set()
        try:
            self.sock.close()
        except OSError:
            pass

    def _accept_loop(self):
        while not self._stop.is_set():
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            t = threading.Thread(target=self._serve_conn, args=(conn,), daemon=True)
            t.start()
            self._threads.append(t)

    # -- connection handling -------------------------------------------------
    def _read_line(self, conn, buf):
        while b"\r\n" not in buf[0]:
            chunk = conn.recv(65536)
            if not chunk:
                return None
            buf[0] += chunk
        line, _, rest = buf[0].partition(b"\r\n")
        buf[0] = rest
        return line

    def _read_exact(self, conn, buf, n):
        while len(buf[0]) < n:
            chunk = conn.recv(65536)
            if not chunk:
                break
            buf[0] += chunk
        out, buf[0] = buf[0][:n], buf[0][n:]
        return out

    def _serve_conn(self, conn):
        conn.settimeout(900)   # long idle keep-alive connections are kept open
        buf = [b""]
        try:
            while not self._stop.is_set():
                line = self._read_line(conn, buf)
                if line is None or line == b"":
                    break
                parts = line.decode("latin-1").split(" ")
                if len(parts) < 3:
                    break
                method, target, version = parts[0], parts[1], parts[2]
                header_lines = []
                while True:
                    hl = self._read_line(conn, buf)
                    if hl is None or hl == b"":
                        break
                    name, _, value = hl.decode("latin-1").partition(":")
                    header_lines.append((name, value[1:] if value.startswith(" ") else value))
                length = 0
                for n, v in header_lines:
                    if n.lower() == "content-length":
                        length = int(v)
                body = self._read_exact(conn, buf, length) if length else b""
                req = Request(method, target, version, header_lines, body)
                record = {
                    "server": self.name,
                    "method": method,
                    "target": target,
                    "headers": header_lines,
                    "raw_b64": base64.b64encode(req.raw()).decode("ascii"),
                    "raw_text": req.raw().decode("latin-1"),
                }
                with self._lock:
                    self.requests.append(record)
                if self.recorder is not None:
                    self.recorder(record)
                status, headers, resp_body = self.handler(req)
                if isinstance(resp_body, str):
                    resp_body = resp_body.encode("utf-8")
                head = "HTTP/1.1 {} {}\r\n".format(status, REASON.get(status, "OK"))
                hdrs = list(headers)
                if not any(h[0].lower() == "content-length" for h in hdrs):
                    hdrs.append(("Content-Length", str(len(resp_body))))
                close = False
                for n, v in hdrs:
                    head += "{}: {}\r\n".format(n, v)
                head += "\r\n"
                conn.sendall(head.encode("latin-1") + resp_body)
                if close or version == "HTTP/1.0":
                    break
        except (OSError, ValueError):
            pass
        finally:
            try:
                conn.close()
            except OSError:
                pass


REASON = {200: "OK", 201: "Created", 204: "No Content", 301: "Moved Permanently",
          302: "Found", 400: "Bad Request", 401: "Unauthorized", 403: "Forbidden",
          404: "Not Found", 405: "Method Not Allowed", 500: "Internal Server Error"}


def json_response(obj, status=200):
    return status, [("Content-Type", "application/json")], json.dumps(obj)
