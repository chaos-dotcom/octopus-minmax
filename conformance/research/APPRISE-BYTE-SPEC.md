# Apprise 1.9.2 → byte-exact HTTP specification

Measured, not guessed: every request byte below was captured on the wire by a raw TCP sink (`scripts/sink_server.py`) while apprise 1.9.2 made the request.

| item | value |
|---|---|
| apprise | 1.9.2 (`/Users/chaos/octopus-minmax/.venv-py/lib/python3.11/site-packages/apprise`) |
| python | 3.11.15 (`/Users/chaos/octopus-minmax/.venv-py/bin/python`) |
| requests | 2.32.3, urllib3 2.8.0 (these two decide the header order and the framing) |
| sink | raw `socket` server, one TCP listener per scheme, records verbatim bytes |
| captures | `conformance/research/apprise/captures/` (`<label>_<n>.request.txt` = raw bytes) |
| case index | `conformance/research/apprise/captures/case_<case>.json` (case → files, returns, logs) |

Fixed values used for the headline captures:

- `BODY` = `line1\nline2` (11 chars, one LF)
- `TITLE` = `Octopus MinMax Results - Sun 27 Sep 23:00`
- call: `ap = Apprise(); ap.add(url); ap.notify(body=BODY, title=TITLE)` (body_format defaults to `text`, notify_type defaults to `info`)

---


## 0. TL;DR for the Rust implementer

1. Header order is always: `Host`, `User-Agent: Apprise`, `Accept-Encoding: gzip, deflate`, `Accept: */*`,
   `Connection: keep-alive`, plugin headers, `Content-Length`; `Content-Type` is appended **after**
   `Content-Length` when requests generated it (form://, pover://) and sits before it when the plugin set it.
2. CRLF line endings, HTTP/1.1, no compression, no `Expect`, body bytes = the bytes given in the tables.
3. JSON bodies are `json.dumps` defaults: `", "` and `": "` separators, `ensure_ascii=True`, insertion key
   order, no trailing newline.
4. `overflow_mode` is `upstream` by default for every scheme in this document: **no chunking, no
   truncation**. `?overflow=split` / `?overflow=truncate` are opt-in and documented per scheme.
5. The body is `rstrip()`-ed and the title `strip()`-ed exactly once, before any scheme sees them.
6. `discord://` = `title + "\r\n" + body`; `tgram://` = `<b>title</b>\r\n` + escaped body with
   LF→CRLF and TAB→3 spaces; `slack://` keeps title and body in separate JSON fields and adds a
   wall-clock `ts`.
7. Non-reproducible bytes: slack `ts`, mailto MIME boundary / `Message-ID` / `Date` / EHLO hostname.
   Everything else is deterministic given the URL, body and title.
8. `add()` returns `False` if any URL in the list fails to parse, but keeps the valid ones;
   `notify()` returns `True` only if every server succeeded, `False` otherwise (never `None`).
9. `telegram://` and `pushover://` do not exist in 1.9.2 — use `tgram://` and `pover://`.
10. The `apprise` logger has a `NullHandler`: apprise prints nothing unless the caller installs a handler.

## 1. Summary table

| scheme | URL form captured | method | target on the wire | body encoding | chunking with default settings |
|---|---|---|---|---|---|
| `json://` | `json://127.0.0.1:18801/notify` | POST | `/notify` | JSON, `json.dumps` defaults | none (limit 32768) |
| `jsons://` | `jsons://127.0.0.1:18806/notify` | POST | `/notify` | same bytes as `json://` | none |
| `form://` | `form://127.0.0.1:18802/notify` | POST | `/notify` | `application/x-www-form-urlencoded` | none (limit 32768) |
| `xml://` | `xml://127.0.0.1:18803/notify` | POST | `/notify` | SOAP-ish XML template | none (limit 32768) |
| `discord://` | `discord://<id>/<token>` | POST | `/api/webhooks/<id>/<token>` on `discord.com` | JSON | **none by default** (`overflow=upstream`); `?overflow=split` slices at 1947 chars |
| `slack://` | `slack://<a>/<b>/<c>/general` | POST | `/services/<a>/<b>/<c>` on `hooks.slack.com` | JSON (md5-independent, but `ts` = wall clock) | none (limit 35000) |
| `tgram://` | `tgram://<bot_token>/<chat_id>` | POST | `/bot<token>/sendMessage` on `api.telegram.org` | JSON, HTML text | **none by default**; `?overflow=split` slices at 4096 |
| `gotify://` | `gotify://127.0.0.1:18804/<token>` | POST | `/message` | JSON | none (limit 32768) |
| `ntfy://` | `ntfy://127.0.0.1:18805/topic`, `ntfy://ntfy.sh/topic`, `ntfy://ntfy.sh/topic?mode=cloud` | POST | `/` (local + cloud) | JSON (topic in body) | none by default (limit 7800) |
| `pover://` | `pover://<user>/<token>` | POST | `/1/messages.json` on `api.pushover.net` | `application/x-www-form-urlencoded` | none by default; `?overflow=split` slices at 1024 |
| `mailto://` | `mailto://127.0.0.1:18830/a@example.com?from=b@example.com&mode=insecure` | SMTP | `EHLO/MAIL/RCPT/DATA` | MIME `multipart/alternative` | none |

`telegram://` and `pushover://` are **NOT SUPPORTED** in 1.9.2: `add()` returns `False` (only `tgram://` and `pover://` are registered). `http://`, `workflows://` and bare hostnames are not schemes.

---

## 2. The shared HTTP framing (applies to every `requests`-based scheme)

Header order is decided by `requests`/`urllib3`, not by apprise. For a POST with a body and no extra headers the order is always:

1. `Host: <host[:port]>` (port omitted when 80/443)
2. `User-Agent: Apprise` (this is `apprise.NotifyBase.app_id`)
3. `Accept-Encoding: gzip, deflate`
4. `Accept: */*`
5. `Connection: keep-alive`
6. any header the plugin set (plugin `Content-Type`, `X-Gotify-Key`, `X-Icon`, `Accept` … — in the order the plugin's dict was built, which is the order shown per scheme below)
7. `Content-Type: application/x-www-form-urlencoded` **only** when the plugin did not set one and passed a dict (form://, pover://) — requests appends it here, after `Content-Length`
8. `Content-Length: <bytes>`

Line endings are CRLF. No `Expect`, no chunked encoding, no compression is used. HTTP/1.1. Body bytes are UTF-8 unless the body is ASCII-only.

---
## 3. `json://` (and `jsons://`)

URL: `json://<host>[:port]/<path>` → `POST http://<host>:<port>/<path>`. `jsons://` is the same over TLS: `jsons://127.0.0.1:18806/notify` produced byte-identical request bytes (capture `https_local_005.request.txt`).

`add()` → `True`, `notify()` → `True`, INFO `Sent JSON POST notification.`

BODY=`line1\nline2`, TITLE=`Octopus MinMax Results - Sun 27 Sep 23:00` (capture `json_001.request.txt`):

- request line: `POST /notify HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `127.0.0.1:18801` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json` |
| 7 | `Content-Length` | `134` |

- body (134 bytes):

```
{"version": "1.0", "title": "Octopus MinMax Results - Sun 27 Sep 23:00", "message": "line1\nline2", "attachments": [], "type": "info"}
```


Same call with `title=""` (capture `json_002.request.txt`) — the key stays, the value is `""`:

```
{"version": "1.0", "title": "", "message": "line1\nline2", "attachments": [], "type": "info"}
```


Rules, verified in the bytes:

- key order is fixed: `version`, `title`, `message`, `attachments`, `type`
- `version` = `"1.0"`, `attachments` = `[]`, `type` = notify type (`info`)
- the body is `json.dumps(payload)` with the **default** separators `, ` and `: ` (no compact separators)
- `ensure_ascii` is on: `£` → `\u00a3`, `é` → `\u00e9`, 🐙 → surrogate pair `\ud83d\udc19`; quotes/backslashes as usual (`"` → `\"`, `\` → `\\`)
- the body string is `rstrip()`-ed and the title `strip()`-ed before use (a trailing `\n` disappears; see `json_003.request.txt`)
- no trailing newline after the JSON
- credentials: user/password in the URL become `Authorization: Basic …`, which requests appends **after** `Content-Length` (capture `json_basicauth`, header 8)

Length limits: `body_maxlen` = 32768, `title_maxlen` = 250, `overflow` = `upstream` by default, so a 4999-char body is sent in **one** request (5220-byte JSON, capture `json_004.request.txt`). With `?overflow=split` a 300-char title is truncated to 250 chars (capture `json_longtitle_split`).

Other captured URL arguments: `?method=GET` (params go in the request line, body still sent: `GET /notify?q=1 HTTP/1.1`), `?method=PUT`, `?+X-Custom=1` (extra header is placed after `Content-Type`, before `Content-Length`).

---
## 4. `form://`

URL: `form://<host>[:port]/<path>` → `POST http://<host>:<port>/<path>`.

`add()` → `True`, `notify()` → `True`, INFO `Sent Form POST notification.`

- request line: `POST /notify HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `127.0.0.1:18802` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Length` | `93` |
| 7 | `Content-Type` | `application/x-www-form-urlencoded` |

- body (93 bytes):

```
version=1.0&title=Octopus+MinMax+Results+-+Sun+27+Sep+23%3A00&message=line1%0Aline2&type=info
```


Rules:

- key order: `version`, `title`, `message`, `type`
- `quote_plus` encoding: space → `+`, `:` → `%3A`, LF → `%0A`, tab → `%09`, `£` → `%C2%A3`, 🐙 → `%F0%9F%90%99`, `"` → `%22`, `\` → `%5C` (capture `form_esc.request.txt`)
- `Content-Type: application/x-www-form-urlencoded` is added by requests, so it comes **after** `Content-Length` — unlike json:// where the plugin sets it before
- body `rstrip()`-ed; same limits as json:// (32768)
- credentials: user/password → HTTP Basic auth (appended last)
- `?method=GET`: all payload keys move into the query string, `Content-Length: 0` and an empty body (capture `form_get`)

---
## 5. `xml://`

URL: `xml://<host>[:port]/<path>` → `POST http://<host>:<port>/<path>`.

`add()` → `True`, `notify()` → `True`, INFO `Sent XML POST notification.`

- request line: `POST /notify HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `127.0.0.1:18803` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/xml` |
| 7 | `Content-Length` | `601` |

- body (601 bytes):

```
<?xml version='1.0' encoding='utf-8'?>
<soapenv:Envelope
    xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/"
    xmlns:xsd="http://www.w3.org/2001/XMLSchema"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <soapenv:Body>
        <Notification xmlns:xsi="https://raw.githubusercontent.com/caronc/apprise/master/apprise/assets/NotifyXML-1.1.xsd">
            <Version>1.1</Version><Subject>Octopus MinMax Results - Sun 27 Sep 23:00</Subject><Message>line1
line2</Message><MessageType>info</MessageType>
            
       </Notification>
    </soapenv:Body>
</soapenv:Envelope>
```


Rules:

- fixed 601-byte template: prologue `<?xml version='1.0' encoding='utf-8'?>\n`, then a `soapenv:Envelope` that puts each namespace on its own line with 4-space indentation
- `<Notification xmlns:xsi="https://raw.githubusercontent.com/caronc/apprise/master/apprise/assets/NotifyXML-1.1.xsd">` — the XSD URL is fixed bytes
- the four payload elements sit on one line with no separator, in this order: `<Version>1.1</Version><Subject>…</Subject><Message>…</Message><MessageType>info</MessageType>`, then `\n            \n       ` (the whitespace where attachments would go)
- the body ends exactly at `</soapenv:Envelope>` — **no trailing newline**
- escaping: `&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`, `'`→`&apos;`, `"`→`&quot;`; spaces, tabs and newlines are left literal (capture `xml_009.request.txt` shows a raw LF and a raw TAB inside `<Message>`)
- `Content-Type: application/xml` is set by the plugin (before `Content-Length`), unlike form://
- credentials: user/password → HTTP Basic auth

- so in the wire bytes a tab in the body is **3 spaces**, an LF is **CRLF**, and `&amp;` in the input becomes `&amp;amp;` (capture `tgram_esc.request.txt`)
- response must be HTTP 200 (body ignored; a JSON `{"ok":true}` sink response was accepted)

### Splitting (measured)

- Default `overflow=upstream`: **no splitting** — a 4999-char body is ONE request whose `text` is 5169 chars (capture `tgram_021.request.txt`)
- `?overflow=split`: two requests, `body_maxlen` 4096. The first `text` is 3239 chars, the second 1909 chars (the body is hard-sliced mid-word; capture list in `case_tgram_long_split.json`). No `[1/2]` counter is added: `title_maxlen = 0` disables it.

---
## 6. `discord://`

**URL form captured:** `discord://123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345`
**Wire target:** `POST https://discord.com/api/webhooks/123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345` (host is hard-coded by the plugin; TLS, SNI `discord.com`, no query string unless `?thread=`).
`notify()` → `True`; INFO log: `Sent Discord notification.`

Capture `discord_010.request.txt`, BODY=`line1\nline2`, TITLE=`Octopus MinMax Results - Sun 27 Sep 23:00`:

- request line: `POST /api/webhooks/123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345 HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `discord.com` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json; charset=utf-8` |
| 7 | `Content-Length` | `217` |

- body (217 bytes):

```
{"tts": false, "wait": true, "avatar_url": "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-256x256.png", "content": "Octopus MinMax Results - Sun 27 Sep 23:00\r\nline1\nline2"}
```


Title/body combination (verbatim bytes):

| call | key | exact value on the wire |
|---|---|---|
| title present | `content` | `Octopus MinMax Results - Sun 27 Sep 23:00\r\nline1\nline2` (title, **CRLF**, body as-is) |
| `title=""` | `content` | `line1\nline2` (body only, no leading CRLF) — capture `discord_011.request.txt` |

Body of the `title=""` call (172 bytes):

```
{"tts": false, "wait": true, "avatar_url": "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-256x256.png", "content": "line1\nline2"}
```


Rules confirmed from the bytes:

- JSON key order: `tts`, `wait`, `avatar_url`, `content`
- `tts` = `false`, `wait` = `true`, `avatar_url` = the asset image URL for the notify type
- the title is joined with **CRLF** (`\r\n`), while the body's own newlines are left untouched (`\n`)
- `Content-Type: application/json; charset=utf-8` (json:// has no charset)
- escaping: `json.dumps` defaults — `"` → `\"`, `\` → `\\`, `£` → `\u00a3`, 🐙 → `\ud83d\udc19`

Chunking:

| setting | result for a 4999-char body |
|---|---|
| default (`overflow=upstream`) | **1 request**. `content` = 5303 bytes of JSON; nothing is truncated or split (`discord_013.request.txt`) |
| `?overflow=split` | 3 requests; hard character slices of the raw body (1947 / 1947 / 1105 chars), title repeated with a counter ` [i/3]` |
| `?overflow=truncate` | 1 request; `content` = 1990 chars (`discord_005.request.txt`) |

`?overflow=split` details (captures `discord_002`, `discord_003`, `discord_004`):

| request | `content` prefix | slice chars | total `content` chars |
|---|---|---|---|
| 1/3 | `Octopus MinMax Results - Sun 27 Sep 23:00 [1/3]\r\n` | 1947 | 1996 |
| 2/3 | `Octopus MinMax Results - Sun 27 Sep 23:00 [2/3]\r\n` | 1947 | 1996 |
| 3/3 | `Octopus MinMax Results - Sun 27 Sep 23:00 [3/3]\r\n` | 1105 | 1154 |

- slice size = `body_maxlen(2000) − min(len(title)+12, title_maxlen(250), 2000) − overflow_buffer(0)` = 1947
- counter format ` [i/N]`, `N = ceil(len(body)/slice)`; appended to the title, then `content = title + "\r\n" + slice`
- the slices are contiguous and lossless (re-joined they equal the 4999 input chars exactly); they cut mid-word, never at line breaks
- ```md fences appear only with `?format=markdown&fields=yes`, where sections become embed fields (`discord_md.request.txt`); the default `format=text` never adds a code block

---
## 7. `slack://`

**URL form captured:** `slack://AAA111BBB1/BBB222CCC2/CCC333DDD3/general` (webhook mode: first three path tokens, then channel)
**Wire target:** `POST https://hooks.slack.com/services/AAA111BBB1/BBB222CCC2/CCC333DDD3` (TLS, SNI `hooks.slack.com`)
`notify()` → `True`; INFO log: `Sent Slack notification to general.`

Capture `slack_015.request.txt`, BODY=`line1\nline2`, TITLE=`Octopus MinMax Results - Sun 27 Sep 23:00`:

- request line: `POST /services/AAA111BBB1/BBB222CCC2/CCC333DDD3 HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `hooks.slack.com` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `application/json` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json; charset=utf-8` |
| 7 | `Content-Length` | `454` |

- body (454 bytes):

```
{"username": "Apprise", "mrkdwn": true, "attachments": [{"title": "Octopus MinMax Results - Sun 27 Sep 23:00", "text": "line1\nline2", "color": "#3AA3E3", "ts": 1790536311.064704, "footer_icon": "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-72x72.png", "footer": "Apprise"}], "icon_url": "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-72x72.png", "channel": "#general"}
```


Title/body combination: the title is **not** concatenated with the body; it becomes `attachments[0].title` and the body becomes `attachments[0].text`.

| element | value |
|---|---|
| JSON key order | `username`, `mrkdwn`, `attachments`, `icon_url`, `channel` |
| `username` | `Apprise` (or `?botname=`) |
| `attachments[0]` key order | `title`, `text`, `color`, `ts`, `footer_icon`, `footer` |
| `attachments[0].title` | `Octopus MinMax Results - Sun 27 Sep 23:00` (title as given; with `title=""` the key is **omitted**) |
| `attachments[0].text` | body, HTML-escaped by the markdown conversion |
| `attachments[0].color` | `#3AA3E3` for `info` (notify-type dependent) |
| `attachments[0].ts` | `time.time()` — **wall clock, not reproducible** (captured `1790536311.064704`, `1790536314.349945`) |
| `channel` | `#general` (from the URL path token) |

Header peculiarity: the plugin adds `Accept: application/json`, which lands **between** `Connection` and `Content-Type`, i.e. before `Content-Length` — unlike json://.

Chunking:

| setting | result for a 4999-char body |
|---|---|
| default (`overflow=upstream`) | **1 request**, 5541-byte JSON body, no truncation, no counter (`slack_017.request.txt`) |
| `body_maxlen` | 35000, so 4999 chars is far below any limit — there is no observed splitting path |

`title=""` (capture `slack_001.request.txt`, 413-byte body): the key stays with an **empty string** — `"title": ""` — and `ts`/`color`/`text`/footer/`channel`/`username`/`icon_url` are unchanged. `Content-Length` drops from 454 to 413.

---
## 8. `tgram://` (the `telegram://` spelling is NOT supported by 1.9.2)

**URL form captured:** `tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321`
**Wire target:** `POST https://api.telegram.org/bot<bot_token>/sendMessage` (TLS, SNI `api.telegram.org`)
`notify()` → `True`; INFO log: `Sent Telegram notification.`

Capture `tgram_018.request.txt`, BODY=`line1\nline2`, TITLE=`Octopus MinMax Results - Sun 27 Sep 23:00`:

- request line: `POST /bot123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/sendMessage HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `api.telegram.org` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json` |
| 7 | `Content-Length` | `187` |

- body (187 bytes):

```
{"disable_notification": false, "disable_web_page_preview": true, "parse_mode": "HTML", "text": "<b>Octopus MinMax Results - Sun 27 Sep 23:00</b>\r\nline1\r\nline2", "chat_id": 987654321}
```


Title/body combination — the title is blended **into** the text (`title_maxlen = 0`), then the whole text is HTML-escaped and rewritten:

| step | what happens | example bytes |
|---|---|---|
| 1 | base class wraps the title: `<b>TITLE</b><br />\r\n` + body | `<b>Octopus MinMax Results - Sun 27 Sep 23:00</b><br />\r\nline1\nline2` |
| 2 | HTML escape (`&`, `<`, `>`) and `&nbsp;`/`&emsp;` for spaces/tabs | |
| 3 | plugin table: `&nbsp;` → `' '`, `&emsp;` → `'   '`, `<br/>`/`<br />` → **CRLF**, `&apos;` → `'`, `&quot;` → `"` | `<b>Octopus MinMax Results - Sun 27 Sep 23:00</b>\r\nline1\r\nline2` |

Exact wire `text` for the short case:

```
<b>Octopus MinMax Results - Sun 27 Sep 23:00</b>\r\nline1\r\nline2
```

Exact wire `text` for `title=""` (captures `tgram_019.request.txt` and the repeat `tgram_004.request.txt`, 135-byte body) — no `<b>` wrapper, and the body's LF still becomes CRLF:

```
{"disable_notification": false, "disable_web_page_preview": true, "parse_mode": "HTML", "text": "line1\r\nline2", "chat_id": 987654321}
```


Transformation rules (confirmed against `tgram_esc.request.txt`, body `quote " and backslash \ then newline\ntab\there …`):

| input | wire bytes |
|---|---|
| LF (`\n`) | `\r\n` (CRLF) |
| TAB | exactly **3 spaces** (`&emsp;` → `'   '`) |
| `<b>` / `&amp;` in the body | `&lt;b&gt;` / `&amp;amp;` (escaped once by the HTML conversion) |
| `"` in the body | stays `"` (`&quot;` is converted back to a literal quote) |
| trailing spaces | preserved (`trailing space  ` + CRLF in the escaped case) |

JSON key order: `disable_notification` (`false`), `disable_web_page_preview` (`true`), `parse_mode` (`HTML`), `text`, `chat_id` (integer `987654321` for a numeric chat id; a string for `@name`). `Content-Type: application/json` (no charset).

Chunking:

| setting | result for a 4999-char body |
|---|---|
| default (`overflow=upstream`) | **1 request**, `text` = 5169 chars, no counter (`tgram_021.request.txt`) |
| `?overflow=split` | 2 requests, `body_maxlen` = 4096: first `text` **3239 chars**, second **1909 chars**; hard slices of the blended text (they cut mid-word), and **no `[1/2]` counter** because `title_maxlen = 0` disables it (`tgram_006`, `tgram_007`) |

---
## 9. `gotify://`

URL: `gotify://<host>[:port]/<token>` → `POST http://<host>:<port>/message` (the plugin's own path suffix is `message`; `gotifys://` = https).

`add()` → `True`, `notify()` → `True`, INFO `Sent Gotify notification.`

- request line: `POST /message HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `127.0.0.1:18804` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json` |
| 7 | `X-Gotify-Key` | `gotifytoken` |
| 8 | `Content-Length` | `96` |

- body (96 bytes):

```
{"priority": 5, "title": "Octopus MinMax Results - Sun 27 Sep 23:00", "message": "line1\nline2"}
```


- the token is a **header** (`X-Gotify-Key`), never part of the URL or body
- JSON keys in order: `priority` (integer 5 for `normal`), `title`, `message`
- `priority` is credential/URL dependent: `?priority=high` → 8 (see the plugin's map); `0` for low
- JSON escaping identical to json:// (`ensure_ascii`, `, `/`: `)
- `title=""` keeps the key with an empty string: `{"priority": 5, "title": "", "message": "line1\\nline2"}` — capture `gotify_003.request.txt`
- body limit 32768, `overflow=upstream` → no splitting

---
## 10. `ntfy://`

Two different targets, decided by `?mode=` and by the hostname:

| URL | method | request line target | host | body |
|---|---|---|---|---|
| `ntfy://127.0.0.1:18805/topic` | POST | `/` | `127.0.0.1:18805` | JSON (topic in body) |
| `ntfy://ntfy.sh/topic` | POST | `/` | `ntfy.sh` (**plain HTTP, port 80**) | JSON |
| `ntfy://ntfy.sh/topic?mode=cloud` | POST | `/` | `ntfy.sh:443` (**https, SNI `ntfy.sh`**, capture `ntfysh_001`) | JSON |

`add()` → `True`, `notify()` → `True`, INFO `Sent ntfy notification to 'http://ntfy.sh'.`

- request line: `POST / HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `127.0.0.1:18805` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Type` | `application/json` |
| 7 | `X-Icon` | `https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-256x256.png` |
| 8 | `Content-Length` | `99` |

- body (99 bytes):

```
{"topic": "topic", "title": "Octopus MinMax Results - Sun 27 Sep 23:00", "message": "line1\nline2"}
```


- the topic is NOT in the path in this mode: it is the first JSON key (`topic`, `title`, `message`)
- extra headers seen: `X-Icon: <asset image for the notify type>` (only because `?image=yes` is the default; `?image=no` removes it), `?priority=`, `?tags=` → `X-Priority` / `X-Tags`, `?mode=cloud` switches the host/URL
- `ntfy://ntfy.sh/topic` (no `mode=`) is treated as a **private** server: apprise posts to `http://ntfy.sh` in clear text with no TLS — an easy surprise for a Rust port
- cloud mode (`?mode=cloud`) posts to `https://ntfy.sh` with the SAME body bytes and the same header list as the private case, only `Host: ntfy.sh` (no port, so 443) and TLS differ — captured after adding an `ntfy.sh` SAN to the capture certificate: `ntfysh_001.request.txt` (99-byte body), `ntfysh_002.request.txt` (5185-byte body, 4999-char input, still ONE request), `ntfysh_003.request.txt` (209-byte escaped body)
- `body_maxlen` 7800 with `overflow_amalgamate_title` = True, `overflow=upstream` by default
- credentials: `user:password@` → HTTP Basic auth; `?auth=token` puts `Authorization: Bearer <token>`

---
## 11. `pover://` (Pushover; `pushover://` is NOT supported)

URL: `pover://<user_key>@<token>` → `POST https://api.pushover.net/1/messages.json`. `pushover://…` → `add()` returns `False`.

`add()` → `True`, `notify()` → `True`, INFO `Sent Pushover notification to ALL_DEVICES.`

- request line: `POST /1/messages.json HTTP/1.1`
- headers in wire order:

| # | header name | value |
|---|---|---|
| 1 | `Host` | `api.pushover.net` |
| 2 | `User-Agent` | `Apprise` |
| 3 | `Accept-Encoding` | `gzip, deflate` |
| 4 | `Accept` | `*/*` |
| 5 | `Connection` | `keep-alive` |
| 6 | `Content-Length` | `189` |
| 7 | `Content-Type` | `application/x-www-form-urlencoded` |
| 8 | `Authorization` | `Basic dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0Og==` |

- body (189 bytes):

```
token=tttttttttttttttttttttttttttttt&user=uuuuuuuuuuuuuuuuuuuuuuuuuuuuuu&priority=0&title=Octopus+MinMax+Results+-+Sun+27+Sep+23%3A00&message=line1%0Aline2&device=ALL_DEVICES&sound=pushover
```


- body is `application/x-www-form-urlencoded`, key order: `token`, `user`, `priority`, `title`, `message`, `device`, `sound`
- `priority` = `0` for `normal`; `device` = `ALL_DEVICES` when the URL names no device; `sound` = `pushover`
- `Authorization: Basic base64("<token>:")` — apprise sends the **token** as the basic-auth user with an empty password, and requests appends it **after** `Content-Type`/`Content-Length`: the captured value is `Basic dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0Og==` = `tttttttttttttttttttttttttttttt:`
- a title-less call sends `title=Apprise+Notifications` (the app **description**, not `Apprise` and not empty) — capture `pover_002.request.txt`
- `body_maxlen` 1024, `overflow=upstream` by default → one request even for 4999 chars (`pover_027.request.txt`)
- `?overflow=split` → 5 requests of exactly 1024 body chars each (the last 903) with the title counter ` [i/5]` appended to the title (capture list in `case_pover_long_split.json`)

---
## 12. `mailto://` (SMTP, no HTTP)

URL: `mailto://<host>:<port>/<to>?from=<from>&mode=insecure` → SMTP to `<host>:<port>`. Captured conversation (`smtp_028.smtp.txt`, `>>>` = client, `<<<` = server):

```
<<< 220 capture.local ESMTP apprise-capture
>>> ehlo 1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa
<<< 250-capture.local
250-SIZE 10240000
250-8BITMIME
250 HELP
>>> mail FROM:<bot@example.com> size=705
<<< 250 2.1.0 Ok
>>> rcpt TO:<octopus@example.com>
<<< 250 2.1.0 Ok
>>> data
<<< 354 End data with <CR><LF>.<CR><LF>
>>> Content-Type: multipart/alternative; boundary="===============5700105647326046606=="
>>> MIME-Version: 1.0
>>> X-Application: Apprise
>>> Subject: Octopus MinMax Results - Sun 27 Sep 23:00
>>> From: bot@example.com
>>> To: octopus@example.com
>>> Message-ID: <179053636704.18970.14132599375883027974@127.0.0.1>
>>> Date: Sun, 27 Sep 2026 19:12:47 +0000
>>> 
>>> --===============5700105647326046606==
>>> Content-Type: text/plain; charset="utf-8"
>>> MIME-Version: 1.0
>>> Content-Transfer-Encoding: quoted-printable
>>> 
>>> line1
>>> line2
>>> --===============5700105647326046606==
>>> Content-Type: text/html; charset="utf-8"
>>> MIME-Version: 1.0
>>> Content-Transfer-Encoding: quoted-printable
>>> 
>>> line1<br/>line2
>>> --===============5700105647326046606==--
>>> .
<<< 250 2.0.0 Ok: queued as CAPTURE
>>> quit
<<< 221 2.0.0 Bye
```

- `notify()` → `True`, INFO `Sent Email to octopus@example.com`
- the SMTP envelope: `ehlo <local fqdn>` (lower-case verb — smtplib quirk), `mail FROM:<from> size=<n>`, `rcpt TO:<to>`, `data`, `.`, `quit`
- message is `multipart/alternative` with a `text/plain` and a `text/html` part, both `quoted-printable`; header order inside DATA: `Content-Type`, `MIME-Version`, `X-Application: Apprise`, `Subject`, `From`, `To`, `Message-ID`, `Date`
- the HTML part is the TEXT→HTML conversion: LF → `<br/>`, space → `&nbsp;`, TAB → `&emsp;`, `&`→`&amp;`, `<`→`&lt;`, quotes → `&quot;` (capture `smtp_029.smtp.txt`)
- **not reproducible byte-for-byte**: the MIME boundary, the `Message-ID` and the `Date` header are random/clock dependent, and `ehlo` carries the local hostname

---
## 13. JSON / escaping rules (all JSON schemes)

- separators: `, ` (comma + space) and `: ` (colon + space) — `json.dumps` defaults, never compact
- `ensure_ascii=True` (the default): non-ASCII is `\uXXXX`, astral chars are surrogate pairs (`\ud83d\udc19` for 🐙)
- no key sorting anywhere: every payload is a literal dict in the order shown per scheme
- no trailing newline, no indentation
- UTF-8 only for the few values that are not ASCII (mailto SMTP body, form urlencoding)
- apprise `rstrip()`s the body once, before any scheme sees it, so a trailing `\n` never reaches the wire

## 14. `add()` / `notify()` return values and exact log text

| input | `add()` | servers added | `notify()` | exact log lines (logger `apprise`) |
|---|---|---|---|---|
| `""` (empty string) | `False` | 0 | `False` | `ERROR There are no service(s) to notify` |
| `bogus://nope/` (unknown scheme) | `False` | 0 | `False` | `ERROR Unparseable URL bogus://nope/` then `ERROR There are no service(s) to notify` |
| `http://127.0.0.1:18801/notify` | `False` | 0 | `False` | `ERROR Unparseable URL http://127.0.0.1:18801/notify` |
| `json://` (scheme known, host missing) | `False` | 0 | `False` | `ERROR Unparseable JSON URL json://` |
| `telegram://…`, `pushover://…` | `False` | 0 | `False` | `ERROR Unparseable URL telegram://…` (password/token parts masked as `u...u@t...t` in some messages) |
| `[valid, invalid]` in one `add()` | `False` | 1 (the valid one) | `True` | `ERROR Unparseable URL bogus://nope/` then success lines |
| two separate `add()` calls, second invalid | `True` then `False` | 1 | `True` | `ERROR Unparseable URL bogus://nope/` |
| `Apprise().notify()` with no servers | – | 0 | `False` | `ERROR There are no service(s) to notify` |
| valid URL, server down | `True` | 1 | `False` | `WARNING A Connection error occurred sending JSON notification to 127.0.0.1.` (+ DEBUG socket exception) |

`add()` returns `True` only if **every** entry it was given parsed; a mixed list returns `False` yet the good servers stay loaded (so `notify()` still returns `True`). `notify()` returns `False` — never `None` — when there is nothing to notify, because the internal `TypeError` is swallowed in `_create_notify_calls`.

Success lines (INFO) per scheme: `Sent JSON POST notification.`, `Sent Form POST notification.`, `Sent XML POST notification.`, `Sent Discord notification.`, `Sent Slack notification to general.`, `Sent Telegram notification.`, `Sent Gotify notification.`, `Sent ntfy notification to 'http://ntfy.sh'.`, `Sent Pushover notification to ALL_DEVICES.`, `Sent Email to octopus@example.com`.

Apprise does NOT write to stdout/stderr by itself: these lines come from the `apprise` logger, which has only a `NullHandler` unless the caller configures logging. The Rust side never sees them unless it installs a handler.

## 15. Multiple servers in one `notify()`

One HTTP request per server, in the order the servers were added (sequential mode, the default): `json://…/notify` + `form://…/notify` + `gotify://…/<token>` produced exactly 3 requests (`json_032`, `gotify_030`, `form_031` — see `case_multi_three.json`) and `notify()` → `True`. If one of the servers fails, the others are still contacted and `notify()` → `False`: `[json://127.0.0.1:18801/notify, json://127.0.0.1:18899/notify]` sent 1 request and returned `False` (`case_multi_one_fails.json`).

## 16. Credential- and environment-dependent bytes, per scheme

| scheme | FIXED bytes (deterministic, copy verbatim) | credentials / environment dependent |
|---|---|---|
| json/jsons | header names+order, `User-Agent: Apprise`, `Content-Type: application/json`, JSON key order, separators, `", ": "` spacing, `version 1.0`, `attachments []`, ensure_ascii escaping | `Host`, path, `Authorization: Basic …` (from user:password), `Content-Length` |
| form | same framing; `Content-Type` position after `Content-Length`; key order; `quote_plus` rules | `Host`, path, Basic auth, `Content-Length` |
| xml | the whole 601-byte template except the three values; element order; XSD URL; no trailing newline | `Host`, path, Basic auth, `Content-Length`, the Subject/Message values |
| discord | host `discord.com`, path `/api/webhooks/<id>/<token>`, key order, `tts/wait`, `avatar_url` value | webhook id + token (path), `Content-Length`, `Content-Type` charset is fixed |
| slack | host `hooks.slack.com`, path `/services/<a>/<b>/<c>`, key order, `Accept: application/json` | tokens (path), channel, **`ts` = clock**, `Content-Length` |
| tgram | host `api.telegram.org`, path `/bot<token>/sendMessage`, key order, `parse_mode: HTML`, the whole text transformation | bot token (path), chat id (body), `Content-Length` |
| gotify | path `/message`, `X-Gotify-Key` header name, key order, `priority` semantics | host:port, token value, `Content-Length` |
| ntfy | key order (`topic`,`title`,`message`), `X-Icon` asset URL | host:port, topic, cloud vs private target, auth |
| pover | host `api.pushover.net`, path `/1/messages.json`, key order, `device=ALL_DEVICES`, `sound=pushover`, `priority=0` | user key + token (body and Basic header), `Content-Length` |
| mailto | SMTP verbs, header order, MIME structure, `X-Application: Apprise` | MIME boundary, `Message-ID`, `Date`, EHLO hostname, `size=`, subject/body |

Nothing else is random: with the same URL, body and title, two runs produce identical bytes for every scheme except slack's `ts` and the mailto MIME boundary/Message-ID/Date.

## 17. How to reproduce

```sh
cd /Users/chaos/octopus-minmax/conformance/research/apprise
# 1. one raw TCP sink per scheme (bounded: exits by itself after --max-seconds)
/Users/chaos/octopus-minmax/.venv-py/bin/python scripts/sink_server.py \
    --config sink_config.json --outdir captures --max-seconds 300 &

# 2. one case per process, redirected via scripts/intercept.py so fixed hosts
#    (discord.com, hooks.slack.com, api.telegram.org, api.pushover.net, ntfy.sh)
#    land on the sink while Host/SNI/path stay untouched
/Users/chaos/octopus-minmax/.venv-py/bin/python scripts/capture_case.py \
    <case>.json captures captures/case_<name>.json

# 3. all cases, both rounds (starts and stops the sink itself)
/Users/chaos/octopus-minmax/.venv-py/bin/python scripts/run_all.py cases.json run_results.json
/Users/chaos/octopus-minmax/.venv-py/bin/python scripts/run_all.py cases_r2.json run_results_r2.json
```

TLS: `certs/srv.pem` is signed by `certs/ca.pem` with SANs for the intercepted hostnames; the capture runs set `REQUESTS_CA_BUNDLE=certs/ca-bundle.pem` so apprise's normal certificate verification passes.

## 18. NOT CAPTURED (and why)

- `ntfy://ntfy.sh/topic?mode=cloud` (https://ntfy.sh): CAPTURED in a second pass after adding an `ntfy.sh` SAN to the capture certificate — `ntfysh_001`, `ntfysh_002`, `ntfysh_003`. (The first pass failed with `WARNING A Connection error occurred sending ntfy:https://ntfy.sh notification.` because the certificate had no `ntfy.sh` SAN; the failed attempt is kept in `case_ntfy_cloud_short.json` as evidence of what apprise does when TLS fails.)
- slack **bot** mode (`slack://xoxb-…`) — only webhook mode is captured.
- `apprise://` (Apprise API), `workflows://`, `discord://` attachments, `?attach=` uploads, `?method=HEAD`.
- anything requiring a real credential against the real service (all captures are against the local sink).

## 19. Capture index

Every file is `<label>_<n>.request.txt` (verbatim request bytes), `.response.txt` (what the sink answered) and `.meta.json`. The case → files mapping, the return values and the full apprise log for each case are in `captures/case_<case>.json`.

```
discord_esc              notify=True  files=discord_012.request.txt
discord_long             notify=True  files=discord_013.request.txt
discord_long_split       notify=True  files=discord_002.request.txt,discord_003.request.txt,discord_004.request.txt
discord_long_truncate    notify=True  files=discord_005.request.txt
discord_md               notify=True  files=discord_014.request.txt
discord_notitle          notify=True  files=discord_011.request.txt
discord_short            notify=True  files=discord_010.request.txt
err_add_list_one_invalid notify=True  files=json_036.request.txt
err_add_list_two_calls   notify=True  files=json_037.request.txt
err_empty_url            notify=False files=-
err_no_servers           notify=False files=-
err_notify_failure       notify=False files=-
err_unsupported          notify=False files=-
err_url_list_one_invalid notify=True  files=json_035.request.txt
form_esc                 notify=True  files=form_007.request.txt
form_get                 notify=True  files=form_023.request.txt
form_short               notify=True  files=form_006.request.txt
gotify_esc               notify=True  files=gotify_023.request.txt
gotify_notitle           notify=True  files=gotify_003.request.txt
gotify_short             notify=True  files=gotify_022.request.txt
json_basicauth           notify=True  files=json_019.request.txt
json_custom_header       notify=True  files=json_018.request.txt
json_esc                 notify=True  files=json_003.request.txt
json_get                 notify=True  files=json_016.request.txt
json_long                notify=True  files=json_004.request.txt
json_longtitle_split     notify=True  files=json_014.request.txt
json_longtitle_upstream  notify=True  files=json_015.request.txt
json_notitle             notify=True  files=json_002.request.txt
json_put                 notify=True  files=json_017.request.txt
json_short               notify=True  files=json_001.request.txt
jsons_short              notify=True  files=https_local_005.request.txt
mailto_esc               notify=True  files=smtp_029.smtp.txt
mailto_short             notify=True  files=smtp_028.smtp.txt
multi_mixed_invalid      notify=True  files=form_034.request.txt,json_033.request.txt
multi_one_fails          notify=False files=json_020.request.txt
multi_three              notify=True  files=form_031.request.txt,gotify_030.request.txt,json_032.request.txt
multi_two_ok             notify=True  files=json_021.request.txt,json_022.request.txt
ntfy_cloud_esc           notify=True  files=ntfysh_003.request.txt
ntfy_cloud_long          notify=True  files=ntfysh_002.request.txt
ntfy_cloud_mode          notify=True  files=ntfysh_001.request.txt
ntfy_cloud_mode2         notify=True  files=ntfysh_005.request.txt
ntfy_cloud_short         notify=False files=-
ntfy_local_short         notify=True  files=ntfy_024.request.txt
ntfy_sh_http             notify=True  files=ntfysh_http_001.request.txt
pover_esc                notify=True  files=pover_026.request.txt
pover_long               notify=True  files=pover_027.request.txt
pover_long_split         notify=True  files=pover_009.request.txt,pover_010.request.txt,pover_011.request.txt,pover_012.request.txt,pover_013.request.txt
pover_notitle            notify=True  files=pover_002.request.txt
pover_short              notify=True  files=pover_025.request.txt
pushover_alias           notify=False files=-
slack_esc                notify=True  files=slack_016.request.txt
slack_long               notify=True  files=slack_017.request.txt
slack_notitle            notify=True  files=slack_001.request.txt
slack_short              notify=True  files=slack_015.request.txt
telegram_alias           notify=False files=-
tgram_esc                notify=True  files=tgram_020.request.txt
tgram_long               notify=True  files=tgram_021.request.txt
tgram_long_split         notify=True  files=tgram_006.request.txt,tgram_007.request.txt
tgram_md                 notify=True  files=tgram_008.request.txt
tgram_notitle            notify=True  files=tgram_019.request.txt
tgram_notitle2           notify=True  files=tgram_004.request.txt
tgram_short              notify=True  files=tgram_018.request.txt
xml_esc                  notify=True  files=xml_009.request.txt
xml_short                notify=True  files=xml_008.request.txt
```
