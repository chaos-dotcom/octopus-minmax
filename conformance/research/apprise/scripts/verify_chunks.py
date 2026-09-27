import json, signal
signal.alarm(60)
from apprise import Apprise
BODY = open("/Users/chaos/octopus-minmax/conformance/research/apprise/bodies.json").read()
b = json.loads(BODY); BODY_LONG=b["BODY_LONG"]; TITLE=b["TITLE"]
cases = {
 "tgram": "tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321?overflow=split",
 "discord": "discord://123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345?overflow=split",
 "pover": "pover://%s@%s?overflow=split" % ("u"*30, "t"*30),
 "json": "json://127.0.0.1:18801/notify?overflow=split",
}
for name, url in cases.items():
    ap = Apprise(); ap.add(url)
    srv = list(ap)[0]
    calls = list(srv._build_send_calls(body=BODY_LONG, title=TITLE,
                                        notify_type='info', body_format='text'))
    print("###", name, "chunks:", len(calls), "body_maxlen=", srv.body_maxlen,
          "title_maxlen=", srv.title_maxlen, "amalgamate=", srv.overflow_amalgamate_title)
    for i, c in enumerate(calls, 1):
        print("   chunk %d: title=%r body_len=%d" % (i, c["title"][:60], len(c["body"])))
        print("      head=%r" % c["body"][:70])
