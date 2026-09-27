# FLASK-BYTE-SPEC — byte-exact HTTP surface of `octopus-minmax` `src/web_server.py`

Target reader: someone writing a Rust reimplementation that must emit **byte-identical** HTTP
responses (status line, header order/case/values, body bytes) and reproduce the flash/session cookie.

* Source of truth: **raw-socket captures**, `conformance/research/flask/captures/*.raw`
  (status line + every header + blank line + body, exactly as they came off the wire).
* Reference implementation: Flask 3.1.0 / Werkzeug 3.1.9 / Jinja2 3.1.6 / itsdangerous 2.2.0 /
  MarkupSafe 3.0.3 / Python 3.11.15, in `/Users/chaos/octopus-minmax/.venv-py`.
* Every value in this document was measured, not guessed. Anything not measured is labelled
  `NOT CAPTURED`.
* Nothing under `src/` was modified.

---

## 1. How the reference was produced (reproduce this, then diff)

Three server instances, all real app code, all killed after the captures:

| run | working directory | entry point | port | why |
|---|---|---|---|---|
| A, C, E, F, I, J | `/Users/chaos/octopus-minmax` | `.venv-py/bin/python -u src/main.py` (= production shape: bot thread + web thread) | 5050 | the real thing, `logs/octobot.log` written by the bot |
| B, D | `/private/tmp/flaskB` (scratch) | `.venv-py/bin/python -u -c "import web_server; web_server.run_server()"` with `PYTHONPATH=/Users/chaos/octopus-minmax/src` | 5051 | `cwd` is scratch, so `logs/octobot.log` is fully controlled; the omitted bot thread only appends log lines |
| G | `/private/tmp/flaskB2` (scratch) | idem | 5051 | `logs/octobot.log` replaced by a directory, then by a mode-000 file |

Extra short-lived instances on ports 5052–5056 (fresh scratch `cwd`, idem) were used only to show
that the `Allow` method order changes per process (`analysis/allow-order.txt`).

Environment for both (only `WEB_PORT` and the `cwd` differ):

```
TZ=Europe/London ONE_OFF=true API_KEY=x ACC_NUMBER=A-00000000 \
BASE_URL=http://127.0.0.1:9 NOTIFICATION_URLS= WEB_PORT=5050
```

Capture method: `socket.create_connection()` + `sendall()` of a hand-written request, then read the
response bytes verbatim (`conformance/research/flask/scripts/wire.py`). No HTTP client library is
involved, so nothing is normalized. **The raw captures are the source of truth**; `curl -sv --raw -D -`
output was used only as a cross-check.

Result of `scripts/render_check.py` (Jinja render of `src/templates/*.html` vs captured bytes):

```
A01_get-root-auth            index.html   rendered=  2028 captured=  2028 match=True
B02_get-config-pristine      config.html  rendered=  6749 captured=  6749 match=True
B13_get-config-escaped-*     config.html  rendered=  6881 captured=  6881 match=True
B12_get-config-escaped       config.html  rendered=  7155 captured=  7155 match=True
B03_get-logs-edgecase        logs.html    rendered=  2366 captured=  2366 match=True
B05_get-logs-empty           logs.html    rendered=  1589 captured=  1589 match=True
D01_get-logs-lonecr-*        logs.html    rendered=  1933 captured=  1933 match=True
```

---

## 2. Wire rules that apply to every response

R1. **The status line is always `HTTP/1.1 …`** — even when the request line says `HTTP/1.0`
    (`captures/A20_get-root-http10.raw`). Werkzeug's dev server sets `protocol_version = "HTTP/1.1"`.

R2. **Header order is generated, not sorted.** Werkzeug's `send_response()` writes `Server` then
    `Date`, then every response header in the order the WSGI header list carries them, then
    `Connection: close`. To match byte-for-byte, a Rust server must emit exactly the sequences in
    §4; alphabetical sorting will not match.

R3. `Server: Werkzeug/3.1.9 Python/3.11.15` — literally `Werkzeug/<werkzeug version> Python/<python version>`.

R4. `Date: Sun, 27 Sep 2026 19:08:51 GMT` — RFC 1123, always `GMT`, always English day/month
    abbreviations, day zero-padded with a space (`%a, %d %b %Y %H:%M:%S GMT`). **Environment
    dependent** (clocks).

R5. **`Connection: close` is always present**, on every response, including errors, even when the
    client sends `Connection: keep-alive` and even on `HTTP/1.0`. Werkzeug's dev server never keeps
    a connection alive ("Always close the connection. This disables HTTP/1.1 keep-alive").
    It is the *last* header on every normal response; on the 414 page it comes *before*
    `Content-Type` (see §4, block H8).

R6. Header names are sent in the exact case listed here; values are sent exactly as listed.
    Line endings are CRLF; the body starts immediately after `CRLF CRLF`.
    There is no trailing whitespace and no final extra CRLF after the body.

R7. Every in-scope body is UTF-8; `Content-Type` is `text/html; charset=utf-8` (note the space after
    `;`) for all Flask/werkzeug responses. The only exception is the http.server-generated
    414 page: `text/html;charset=utf-8` (no space) — see block H8.

R8. `Content-Length` is always present (never `Transfer-Encoding: chunked`) because every body is a
    materialized `bytes` object. For `HEAD` the same `Content-Length` is sent as for `GET` and the
    body is empty (`captures/C01_head-config.raw` → `Content-Length: 6764`, 0 body bytes).
    `OPTIONS /` → `Content-Length: 0` with an empty body (`captures/A16_options-root.raw`).

R9. **No other headers ever appear.** Specifically: no `X-Powered-By`, no `Cache-Control`, no
    `ETag`, no `Last-Modified`, no `Content-Encoding` (an `Accept-Encoding: gzip` request still gets
    the plain body — `captures/E01_get-root-gzip.raw`), no HSTS, no `Expect-CT`.

R10. **Malformed request lines produce a body-only "HTTP/0.9 style" response**: no status line and
    no headers at all, just body bytes (the Python `http.server` error page itself uses LF-only line
    endings). This happens
    when the request line has 2 tokens, or the version token is invalid, because Python's
    `http.server` still holds `request_version = "HTTP/0.9"` while it formats the error
    (`captures/C11_get-empty-path.raw`, `C17_bad-version.raw`, `C18_bad-syntax.raw`).

R11. A non-HTTP byte stream (e.g. a TLS ClientHello) gets **no response at all**; the connection is
    closed (`captures/C19_garbage.raw`, 0 bytes).

---

## 3. Routing table (what matches what)

| request target | method | result | capture |
|---|---|---|---|
| `/` | GET | 200 index | `A01_get-root-auth.raw` |
| `//` | GET | 200 index (werkzeug's `Map` defaults to `merge_slashes=True`) | `A19_get-root-double-slash.raw` |
| `/../` | GET | 404 | `A19b_get-root-dotdot.raw` |
| `/config` | GET | 200 config form | `A06_get-config-auth.raw` |
| `/config?x=1&y=2` | GET | 200 config form (query ignored) | `C02_get-config-query.raw` |
| `/config` | POST | 302 → `Location: config` | `A25_post-config-valid.raw` |
| `/config/` | GET | 404 (no trailing-slash redirect) | `A18_get-config-slash-404.raw` |
| `/config.` | GET | 404 | `C09_get-config-trailing-dot.raw` |
| `/logs` | GET | 200 logs page | `A10_get-logs-auth.raw` |
| `/nope`, `/favicon.ico`, `/static/style.css`, `/static/` | GET | 404 (the app has no `static/` folder, so no static route exists) | `A11`, `A14`, `A15`, `C10` |
| `/` | HEAD | 200, no body | `A12_head-root.raw` |
| `/config` | HEAD | 200, no body | `C01_head-config.raw` |
| `/` | OPTIONS | 200, `Allow: GET, HEAD, OPTIONS`, empty body | `A16_options-root.raw` |
| `/` | POST / PUT / TRACE / `NOTAMETHOD` | 405 | `A13`, `C13`, `C14`, `C12` |
| `/logs` | POST | 405 | `A17_post-logs-405.raw` |
| `/nope` | POST | 404 (404 wins over 405) | `A17b_post-404.raw` |
| `/` | GET, request target > 64 KiB | 414 (from Python's http.server, not Flask) | `E05_get-root-long-url-70k.raw` |

Auth rule for `/`, `/config` (GET+POST), `/logs`: **skip** the check when the request carries a
non-empty `X-Ingress-Path` **or** `X-Hassio-Ingress` header (any case of the header name; any
non-empty value, so `X-Hassio-Ingress: false` and `X-Ingress-Path: 0` also skip auth); otherwise
require `Authorization: Basic …` with username exactly `WEB_USERNAME` (default `admin`) and password
exactly `WEB_PASSWORD` (default `admin`). An empty header value does **not** skip auth
(`A08b_get-config-ingress-empty.raw` → 401).

401 is returned for: missing header, wrong user, wrong password, empty password, scheme without
token, invalid base64, token without a colon (`A02`–`A05d`, `A09`, `A32`).

---

## 4. Header blocks (verbatim, in wire order)

`<N>` marks the only per-response variable bytes. Bodies are in §5.

### H1 — `200 OK` for `/`, `/config` (GET), `/logs`, and all `HEAD`/`OPTIONS` variants

```
HTTP/1.1 200 OK
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: <len(body)>
Connection: close
```
`OPTIONS /` inserts `Allow: GET, HEAD, OPTIONS` between `Content-Type` and `Content-Length`:
```
HTTP/1.1 200 OK
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Allow: GET, HEAD, OPTIONS
Content-Length: 0
Connection: close
```
(`captures/A16_options-root.raw`; `Allow` comes before `Content-Length`, because Flask's
`make_default_options_response()` builds an empty response and sets `Allow` first, and only then
does werkzeug append the auto-computed `Content-Length`.)

**The `Allow` value's method *order* is not byte-stable** — it is the iteration order of a Python
`set`, seeded per process. Run A always produced `Allow: GET, HEAD, OPTIONS`, but fresh processes
produced `GET, OPTIONS, HEAD` and `HEAD, GET, OPTIONS` (evidence:
`analysis/allow-order.txt`). Pick one order for the Rust port and compare `Allow` as a set.

### H1v — `200 OK` when the request carries a session cookie that was **read** (flashes consumed)

```
HTTP/1.1 200 OK
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: <len(body)>
Vary: Cookie
Set-Cookie: session=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; HttpOnly; Path=/
Connection: close
```
`Vary` comes before `Set-Cookie`, both after `Content-Length`, both before `Connection`.
Examples: `A26_get-config-flash.raw`, `A28_get-config-flash-err.raw`, `B12_get-config-escaped.raw`.

### H1p — `200 OK` when the loaded session is non-empty, unchanged, but permanent

```
HTTP/1.1 200 OK
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: 7038
Vary: Cookie
Set-Cookie: session=eyJfcGVybWFuZW50Ijp0cnVlfQ.arlqMw.NzpGh3rX7qSACQdBxRVe7fQrvVM; Expires=Wed, 28 Oct 2026 19:10:43 GMT; HttpOnly; Path=/
Connection: close
```
(`B22_get-config-cookie-permanent.raw`; `Expires` = response time + 31 days. `HttpOnly` before
`Path`, and `Expires` between the value and `HttpOnly`.)

### H2 — `302 FOUND` (POST `/config`)

```
HTTP/1.1 302 FOUND
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: 199
Location: config
Vary: Cookie
Set-Cookie: session=<value>; HttpOnly; Path=/
Connection: close
```
`Location` is **relative** (`config`, no leading slash, no scheme, no host) in every captured variant:
plain, `HTTP/1.0`, `Host: example.com`, `X-Forwarded-Host`/`X-Forwarded-Proto`, `X-Script-Name`
(`A24b`–`A24e`). Flask sets `Response.autocorrect_location_header = False`, so werkzeug's
absolute-location rewrite never runs. A Rust server must hard-code `Location: config`.

### H3 — `401 UNAUTHORIZED` (any protected route)

```
HTTP/1.1 401 UNAUTHORIZED
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
WWW-Authenticate: Basic realm="OctoBot Login Required"
Content-Type: text/html; charset=utf-8
Content-Length: 23
Connection: close
```
`WWW-Authenticate` is the app's own header, so it precedes Flask's `Content-Type`.
Realm string includes the double quotes.

### H4 — `404 NOT FOUND`

```
HTTP/1.1 404 NOT FOUND
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: 207
Connection: close
```

### H5 — `405 METHOD NOT ALLOWED`

```
HTTP/1.1 405 METHOD NOT ALLOWED
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Allow: GET, HEAD, OPTIONS
Content-Length: 153
Connection: close
```
`Allow` differs per route in *content*: `GET, HEAD, OPTIONS` for `/` and `/logs`, and
`POST, GET, HEAD, OPTIONS` for `/config` (`captures/I02_delete-logs-405.raw`,
`captures/I01_put-config-405.raw`). The **order inside `Allow` is process-random** (Python `set`
iteration order over the matched methods — measured orders include `POST, GET, HEAD, OPTIONS`,
`HEAD, GET, POST, OPTIONS`, `POST, GET, OPTIONS, HEAD`, `GET, OPTIONS, HEAD, POST`; see
`analysis/allow-order.txt`). It is stable within one process.

### H6 — `500 INTERNAL SERVER ERROR`

```
HTTP/1.1 500 INTERNAL SERVER ERROR
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Content-Type: text/html; charset=utf-8
Content-Length: 265
Connection: close
```
Reachable: a cookie that is correctly signed but whose payload is not JSON makes
`flask.sessions.open_session` raise `itsdangerous.exc.BadPayload` (only `BadSignature` is caught),
which leaves `ctx.session = None` and then blows up in `save_session` →
`AttributeError: 'NoneType' object has no attribute 'accessed'` (`B19_get-config-cookie-badpayload.raw`).
The traceback goes to the server log only, never to the response.

### H7 — `200 OK` normal case, header names/values summary

| header | value | notes |
|---|---|---|
| `Server` | `Werkzeug/<werkzeug> Python/<python>` | env dependent |
| `Date` | `%a, %d %b %Y %H:%M:%S GMT` | env dependent |
| `Content-Type` | `text/html; charset=utf-8` | fixed |
| `Content-Length` | decimal byte length of the body | fixed per body |
| `Vary` | `Cookie` | only when a session cookie was read |
| `Set-Cookie` | see §8 | time dependent |
| `Allow` | `GET, HEAD, OPTIONS[, POST]` | 405/OPTIONS only |
| `Location` | `config` | 302 only |
| `WWW-Authenticate` | `Basic realm="OctoBot Login Required"` | 401 only |
| `Connection` | `close` | always, last |

### H8 — `414 Request-URI Too Long` (Python `http.server`, not Flask)

```
HTTP/1.1 414 Request-URI Too Long
Server: Werkzeug/3.1.9 Python/3.11.15
Date: <RFC1123 GMT>
Connection: close
Content-Type: text/html;charset=utf-8
Content-Length: 327
```
Note the mixed-case status message (`Request-URI Too Long`, not upper-cased) and
`text/html;charset=utf-8` without the space. Body is the `http.server` error page (§5.9).
---

## 5. Bodies

All bodies end **without** a trailing extra newline beyond what is shown; byte counts and SHA-256
(16 hex chars) let a Rust port assert equality. Full bodies are also on disk under
`conformance/research/flask/analysis/bodies/<capture>.body`.

| body | bytes | sha256[:16] | produced by |
|---|---|---|---|
| index (`/`) | 2028 | `b3c1001d2417d321` | `render_template('index.html')` |
| config, pristine defaults (`ONE_OFF=true`) | 6749 | `f7b97d8a412d2798` | `render_template('config.html', config=...)` |
| config, run A before its first POST (`one_off_run=False`) | 6742 | `c2f21ba9c9e2272d` | idem |
| config, escaped values, no flash | 6881 | `872c94f7a3792c5f` | idem |
| config, escaped values + success flash | 7155 | `c74767d9810d62ee` | idem |
| config, two error flashes | 7220 | `fb49df4573d718e3` | `J02_get-config-two-banners.raw` |
| logs, empty group list | 1589 | `ce5b9bde53f5bbe5` | `render_template('logs.html', log_entries=[])` |
| 401 | 23 | `0fcfb12dfad07ba0` | app literal `'Authentication required'` |
| 302 redirect page | 199 | `275acde84da62fa1` | werkzeug `redirect()` |
| 404 page | 207 | `e9639e3c4681ce85` | werkzeug `NotFound` |
| 405 page | 153 | `bfb286554b24db87` | werkzeug `MethodNotAllowed` |
| 500 page | 265 | `ae5163256b944013` | werkzeug `InternalServerError` + dev-server fallback |
| 414 page (http.server) | 327 | `06d2ac9c02602133` | Python `http.server.send_error` |
| 400 page (http.server, body-only response) | 363 | `0e9f1eb497607a0c` | Python `http.server.send_error` |

### 5.1 `GET /` — exact body (2028 bytes)

```
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Octopus MinMax Bot</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {
            darkMode: 'class',
        }
    </script>
</head>
<body class="bg-gray-900 text-gray-100 min-h-screen">
    <nav class="bg-gray-800 border-b border-gray-700 px-6 py-4">
        <div class="max-w-7xl mx-auto">
            <h1 class="text-2xl font-bold text-blue-400">🐙 Octopus MinMax Bot</h1>
        </div>
    </nav>

    <main class="max-w-7xl mx-auto px-6 py-8">
        <!-- Flash messages -->
        
            
        

        
<div class="flex flex-col items-center justify-center min-h-[60vh]">
    <h2 class="text-3xl font-bold mb-12 text-gray-100">Dashboard</h2>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-8 w-full max-w-4xl">
        <!-- Config Button -->
        <a href="config" class="group">
            <div class="bg-gray-800 hover:bg-gray-700 border-2 border-gray-600 hover:border-blue-500 rounded-2xl p-12 text-center transition-all duration-200 transform hover:scale-105">
                <div class="text-6xl mb-4">⚙️</div>
                <h3 class="text-2xl font-bold text-gray-100 group-hover:text-blue-400">Configuration</h3>
                <p class="text-gray-400 mt-2">Edit bot settings</p>
            </div>
        </a>

        <!-- Logs Button -->
        <a href="logs" class="group">
            <div class="bg-gray-800 hover:bg-gray-700 border-2 border-gray-600 hover:border-blue-500 rounded-2xl p-12 text-center transition-all duration-200 transform hover:scale-105">
                <div class="text-6xl mb-4">📋</div>
                <h3 class="text-2xl font-bold text-gray-100 group-hover:text-blue-400">Logs</h3>
                <p class="text-gray-400 mt-2">View bot activity</p>
            </div>
        </a>
    </div>
</div>

    </main>
</body>
</html>
```

### 5.2 `base.html` flash-banner block (exact bytes)

`base.html` is rendered with the default Jinja2 environment (`trim_blocks=False`,
`lstrip_blocks=False`, `autoescape=True`), so every tag line keeps its surrounding whitespace.
Between `<!-- Flash messages -->` and `{% block content %}` the output bytes are exactly:

* **no messages** (42 whitespace bytes between the comment and the block tag):
```
<!-- Flash messages -->
        
            
        

        

```
* **one message**, `success`:
```
<!-- Flash messages -->
        
            
                
                    <div class="mb-6 p-4 rounded-lg bg-green-900 border border-green-700 text-green-100">
                        <ESCAPED MESSAGE>
                    </div>
                
            
        

        

```
* **one message**, any other category (the app only ever flashes `error`):
  identical except `bg-red-900`, `border-red-700`, `text-red-100`.
* **N messages**: the per-message unit repeats once per message — it is the `\n` + 16 spaces +
`<div …>` + `\n` + 24 spaces + `<ESCAPED MESSAGE>` + `\n` + 20 spaces + `</div>` + `\n` + 16 spaces shown here:

```
                
                    <div class="mb-6 p-4 rounded-lg bg-red-900 border border-red-700 text-red-100">
                        <ESCAPED MESSAGE>
                    </div>

```
  (verified with two errors: `J02_get-config-two-banners.raw`.)

Category rule: `category == 'error'` → red; **everything else** → green.

### 5.3 `config.html`

Rendered from `src/templates/config.html`. Substitution points (all escaped, §6):

| template | value written |
|---|---|
| `value="{{ config.api_key }}"` | `config.API_KEY` |
| `value="{{ config.acc_number }}"` | `config.ACC_NUMBER` |
| `value="{{ config.base_url }}"` | `config.BASE_URL` |
| `value="{{ config.execution_time }}"` | `config.EXECUTION_TIME` |
| `value="{{ config.switch_threshold }}"` | `config.SWITCH_THRESHOLD` rendered by `str()` (int) |
| `value="{{ config.tariffs }}"` | `config.TARIFFS` |
| `<textarea …>{{ config.notification_urls }}</textarea>` | `config.NOTIFICATION_URLS`, newlines kept verbatim |
| `{% if config.dry_run %}checked{% endif %}` | literal `checked` (no spaces around it) or nothing |
| `{% if config.one_off_run %}checked{% endif %}` | ditto |
| `{% if config.batch_notifications %}checked{% endif %}` | ditto |

Exact rendering of a checkbox line, in both states:

```
…<input type="checkbox" id="dry_run" name="dry_run" checked
                           class="…">
…<input type="checkbox" id="dry_run" name="dry_run" 
                           class="…">
```
i.e. `checked` immediately after `name="dry_run"`, and when unchecked the space before the newline
is kept (`name="dry_run" 
`). `one_off_executed` and `one_off_run`-only fields are not shown.

The measured byte slices are:

```
B02 (fresh process, ONE_OFF=true -> one_off_run=True, dry_run=False):
  <input type="checkbox" id="dry_run" name="dry_run" \n                           class="…
  <input type="checkbox" id="one_off_run" name="one_off_run" checked\n                           class="…
A06 (same process after a POST reset one_off_run to False):
  <input type="checkbox" id="one_off_run" name="one_off_run" \n                           class="…
```
The only difference between the 6749-byte `B02` body and the 6742-byte `A06` body is that 7-byte
`checked` (`A06_get-config-auth.raw` was captured after run A's first POST had already forced
`one_off_run=False`).

### 5.4 `logs.html`

`src/templates/logs.html`, with one repeated unit per entry (leading 20 spaces, then the div):

```
                \n                    <div class="border-b border-gray-800 pb-2 mb-2 whitespace-pre-wrap"><ESCAPED ENTRY></div>\n
```
`<ESCAPED ENTRY>` keeps every `\n` inside it (CSS `whitespace-pre-wrap`), so a multi-line log entry
is one `<div>`. `<div>` sequences are concatenated with no other separator. The page ends with
`</script>\n\n    </main>\n</body>\n</html>` and no trailing newline.

### 5.5–5.9 Fixed bodies, verbatim

302 (199 bytes; note `<html lang=en>`, the escaped relative target appears twice — as the `href`
and as the link text):
```
<!doctype html>
<html lang=en>
<title>Redirecting...</title>
<h1>Redirecting...</h1>
<p>You should be redirected automatically to the target URL: <a href="config">config</a>. If not, click the link.
```
404 (207 bytes):
```
<!doctype html>
<html lang=en>
<title>404 Not Found</title>
<h1>Not Found</h1>
<p>The requested URL was not found on the server. If you entered the URL manually please check your spelling and try again.</p>
```
405 (153 bytes; `Allow` differs but the body does not):
```
<!doctype html>
<html lang=en>
<title>405 Method Not Allowed</title>
<h1>Method Not Allowed</h1>
<p>The method is not allowed for the requested URL.</p>
```
500 (265 bytes):
```
<!doctype html>
<html lang=en>
<title>500 Internal Server Error</title>
<h1>Internal Server Error</h1>
<p>The server encountered an internal error and was unable to complete your request. Either the server is overloaded or there is an error in the application.</p>
```
401 (23 bytes, no HTML tags, no trailing newline):
```
Authentication required```
414 (327 bytes) and the 400 for a malformed request line (363 bytes) — both from Python's
`http.server`, LF-only line endings inside the body, sent as a body-only/HTTP-0.9-style response for
the 400 case:
```
<!DOCTYPE HTML>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <title>Error response</title>
    </head>
    <body>
        <h1>Error response</h1>
        <p>Error code: 414</p>
        <p>Message: Request-URI Too Long.</p>
        <p>Error code explanation: 414 - URI is too long.</p>
    </body>
</html>
```

---

## 6. HTML escaping (Jinja2 + MarkupSafe 3.0.3, `autoescape=True`)

Measured with `api_key=k&<>"'`, `base_url=http://127.0.0.1:9/x?y=1&z=<2>`,
`notification_urls=…\nsecond line "quoted" 'single' £é` (`B11`/`B12`/`B13`):

| input character | output bytes | note |
|---|---|---|
| `&` | `&amp;` | five characters, not `&#38;` |
| `<` | `&lt;` | |
| `>` | `&gt;` | |
| `"` | `&#34;` | **decimal**, not `&quot;` |
| `'` | `&#39;` | **decimal**, not `&#x27;` |
| `\n` | `\n` (0x0A) | kept verbatim, even inside `value="…"` and `<textarea>` |
| `£` (U+00A3) | `\xc2\xa3` | UTF-8 raw, no numeric escape |
| `é` (U+00E9) | `\xc3\xa9` | UTF-8 raw, no numeric escape |
| space, `,`, `:`, `/`, `?`, `=`, `(`, `)` | unchanged | |

So the exact substitution table is: `&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`, `"`→`&#34;`, `'`→`&#39;`,
everything else copied as UTF-8 bytes. This holds for config values **and** for log entries
(`B03_get-logs-edgecase.raw` shows `&amp; &lt; &gt; &#34; &#39; £ é` inside the log div).

---

## 7. `group_log_entries()` and `tail_file()` — exact behaviour

`tail_file('logs/octobot.log', None)` (path is **relative to the process CWD**) then
`group_log_entries(lines)`:

```
open(path, 'r', encoding='utf-8')      # newline=None  → universal newlines
lines = f.readlines()                  # \r\n and lone \r are translated to \n, and each
                                       # translated newline ends a line — including the last one
```
Grouping (verbatim semantics of the source):

```
pattern = ^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}   (re.match → anchored at the START of the line)
entry starts when the line matches and the previous entry is non-empty
non-matching lines are appended to the current entry, or start one if there is none
the final entry is emitted if non-empty
```

Measured results (see `analysis/log-groups.txt` for the full list):

| input file | entries produced |
|---|---|
| `first line without any timestamp\n    indented continuation\n2024-01-02 03:04:05 …\nsecond line\nthird, & < > " ' £ é\n\n2024-01-02 03:04:06 …\n2024-01-02 03:04:07 …` (no final LF) | 4: `["first line…\n    indented continuation\n", "2024-01-02 03:04:05 …\nsecond line…\nthird…\n\n", "2024-01-02 03:04:06 …\n", "2024-01-02 03:04:07 …(no \n)"]` |
| CRLF everywhere | `\r\n` becomes `\n`; entries end with `\n` |
| empty file | 0 entries → page still renders (1589-byte body) |
| file missing | 1 entry, exactly `Log file not found. The bot may not have started yet.` (no `\n`) |
| invalid UTF-8 | 1 entry, exactly `Error reading log file: 'utf-8' codec can't decode byte 0xff in position 0: invalid start byte` |
| path is a directory | 1 entry, exactly `Error reading log file: [Errno 21] Is a directory: 'logs/octobot.log'` |
| mode 000 | 1 entry, exactly `Error reading log file: [Errno 13] Permission denied: 'logs/octobot.log'` |
| single untimestamped line, no LF | 1 entry, that line without `\n` |
| `2024-13-99 99:99:99 …` | **matches** (the regex does not validate ranges) |
| ` 2024-01-02 03:04:05 …` (leading space) | does **not** match → appended to the previous entry |
| lone `\r` separator | splits lines (and the `\r` is gone) |
| `\x0b \x0c \x85 U+2028` inside a line | **not** separators; kept verbatim |
| UTF-8 BOM before the first timestamp | BOM stays as U+FEFF and the line does **not** match → a continuation line |

The `Error reading log file: …` strings embed CPython/OS error text (`[Errno 21] …`), so they are
**platform dependent**; a Rust port cannot match them byte-for-byte unless it reproduces the same
message format.

---

## 8. The flash / session cookie — exact algorithm

Flask's `SecureCookieSessionInterface` with `app.secret_key = 'octobot-tool'`, itsdangerous 2.2.0.

### 8.1 Algorithm

```
secret_key  = b"octobot-tool"          # app.secret_key, a str → encoded utf-8
salt        = b"cookie-session"        # SecureCookieSessionInterface.salt
sep         = b"."
digest      = SHA-1                    # digest_method
key_deriv   = "hmac"

# 1. derive the signing key   (Signer.derive_key, key_derivation == "hmac")
k = HMAC_SHA1(key = secret_key, msg = salt)          # = hmac(secret_key, digestmod=sha1) over salt

# 2. serialize the tagged session to JSON  (Flask TaggedJSONSerializer → flask.json.dumps)
json_bytes = json.dumps(tagged, separators=(",", ":"), ensure_ascii=True, sort_keys=True).encode("utf-8")
#   the tuple ("error", "msg") inside _flashes is tagged as {" t": ["error", "msg"]}
#   sort_keys=True and ensure_ascii=True are Flask's DefaultJSONProvider defaults

# 3. compress if that saves at least one byte   (URLSafeSerializerMixin.dump_payload)
z          = zlib.compress(json_bytes)               # zlib default level
compressed = len(z) < (len(json_bytes) - 1)
payload    = b"." + b64url(z) if compressed else b64url(json_bytes)
#   b64url(x) = base64.urlsafe_b64encode(x) with all "=" padding stripped

# 4. timestamp   (TimestampSigner.get_timestamp → int(time.time()), UTC epoch seconds)
ts  = b64url(struct.pack(">Q", epoch_seconds).lstrip(b"\x00"))   # big-endian, leading NULs dropped

# 5. sign
value = payload + b"." + ts
sig   = b64url(HMAC_SHA1(key = k, msg = value))
cookie_value = value + b"." + sig          # ASCII
```

`Set-Cookie` header (werkzeug `dump_cookie`, attribute order is fixed):

| situation | header value | capture |
|---|---|---|
| flash written | `session=<cookie_value>; HttpOnly; Path=/` | `A25`, `A27`, `A29`, `A30`, `A31` |
| session emptied by reading flashes | `session=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; HttpOnly; Path=/` | `A26`, `A28`, `B12` |
| session permanent and non-empty | `session=<cookie_value>; Expires=<now+31d RFC1123>; HttpOnly; Path=/` | `B22` |

There is **no `SameSite`** attribute (`SESSION_COOKIE_SAMESITE` is `None`), no `Domain`, no `Secure`.
Attribute order comes from werkzeug: Domain, Expires, Max-Age, Secure, HttpOnly, Path, SameSite,
Partitioned — only the ones set are emitted. `Max-Age=0` **is** emitted for the delete cookie.

The signature is deterministic for a given key and timestamp: **no randomness**. Two POSTs in the
same second produce byte-identical cookies (`A24b`–`A25`: same value). Reading a cookie never
re-signs it; only `should_set_cookie` (modified, or permanent + `SESSION_REFRESH_EACH_REQUEST=True`)
does.

### 8.2 Verifier (stdlib only, reproduces the captured cookies byte-for-byte)

`conformance/research/flask/scripts/flash_cookie_repro.py` — run it:

```
/usr/bin/python3 conformance/research/flask/scripts/flash_cookie_repro.py
A25_post-config-valid      ts=1790536131 compressed=True  match=True
A27_post-config-bad-time   ts=1790536131 compressed=False match=True
A29_post-config-bad-num    ts=1790536131 compressed=False match=True
A30_post-config-negative   ts=1790536131 compressed=False match=True
A31_post-config-both-bad   ts=1790536131 compressed=True  match=True
ALL CAPTURES REPRODUCED
```

Each `match=True` means: parse the captured `Set-Cookie`, recover its own timestamp, rebuild the
cookie from the flash messages, and compare the full base64 string.

Cross-check against the real library with the clock frozen
(`scripts/cookie_xcheck.py`, run under `.venv-py/bin/python`): flash payloads, non-ASCII text,
`sort_keys`, tuple tagging and the compression branch all agree (`XCHECK OK`).

### 8.3 `_flashes` payload shapes seen on the wire

| POST /config case | flash messages | JSON (`ensure_ascii=True`, `sort_keys=True`, `separators=(",",":")`) | zlib? |
|---|---|---|---|
| valid form | `[("success", "Configuration updated successfully! (Will reset on container restart)")]` | `{"_flashes":[{" t":["success","Configuration updated successfully! (Will reset on container restart)"]}]}` (105 B) | **yes** (99 B) |
| `execution_time=25:00` | `[("error", "Execution time must be in HH:MM format (00:00 to 23:59)")]` | `{"_flashes":[{" t":["error","Execution time must be in HH:MM format (00:00 to 23:59)"]}]}` (89 B) | no (zlib would give 95) |
| `switch_threshold=abc` | `[("error", "Switch threshold must be a number")]` | `{"_flashes":[{" t":["error","Switch threshold must be a number"]}]}` (67 B) | no (zlib would give 73) |
| `switch_threshold=-5` | `[("error", "Switch threshold must be positive")]` | `{"_flashes":[{" t":["error","Switch threshold must be positive"]}]}` (67 B) | no (zlib would give 73) |
| `execution_time=25:00` **and** `switch_threshold=abc` | two `error` tuples in that order | 142 B | **yes** (124 B) |

Exact flash texts (the only strings this app can flash):

```
Configuration updated successfully! (Will reset on container restart)      # success
Execution time must be in HH:MM format (00:00 to 23:59)                    # error, validate_config
Switch threshold must be a number                                          # error, validate_config
Switch threshold must be positive                                          # error, validate_config
Error updating config: <str(exception)>                                    # error, unreachable here (see below)
```

### 8.4 Which requests get `Vary` / `Set-Cookie`

Measured with hand-built cookies (all on `GET /config`):

| request cookie | session after read | response headers |
|---|---|---|
| none | empty, untouched | none of the two |
| `session=` (empty value) | empty, untouched | none |
| garbage `garbage.value.sig` | empty (BadSignature caught) | none (`B21`) |
| correctly signed, wrong key | empty | none (`B18`) |
| correctly signed, timestamp 32 days old | empty, `SignatureExpired` (max_age = 31 days) | none (`B17`) |
| correctly signed, payload not JSON | **500** (BadPayload escapes `except BadSignature`) | `H6` (`B19`) |
| valid, `{"_flashes":[…]}` | flashes popped → modified, empty | `Vary: Cookie` + delete cookie (`A26`, `B12`) |
| valid, no `_flashes` | non-empty, untouched | none (`B16`) |
| valid, `{"_permanent": true}` | non-empty, permanent | `Vary: Cookie` + re-signed cookie with `Expires` (`B22`) |

`max_age` for cookie acceptance is `PERMANENT_SESSION_LIFETIME` = 31 days = 2678400 s.

### 8.5 `group_log_entries` / auth / routing edge cases worth a unit test

See §7 (logs) and §3 (auth). Extra measured facts a Rust port must not "fix":

* POSTing a form whose `Content-Type` is `application/json` still succeeds with a **302 and a
  success flash**, because `request.form` is empty and validation of an empty dict passes
  (`C05_post-config-json-ct.raw`). `update_config({})` then resets `ONE_OFF_RUN`, `DRY_RUN` and
  `BATCH_NOTIFICATIONS` to `False`.
* A `Content-Length` larger than the body: werkzeug 3.1.9 tolerates it, the request is processed
  (`C24_post-config-body-extra.raw`).
* Duplicate form field: the **first** value wins (`request.form.to_dict()`), `B14`/`B15`.
* `POST /config` with no body at all (no `Content-Length`) → 302 + success flash (`A33`).
* `http.server` ignores a header line without a colon; the request still succeeds (`C20`).
* An absolute-form request target (`GET http://example.com/config HTTP/1.1`) routes on the path
  only; the `Host`/authority is not checked (`C21`, `C22`).
* `//` → the index route (200), `/../` → 404 (`A19`, `A19b`).

---

## 9. Environment dependent vs fixed bytes

| byte region | fixed? | depends on |
|---|---|---|
| status line, all header names/order/case | **fixed** | — |
| `Content-Type`, `Content-Length`, `Location`, `Vary`, `WWW-Authenticate`, `Connection` | **fixed** | body sizes (fixed per body) |
| `Allow` method **set** | **fixed** | route (`GET, HEAD, OPTIONS` / `POST, GET, HEAD, OPTIONS`) |
| `Server` value | **not fixed** | werkzeug + python version (`Werkzeug/3.1.9 Python/3.11.15` today) |
| `Date` value | **not fixed** | wall clock; format fixed |
| `Set-Cookie` value | **time dependent, not random** | `int(time.time())` at signing |
| `Set-Cookie` delete form | **fixed** | — |
| `Set-Cookie` `Expires` (permanent session) | **time dependent** | now + 31 days |
| `Allow` method **order** | **not fixed** | Python `set` iteration order, seeded per process; the *set* of methods is fixed |
| template bodies | **fixed given config values** | config state (env defaults + POST history) |
| logs body | **fixed given the log file** | the log file bytes |
| 404/405/500/302/401 bodies | **fixed** | — |
| 400/414 bodies | **fixed text**, but platform-specific wording comes from CPython | CPython `http.server` |
| `Error reading log file: …` text | **not fixed** | CPython errno formatting / OS wording |
| error 500 traceback | server log only, never in the response | — |

---

## 10. How to reproduce (copy/paste)

```bash
# 0. capture harness lives here
cd /Users/chaos/octopus-minmax/conformance/research/flask

# 1. start the production-shaped instance (bounded: this is run in the background and killed after)
cd /Users/chaos/octopus-minmax && env TZ=Europe/London ONE_OFF=true API_KEY=x ACC_NUMBER=A-00000000 \
  BASE_URL=http://127.0.0.1:9 NOTIFICATION_URLS= WEB_PORT=5050 .venv-py/bin/python -u src/main.py &

# 2. raw-socket captures (no HTTP library, so bytes are verbatim)
/usr/bin/python3 scripts/captureA.py     # auth, config, logs, 404/405/HEAD/OPTIONS/HTTP1.0
/usr/bin/python3 scripts/captureC.py     # HEAD/query/json/multipart/odd methods
/usr/bin/python3 scripts/captureC2.py    # bad version, 2-token request line, absolute URI, ...
/usr/bin/python3 scripts/captureE.py     # Accept-Encoding, conditional GET, 414
/usr/bin/python3 scripts/captureF.py     # ingress auth-skip matrix
/usr/bin/python3 scripts/captureI.py     # PUT/DELETE -> 405 Allow lists
/usr/bin/python3 scripts/captureJ.py     # two flashes -> two banners

# 3. log-file scenarios + HTML escaping (scratch cwd, port 5051, web server only)
mkdir -p /private/tmp/flaskB/logs && cd /private/tmp/flaskB
env TZ=Europe/London ONE_OFF=true API_KEY=x ACC_NUMBER=A-00000000 BASE_URL=http://127.0.0.1:9 \
  NOTIFICATION_URLS= WEB_PORT=5051 PYTHONPATH=/Users/chaos/octopus-minmax/src \
  /Users/chaos/octopus-minmax/.venv-py/bin/python -u -c "import web_server; web_server.run_server()" &
cd /Users/chaos/octopus-minmax/conformance/research/flask
/usr/bin/python3 scripts/captureB.py
/usr/bin/python3 scripts/captureD.py     # lone CR, VT/FF/NEL/LS, BOM, blank CRLF

# 4. verification
/usr/bin/python3 scripts/analyze.py            # -> analysis/summary.tsv, log-groups.txt, bodies/
/usr/bin/python3 scripts/flash_cookie_repro.py # cookie reproduced from the captures
.venv-py/bin/python scripts/cookie_xcheck.py   # ditto vs the real Flask/itsdangerous
.venv-py/bin/python scripts/render_check.py    # bodies == Jinja render of src/templates

# 5. MUST kill both listeners when done
lsof -nP -iTCP -sTCP:LISTEN | grep -E '5050|5051'
```

`curl` cross-check (secondary evidence only — the raw files win):

```bash
curl -s --raw -D - -u admin:admin http://127.0.0.1:5050/ | head -20
curl -s --raw -D - -H 'X-Ingress-Path: /x' http://127.0.0.1:5050/config | head -20
curl -s --raw -D - -u admin:admin -d 'execution_time=25:00' http://127.0.0.1:5050/config
```

---

## 11. Capture inventory

| file(s) | what |
|---|---|
| `captures/A*.raw` | `/`, `/config`, `/logs`, 404/405/HEAD/OPTIONS/HTTP1.0/Connection/`Host`/`X-Forwarded-*`, POST `/config` matrix, cookie acceptance |
| `captures/B*.raw` | pristine pages, log-file scenarios, HTML escaping, duplicate fields, cookie acceptance |
| `captures/C*.raw` | HEAD `/config`, query string, lowercase auth header, JSON/multipart bodies, odd methods/targets, 400/404 body-only responses |
| `captures/D*.raw` | CRLF/lone-CR/odd-separator/BOM log files |
| `captures/E*.raw` | `Accept-Encoding`, conditional GET, long URL, 414 |
| `captures/F*.raw` | ingress-header auth-skip matrix |
| `captures/G*.raw` | log path is a directory / mode 000 |
| `captures/I*.raw`, `J*.raw` | 405 `Allow` lists; two-flash banner loop |
| `analysis/summary.tsv` | every capture: status line, headers in order, body size, body sha256 |
| `analysis/bodies/*.body` | body bytes of every capture |
| `analysis/log-groups.txt` | the entries `group_log_entries` produced for each log scenario |
| `zlib/*.json`, `zlib/*.deflate`, `zlib/*.deflate.miniz` | zlib streams used for the Rust backend check (§12) |
| `scripts/*.py` | the capture harness, the cookie verifier, the render/cookie cross-checks |

Total: 115 raw captures. Every file in `captures/` is the literal wire bytes; the first line is the
status line unless the response was HTTP/0.9-style (then the file starts with the body).

## 12. Notes for the Rust implementation

1. **Emit headers in the measured order.** `Server`, `Date`, then the block in §4, then
   `Connection: close`. Hyper/axum will happily reorder; build the header list explicitly.
2. **Deflate must be real zlib.** The compressed cookie branch is `zlib.compress(json)`. The same
   JSON compressed with `flate2`'s default `rust_backend` (`miniz_oxide`) has the **same length but
   different bytes** from byte ~25 onward:
   ```
   success: len got=99 want=99 first_diff_byte=Some(29)
   bothbad: len got=124 want=124 first_diff_byte=Some(25)
   ascii:   len got=96 want=96 first_diff_byte=Some(25)
   ```
   With `flate2 = { version = "1", features = ["zlib"] }` (links system zlib, zlib 1.2.12 here) the
   streams are **byte-identical** to Python's:
   ```
   success: json=105 got=99 want=99 identical=true
   bothbad: json=142 got=124 want=124 identical=true
   ascii:   json=98 got=96 want=96 identical=true
   ```
   Evidence: `zlib/*.json`, `zlib/*.deflate` (zlib), `zlib/*.deflate.miniz` (miniz_oxide).
   So: use the `zlib` (or `zlib-ng`/`zlib-rs` only after a byte-comparison) backend, not
   `rust_backend`.
3. `Location: config` — relative, always. Do not "correct" it.
4. `HEAD` must send the real `Content-Length` and no body; `OPTIONS` must answer with
   `Allow` (`GET, HEAD, OPTIONS` for `/`, `POST, GET, HEAD, OPTIONS` for `/config`) and
   `Content-Length: 0`. The order inside `Allow` is process-random in Flask — choose one and note it.
5. Session state lives in the cookie only; there is no server-side store. `session.modified` /
   `session.accessed` semantics decide whether `Vary`/`Set-Cookie` appear (§8.4).
6. Config state is **in-process and mutable** via POST `/config`; `update_config` also forces the
   three checkboxes to `False` when their fields are absent from the form.

## 13. NOT CAPTURED

| item | reason |
|---|---|
| a flash message containing non-ASCII on the wire | no route can flash arbitrary text; the serializer branch was verified offline instead (`scripts/cookie_xcheck.py`, case "pounds £ euro € accent é …" matched) |
| `Error updating config: <exception>` flash | unreachable with the current validation: `validate_config` rejects every value that would raise inside `update_config` (checked `execution_time`, `switch_threshold`) |
| `TZ` effect on any response byte | the Date header is GMT, cookie timestamps are epoch seconds, and the only local-time strings live in the log file written by the bot (captured indirectly in `A10_get-logs-auth.raw`) |
| `WEB_USERNAME`/`WEB_PASSWORD` set to non-default values | only the default `admin:admin` pair was exercised; the comparison is a plain string equality |
| multi-value/duplicate `Set-Cookie` | impossible here: the app sets at most one cookie per response |
| TLS/HTTPS | the app never configures TLS; the dev server speaks plain HTTP |
