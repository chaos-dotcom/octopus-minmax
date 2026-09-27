
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap
PORT = 5051
LOGFILE = "/private/tmp/flaskB/logs/octobot.log"
def write(data):
    with open(LOGFILE, "wb") as f: f.write(data)
def G(*a, **k): return build("GET", *a, **k)

# lone CR is a line separator in Python text mode; \x0b \x0c \x85 \u2028 are not
write(b"2024-01-02 03:04:05 line A\rstill A (lone CR)\r2024-01-02 03:04:06 line B\n"
      b"tail with VT \x0b FF \x0c NEL \xc2\x85 LS \xe2\x80\xa8 kept\ntrailing\n")
cap(PORT, "D01_get-logs-lonecr-and-odd-separators", G("/logs"), "lone CR splits; VT/FF/NEL/LS do not")

write(b"\xef\xbb\xbf2024-01-02 03:04:05 BOM at start of file\nnext line\n")
cap(PORT, "D02_get-logs-bom", G("/logs"), "UTF-8 BOM before the first timestamp")

write(b"2024-01-02 03:04:05 a\r\n\r\n2024-01-02 03:04:06 b")
cap(PORT, "D03_get-logs-blank-crlf", G("/logs"), "blank CRLF line between entries")
print("DONE D")
