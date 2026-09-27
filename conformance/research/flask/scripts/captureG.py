
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap
PORT = 5051
def G(*a, **k): return build("GET", *a, **k)
cap(PORT, "G01_get-logs-isdir", G("/logs"), "logs/octobot.log is a directory")
