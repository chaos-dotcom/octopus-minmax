
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap
PORT = 5051
def G(*a, **k): return build("GET", *a, **k)
cap(PORT, "G02_get-logs-permission", G("/logs"), "logs/octobot.log mode 000 (running as uid 501/root?)")
