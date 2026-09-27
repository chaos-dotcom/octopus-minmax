
import sys, os, base64
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request, cap
PORT = 5050
def G(*a, **k): return build("GET", *a, **k)
A = "admin:admin"
cap(PORT, "E01_get-root-gzip", G("/", headers=["Accept-Encoding: gzip, deflate"]), "Accept-Encoding: gzip")
cap(PORT, "E02_get-root-if-modified", G("/", headers=["If-None-Match: \"x\"", "If-Modified-Since: Wed, 21 Oct 2015 07:28:00 GMT"]), "conditional GET")
cap(PORT, "E03_get-root-accept-html", G("/", headers=["Accept: text/html", "Accept-Language: en-GB,en;q=0.9"]), "Accept headers")
cap(PORT, "E04_get-root-long-url", build("GET", "/" + "a"*300, auth=None), "unknown path of 300 chars")
cap(PORT, "E05_get-root-long-url-70k", build("GET", "/" + "a"*70000, auth=None), "request target > 64 KiB")
cap(PORT, "E06_get-index-cookie-other", G("/", headers=["Cookie: other=1; another=2"]), "unrelated cookies on /")
print("DONE E")
