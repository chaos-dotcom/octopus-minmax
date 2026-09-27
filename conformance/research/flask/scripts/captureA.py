
"""Run A captures: the app as production runs it (cwd = repo root, logs/octobot.log present), port 5050.
Raw sockets only; the response bytes saved here are the source of truth."""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request, save, show, split, set_cookie, cap

PORT = 5050

VALID = ("api_key=x&acc_number=A-00000000&base_url=http%3A%2F%2F127.0.0.1%3A9"
         "&execution_time=23%3A00&switch_threshold=2&tariffs=go%2Cagile%2Cflexible"
         "&notification_urls=&batch_notifications=false")

def G(*a, **k): return build("GET", *a, **k)
def P(path, form, **k): return build("POST", path, body=form.encode("latin-1"), **k)

# --- authentication on / ---------------------------------------------------
cap(PORT, "A01_get-root-auth",        G("/"), "GET / with admin:admin")
cap(PORT, "A02_get-root-noauth",      G("/", auth=None), "GET / no Authorization")
cap(PORT, "A03_get-root-badauth",     G("/", auth="admin:wrong"), "GET / wrong password")
cap(PORT, "A04_get-root-user-noauth", G("/", auth="bob:admin"), "GET / wrong username")
cap(PORT, "A05_get-auth-empty-basic", build("GET", "/", auth="Basic", host="127.0.0.1:5050"), "Authorization: Basic (no token)")
cap(PORT, "A05b_get-auth-badtoken",   build("GET", "/", auth="Basic !!!!notbase64!!!!"), "Authorization: Basic with invalid base64")
cap(PORT, "A05c_get-auth-no-colon",   G("/", auth="admin"), "Basic token without colon")
cap(PORT, "A05d_get-auth-empty-pw",   G("/", auth="admin:"), "Basic admin with empty password")
# --- config ----------------------------------------------------------------
cap(PORT, "A06_get-config-auth",      G("/config"), "GET /config with auth")
cap(PORT, "A07_get-config-ingress",   G("/config", headers=["X-Ingress-Path: /api/hassio_ingress/abc123"], auth=None), "GET /config ingress path, no auth")
cap(PORT, "A08_get-config-hassio",    G("/config", headers=["X-Hassio-Ingress: true"], auth=None), "GET /config X-Hassio-Ingress, no auth")
cap(PORT, "A08b_get-config-ingress-empty", G("/config", headers=["X-Ingress-Path:"], auth=None), "GET /config empty X-Ingress-Path, no auth")
cap(PORT, "A08c_get-root-ingress",    G("/", headers=["X-Ingress-Path: /x"], auth=None), "GET / ingress header, no auth")
cap(PORT, "A09_get-config-noauth",    G("/config", auth=None), "GET /config no auth")
# --- logs ------------------------------------------------------------------
cap(PORT, "A10_get-logs-auth",        G("/logs"), "GET /logs with auth, logs/octobot.log present")
# --- unknown paths / methods ----------------------------------------------
cap(PORT, "A11_get-404",              G("/nope"), "GET /nope")
cap(PORT, "A12_head-root",            build("HEAD", "/"), "HEAD /")
cap(PORT, "A13_post-root-405",        P("/", "a=b"), "POST / -> 405")
cap(PORT, "A14_get-favicon",          G("/favicon.ico"), "GET /favicon.ico")
cap(PORT, "A15_get-static",           G("/static/style.css"), "GET /static/style.css")
cap(PORT, "A16_options-root",         build("OPTIONS", "/"), "OPTIONS /")
cap(PORT, "A17_post-logs-405",        P("/logs", "a=b"), "POST /logs -> 405")
cap(PORT, "A17b_post-404",            P("/nope", "a=b"), "POST /nope -> 404")
cap(PORT, "A18_get-config-slash-404", G("/config/"), "GET /config/ (trailing slash)")
cap(PORT, "A19_get-root-double-slash",G("//"), "GET //")
cap(PORT, "A19b_get-root-dotdot",     G("/../"), "GET /../")
# --- protocol variations ---------------------------------------------------
cap(PORT, "A20_get-root-http10",      G("/", ver="HTTP/1.0"), "GET / HTTP/1.0")
cap(PORT, "A21_get-root-close",       G("/", headers=["Connection: close"]), "GET / Connection: close")
cap(PORT, "A22_get-root-host-other",  G("/", host="example.com"), "Host: example.com")
cap(PORT, "A23_get-root-fwd",         G("/", headers=["X-Forwarded-Proto: https", "X-Forwarded-Host: fwd.example", "X-Forwarded-For: 10.0.0.9"]), "X-Forwarded-* on GET /")
cap(PORT, "A24_get-root-http10-host", G("/", ver="HTTP/1.0", host="example.com:8080"), "HTTP/1.0 Host: example.com:8080")
cap(PORT, "A24b_post-config-http10",  P("/config", VALID, ver="HTTP/1.0"), "POST /config HTTP/1.0")
cap(PORT, "A24c_post-config-host",    P("/config", VALID, host="example.com"), "POST /config Host: example.com")
cap(PORT, "A24d_post-config-fwd",     P("/config", VALID, headers=["X-Forwarded-Host: fwd.example", "X-Forwarded-Proto: https"]), "POST /config with X-Forwarded-*")
cap(PORT, "A24e_post-config-script",  P("/config", VALID, headers=["X-Script-Name: /prefix"]), "POST /config with X-Script-Name")
# --- POST /config ----------------------------------------------------------
r = cap(PORT, "A25_post-config-valid", P("/config", VALID), "POST /config valid form")
ck = set_cookie(r); print("  cookie:", ck)
cap(PORT, "A26_get-config-flash",     G("/config", headers=["Cookie: " + ck.split(";")[0]]), "GET /config with the success flash cookie")
r = cap(PORT, "A27_post-config-bad-time", P("/config", VALID.replace("execution_time=23%3A00","execution_time=25%3A00")), "POST /config execution_time=25:00")
ck2 = set_cookie(r); print("  cookie:", ck2)
cap(PORT, "A28_get-config-flash-err", G("/config", headers=["Cookie: " + ck2.split(";")[0]]), "GET /config with the error flash cookie")
cap(PORT, "A28b_get-config-flash-twice", G("/config", headers=["Cookie: " + ck2.split(";")[0]]), "same cookie again (flash already consumed)")
cap(PORT, "A28c_get-config-badcookie", G("/config", headers=["Cookie: session=garbage.value.sig"]), "tampered cookie")
cap(PORT, "A29_post-config-bad-num",  P("/config", VALID.replace("switch_threshold=2","switch_threshold=abc")), "switch_threshold=abc")
cap(PORT, "A30_post-config-negative", P("/config", VALID.replace("switch_threshold=2","switch_threshold=-5")), "switch_threshold=-5")
cap(PORT, "A31_post-config-both-bad", P("/config", VALID.replace("execution_time=23%3A00","execution_time=25%3A00").replace("switch_threshold=2","switch_threshold=abc")), "both invalid")
cap(PORT, "A32_post-config-noauth",   P("/config", VALID, auth=None), "POST /config without auth")
cap(PORT, "A33_post-config-empty-body", build("POST", "/config", body=None), "POST /config with no body at all")
cap(PORT, "A34_post-config-empty-form", P("/config", ""), "POST /config with empty form")
cap(PORT, "A35_post-config-missing-fields", P("/config", "api_key=z"), "POST /config with only api_key")
cap(PORT, "A36_post-config-unicode", P("/config", "api_key=x&execution_time=23%3A00&switch_threshold=2&tariffs=go"), "POST /config minimal valid")
print("DONE A")
