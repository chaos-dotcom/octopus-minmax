import signal, json
signal.alarm(60)
from apprise import Apprise
urls = [
 "json://127.0.0.1:18801/notify",
 "jsons://127.0.0.1:18810/notify",
 "form://127.0.0.1:18802/notify",
 "form://127.0.0.1:18802/notify/path",
 "xml://127.0.0.1:18803/notify",
 "xmls://127.0.0.1:18803/notify",
 "discord://123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345",
 "slack://AAA111BBB1/BBB222CCC2/CCC333DDD3/general",
 "slack://xoxb-1234-5678-abcdef/abcd/general",
 "tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321",
 "telegram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321",
 "gotify://127.0.0.1:18804/gotifytoken",
 "gotifys://127.0.0.1:18804/gotifytoken",
 "ntfy://127.0.0.1:18805/topic",
 "ntfy://ntfy.sh/topic",
 "pover://%s@%s" % ("u"*30, "t"*30),
 "pushover://%s@%s" % ("u"*30, "t"*30),
 "mailto://127.0.0.1:18830/octopus@example.com?from=bot@example.com&mode=insecure",
 "mailto://127.0.0.1:18830?to=octopus@example.com&from=bot@example.com&mode=insecure",
 "workflows://127.0.0.1:18801/notify",
 "jsons://discord.com/x",
 "",
 "bad://whatever",
 "json://",
 "http://127.0.0.1:18801/notify",
]
out = []
for u in urls:
    ap = Apprise()
    try:
        r = ap.add(u)
    except Exception as e:
        r = "EXC %s %s" % (type(e).__name__, e)
    srv = [type(s).__name__ + " " + s.url() for s in ap]
    out.append({"url": u, "add": r if isinstance(r, str) else bool(r), "servers": srv})
print(json.dumps(out, indent=1))
