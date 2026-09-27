
import sys, os, base64
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, request, save, show, split, set_cookie, cap
PORT = 5050
def G(*a, **k): return build("GET", *a, **k)

def raw(method, target, headers=(), body=b"", ver="HTTP/1.1"):
    h = [f"{method} {target} {ver}"] + list(headers)
    return ("\r\n".join(h) + "\r\n\r\n").encode("latin-1") + body

BND = "XX"
parts = (b"--XX\r\nContent-Disposition: form-data; name=\"execution_time\"\r\n\r\n23:00\r\n"
         b"--XX\r\nContent-Disposition: form-data; name=\"switch_threshold\"\r\n\r\n5\r\n--XX--\r\n")
cap(PORT, "C07_post-config-multipart",
    raw("POST", "/config", [f"Host: 127.0.0.1:5050",
         "Authorization: Basic " + base64.b64encode(b"admin:admin").decode(),
         f"Content-Type: multipart/form-data; boundary={BND}",
         f"Content-Length: {len(parts)}"], parts),
    "POST /config multipart/form-data")
cap(PORT, "C17_bad-version", raw("GET", "/", ["Host: 127.0.0.1:5050"], ver="HTTP/9x"), "malformed HTTP version")
cap(PORT, "C18_bad-syntax", b"GET /\r\n\r\n", "request line with no version (2 tokens)")
cap(PORT, "C19_garbage", b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03", "TLS ClientHello, not HTTP")
cap(PORT, "C20_header-no-colon", raw("GET", "/", ["Host: 127.0.0.1:5050", "Authorization: Basic YWRtaW46YWRtaW4=", "NoColonHeader"]), "header line without a colon")
cap(PORT, "C21_absolute-uri", raw("GET", "http://127.0.0.1:5050/", ["Host: 127.0.0.1:5050", "Authorization: Basic YWRtaW46YWRtaW4="]), "absolute-form request target")
cap(PORT, "C22_absolute-uri-otherhost", raw("GET", "http://example.com/config", ["Host: 127.0.0.1:5050", "Authorization: Basic YWRtaW46YWRtaW4="]), "absolute-form with a different host")
cap(PORT, "C23_space-in-target", raw("GET", "/a%20b", ["Host: 127.0.0.1:5050", "Authorization: Basic YWRtaW46YWRtaW4="]), "percent-encoded space")
cap(PORT, "C24_post-config-body-extra", raw("POST", "/config", ["Host: 127.0.0.1:5050", "Authorization: Basic YWRtaW46YWRtaW4=", "Content-Type: application/x-www-form-urlencoded", "Content-Length: 10"], b"execution_time=23:00"), "Content-Length larger than the body")
print("DONE C2")
