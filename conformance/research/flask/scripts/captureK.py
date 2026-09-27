
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request
import sys as _s
PORT = int(_s.argv[1]) if len(_s.argv) > 1 else 5050
def allow(method, target):
    r = request(PORT, build(method, target, body=b"", headers=["Content-Length: 0"]))
    for line in r.partition(b"\r\n\r\n")[0].split(b"\r\n"):
        if line.startswith(b"Allow:"):
            return line.decode()
    return "no Allow"
for i in range(3):
    print(PORT, "PUT /config :", allow("PUT", "/config"))
for i in range(3):
    print(PORT, "PUT /logs   :", allow("PUT", "/logs"))
print(PORT, "PUT /       :", allow("PUT", "/"))
