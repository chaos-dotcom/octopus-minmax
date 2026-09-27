
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap, set_cookie
PORT = 5050
VALID = ("api_key=x&acc_number=A-00000000&base_url=http%3A%2F%2F127.0.0.1%3A9"
         "&execution_time=23%3A00&switch_threshold=2")
bad = VALID.replace("execution_time=23%3A00","execution_time=25%3A00").replace("switch_threshold=2","switch_threshold=abc")
r = cap(PORT, "J01_post-config-two-errors", build("POST", "/config", body=bad.encode()), "two validation errors in one POST")
ck = set_cookie(r).split(";")[0]
cap(PORT, "J02_get-config-two-banners", build("GET", "/config", headers=["Cookie: " + ck]), "GET /config with two flashed messages")
