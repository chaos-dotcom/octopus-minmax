
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap, request
PORT = 5050
def G(*a, **k): return build("GET", *a, **k)
cap(PORT, "F01_get-config-hassio-false", G("/config", headers=["X-Hassio-Ingress: false"], auth=None), "X-Hassio-Ingress: false (truthy string) skips auth")
cap(PORT, "F02_get-config-ingress-zero", G("/config", headers=["X-Ingress-Path: 0"], auth=None), "X-Ingress-Path: 0 skips auth")
cap(PORT, "F03_get-config-both-ingress", G("/config", headers=["X-ingress-path: /x", "x-HASSIO-ingress: yes"], auth=None), "lowercase/mixed-case ingress header names")
cap(PORT, "F04_get-config-auth-and-ingress-empty", G("/config", headers=["X-Ingress-Path:"], auth="admin:admin"), "empty ingress header + valid auth")
cap(PORT, "F05_get-root-wrong-auth-ingress", G("/", headers=["X-Ingress-Path: /x"], auth="admin:wrong"), "ingress header wins over bad credentials")
print("DONE F")
