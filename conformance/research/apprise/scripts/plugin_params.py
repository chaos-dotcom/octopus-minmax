import json, signal
signal.alarm(60)
from apprise import Apprise
urls = {
 "json":"json://127.0.0.1:18801/notify","form":"form://127.0.0.1:18802/notify",
 "xml":"xml://127.0.0.1:18803/notify","gotify":"gotify://127.0.0.1:18804/tok",
 "ntfy":"ntfy://127.0.0.1:18805/topic",
 "discord":"discord://123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345",
 "slack":"slack://AAA111BBB1/BBB222CCC2/CCC333DDD3/general",
 "tgram":"tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321",
 "pover":"pover://%s@%s" % ("u"*30,"t"*30),
 "mailto":"mailto://127.0.0.1:18830/a@example.com?from=b@example.com&mode=insecure",
}
for k,u in urls.items():
    ap = Apprise(); ap.add(u); s = list(ap)[0]
    print("%-8s %-22s notify_format=%-9s body_maxlen=%-6s title_maxlen=%-4s amalgamate=%-5s overflow=%s" % (
        k, type(s).__name__, s.notify_format, s.body_maxlen, s.title_maxlen,
        s.overflow_amalgamate_title, s.overflow_mode))
