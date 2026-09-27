import json, signal, inspect
signal.alarm(60)
from apprise.conversion import convert_between, text_to_html
from apprise.common import NotifyFormat
from apprise import Apprise
b = json.loads(open("/Users/chaos/octopus-minmax/conformance/research/apprise/bodies.json").read())
BODY_LONG=b["BODY_LONG"]; TITLE=b["TITLE"]
esc = convert_between(NotifyFormat.TEXT, NotifyFormat.HTML, BODY_LONG)
print("text_to_html len:", len(esc))
print("source of text_to_html:", inspect.getsource(text_to_html))
print("head:", repr(esc[:120]))
ap = Apprise(); ap.add("tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321?overflow=split")
srv = list(ap)[0]
calls = list(srv._build_send_calls(body=BODY_LONG, title=TITLE, notify_type="info", body_format="text"))
c1, c2 = calls[0]["body"], calls[1]["body"]
print("c1 len", len(c1), "c2 len", len(c2))
print("c1 == amalgam[:4096]?", c1 == ("<b>%s</b><br />\r\n" % TITLE) + esc [:4096-56])
print("c1[-40:]", repr(c1[-40:]))
print("c2[:40]", repr(c2[:40]))
# where does c2 come from?
print("c2 in BODY_LONG at", BODY_LONG.find(c2[:40]))
print("c2 in esc at", esc.find(c2[:40]))
