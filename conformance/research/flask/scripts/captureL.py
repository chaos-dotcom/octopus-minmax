
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request
import sys as _s
PORT = int(_s.argv[1]) if len(_s.argv) > 1 else 5050
def hdr(method, target):
    r = request(PORT, build(method, target))
    out = []
    for line in r.partition(b"\r\n\r\n")[0].split(b"\r\n"):
        if line.startswith(b"Allow:") or line.startswith(b"HTTP/"):
            out.append(line.decode())
    return " / ".join(out)
print(PORT, "OPTIONS /      :", hdr("OPTIONS", "/"))
print(PORT, "OPTIONS /config:", hdr("OPTIONS", "/config"))
