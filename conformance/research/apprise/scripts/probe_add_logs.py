import logging, signal, json
signal.alarm(60)
LOG=[]
class C(logging.Handler):
    def emit(self, r): LOG.append((r.levelname, r.getMessage()))
lg = logging.getLogger("apprise"); lg.setLevel(logging.DEBUG); lg.addHandler(C())
from apprise import Apprise
urls = ["", "   ", "bogus://nope/", "http://127.0.0.1:18801/notify", "json://",
        "telegram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321",
        "pushover://uuuuuuuuuuuuuuuuuuuuuuuuuuuuuu@tttttttttttttttttttttttttttttt",
        "json://127.0.0.1:18801/notify"]
for u in urls:
    LOG.clear()
    ap = Apprise()
    r = ap.add(u)
    print(json.dumps({"url": u, "add": bool(r), "n_servers": len(ap),
                      "log": [lv+": "+m for lv,m in LOG if lv != "DEBUG"]}))
