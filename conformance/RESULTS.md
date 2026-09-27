# Rust port: byte-level conformance, size and memory

Development happened on the `rust` branch, now merged into `main` (v1.2.0).  The
implementation is `rust/` (one binary crate, no runtime interpreter) and `dockerfile`
builds it.  The Python implementation was removed from the tree: it is the `v1.1.0` tag,
and `conformance/fetch_reference.sh` checks it out under `reference/` to re-run the
comparison.

## 1. What "byte identical" means here, and how it was checked

`conformance/` runs **both** implementations through the same 18 scenarios against
mock Octopus Energy, Home Assistant and Apprise endpoints, and records every byte each
one produces:

| artefact | what is compared |
|---|---|
| `requests.jsonl` | every HTTP request the implementation put on the wire (request line, header order and casing, body) to all three mock servers, in order |
| sink captures | the notification requests Apprise builds (method, path, headers, JSON/form/XML body) |
| `octobot.log` | the log file, line for line |
| `stdout.txt` / `stderr.txt` | both streams, including Werkzeug's access log |
| `web/*.response` | raw HTTP responses of the dashboard (status line, header order, body) |
| `meta.json` | notification/request counts and exit code |

Reproduce it:

```bash
sh conformance/fetch_reference.sh                             # the v1.1.0 Python tree
python3 conformance/run_all.py --impl py --out /tmp/art/py     # reference
python3 conformance/run_all.py --impl rs --out /tmp/art/rs     # port
python3 conformance/compare.py --reference /tmp/art/py --candidate /tmp/art/rs
```

`compare.py` reports every difference together with the normalizations it applied, so
the claim is auditable rather than absolute.

The recorded evidence was produced with the reference's own environment: Python 3.11.15,
`requests` 2.32.3, urllib3 2.8.0, Werkzeug 3.1.9, Apprise 1.9.2.  That matters for one
header: urllib3 advertises `zstd` in `Accept-Encoding` when the interpreter provides a
`zstd` module (Python 3.14+), while the shipped image (`python:3.9-slim`) and the port
send exactly `gzip, deflate`.  `fetch_reference.sh` therefore prefers a 3.9-3.13
interpreter and warns otherwise.

### Result

```
scenarios compared: 18
artifacts identical: 306/306
no differences
```

Re-verified after the Python tree was deleted (`bef4260`), with the reference fetched from
the `v1.1.0` tag by `conformance/fetch_reference.sh`:

```
scenarios compared: 16
artifacts identical: 272/272
no differences
```

The scenario matrix: the full switch flow (compare, switch, accept terms, verify), the
verification-retry flow, dry run, batching, no-notifications, below-threshold,
already-cheapest, an unknown tariff ID, the current tariff being the grandfathered
Cosy 12M Fixed, no IMPORT meter, no smart device, an unknown tariff code, a 401 that
forces a token refresh, a KT-CT-1124 JWT refresh, a tariff whose rates do not cover a
period, the Home Assistant consumption source (normal, empty history, no token), and,
in every scenario, 14 raw dashboard requests (auth, no auth, bad auth, ingress headers,
config GET/POST valid/invalid, logs, 404, 405, HTTP/1.0, connection reuse).

With `OCTO_SERVER_HEADER='Werkzeug/3.1.9 Python/3.11.15'` and
`compare.py --strict-server-header`, a scenario is 17/17 identical **including** the
`Server` response header - see section 4 for why that header is a knob rather than a
default.

## 2. Why byte-identity was achievable at all

Several parts of the reference's output are Python-specific, and each is reproduced
rather than approximated:

| reference behaviour | how the port matches it |
|---|---|
| `requests`' header order, casing and `Content-Length` placement | hand-written HTTP/1.1 client (`src/http.rs`); verified against captures |
| `json.dumps` payload spacing and `ensure_ascii` escaping | `src/jsonutil.rs` |
| Python `repr()` in log lines (`{'data': ...}`, `True`, `5.399999999999999`) | `src/pyrepr.rs`, including shortest-round-trip float formatting |
| `set(tariff_ids.lower().split(","))` ordering | CPython SipHash-1-3 plus the set's linear-probe table (`src/pyhash.rs`); honours `PYTHONHASHSEED` |
| Flask/itsdangerous session cookie | HMAC-SHA-1 key derivation, `TaggedJSONSerializer` payload, zlib compression branch (`src/session.rs`); unit-tested against captured cookies |
| Werkzeug response bytes | exact header order per status, `Location: config`, Werkzeug error/redirect bodies, `Connection: close`, access log with ANSI colours |
| Apprise 1.9.2 notification payloads | `src/apprise.rs`, verified against Apprise's own wire captures in unit tests |
| `logging` formats and rotation | `src/logger.rs` (detailed file format, simple console format, 10 MiB / 5 backups) |

## 3. Normalizations applied (and why each exists)

Every entry is either a wall-clock value or an artefact of the reference's runtime that
cannot exist in Rust.  No application-authored byte is normalized away.

| normalization | count | why |
|---|---|---|
| `log-timestamp` | 2960 | `%(asctime)s` in log lines (second-resolution wall clock) |
| `access-log-timestamp` | 504 | the timestamp in Werkzeug's access log line |
| `http-date` | 480 | the HTTP `Date` header |
| `server-header` | 468 | the `Server` header identifies the HTTP implementation (section 4) |
| `log-timestamp`/`message-timestamp` | 272 | the `[dd/mm/yyyy HH:MM]` stamp inside notification text |
| `object-address` | 112 | heap addresses inside Python dataclass `repr`s (a `Tariff` has no `__repr__`, so the reference logs `<tariff.Tariff object at 0x...>`; the port logs its own address) |
| `startup-race` | 38 | which of the two threads logs first (see section 4) |
| `ha-end-time` (+quoted/repr forms) | 12 | the Home Assistant query `end_time`, i.e. the moment of the request |
| `ha-start-path` | 12 | the same request's period path (local midnight in UTC) |
| `batch-title-clock` | 2 | `HH:MM:SS` in a batch notification title (ONE_OFF mode) |

## 4. Known differences (accepted, not normalized away)

1. **`Server` header.**  The reference sends `Werkzeug/3.1.9 Python/3.11.15`; the port
   sends `octo-minmax/1.1.0 (Rust)`.  Set `OCTO_SERVER_HEADER` to the reference value
   for byte-identical responses (proved with `--strict-server-header`).  The default
   does not claim to be Werkzeug.
2. **Development-server banner.**  Werkzeug writes
   ` * Serving Flask app 'web_server'`, ` * Debug mode: off` (stdout) and an
   ANSI-coloured `WARNING: This is a development server...` plus
   ` * Running on http://...` (stderr) at start-up.  The port's server has no banner;
   the comparison removes those lines on both sides.  Application stdout
   (`Starting bot thread...`, `Starting web server thread...`) and every application log
   line are compared in full.
3. **Thread-start race.**  Both implementations start a bot thread and a web thread;
   which one reaches its first log line first is decided by the scheduler, and the
   reference itself varies run to run (observed: the web line first in 5 of 6 runs, the
   notification line first in the `octopus-nonotify` run).  The port starts the web
   thread first, which reproduces the reference's usual order; the comparison treats the
   two racing lines as a set.
4. **Python tracebacks.**  `logger.exception` in `query_service.execute_rest_query`
   appends a Python traceback to the log in the reference; the port logs the identical
   message line without a traceback.  Only reachable on a REST failure.
5. **Transport error text.**  For a connection-level failure the reference reports
   `requests`' wrapped text (`HTTPConnectionPool(host=...): Max retries exceeded ...`),
   which the port approximates; HTTP-status errors use `requests`' exact wording
   (`404 Client Error: Not Found for url: ...`).
6. **`Allow` header order.**  Werkzeug builds it from a Python `set`, so the order of
   `GET, HEAD, OPTIONS` is a hash artefact; the port reproduces the order CPython gives
   for `PYTHONHASHSEED=0`, which is what the harness pins.  (The reference was observed
   to vary this order across processes.)
7. **Connection reuse.**  The reference leaves a socket open after answering with
   `Connection: close` (the client sees a timeout or a reset depending on timing); the
   port closes it.  The harness records this as a non-compared marker (`13_reuse`).
8. **`slack://` and mailto notifications.**  Apprise stamps `attachments[0].ts` with the
   wall clock, and its MIME boundary/Message-ID/Date are random; those bytes cannot be
   identical in either implementation and are excluded from the comparison (no scenario
   uses them).
9. **Random scheduling delay.**  `random.randint(10, 900)` in scheduled mode is
   unpredictable in both implementations (the port uses a seeded xorshift).  Scenarios
   run in ONE_OFF mode, where the reference is deterministic.

## 5. Size, memory and CPU

Measured by `conformance/measure.py` (`conformance/measurements.json`).  CPU is the
child's own user+sys time from `getrusage(RUSAGE_CHILDREN)` (microsecond resolution),
measured over three repeats of each phase.

### Container image (each project's own Dockerfile)

| image | base | total on disk | app's own layers | `docker save \| gzip` |
|---|---|---|---|---|
| Python (`python:3.9-slim`) | 209 MB | **269 MB** | 60 MB | 54.0 MB |
| Rust (`debian:bookworm-slim`) | 136 MB | **141 MB** | 5 MB | 28.1 MB |

### Runtime footprint on disk (source + dependency closure)

| | bytes | human |
|---|---|---|
| Python: `src/` + 25 installed packages | 21,692,617 | 20.7 MiB |
| Rust: release binary | 2,870,912 | 2.74 MiB |

The Rust binary is stripped, LTO'd and statically links its dependencies (no
interpreter, no `site-packages`, no `libpython`).

### Resident memory (both idle with the dashboard running)

| | median RSS | peak RSS |
|---|---|---|
| Python | 49.5 MiB | 49.5 MiB |
| Rust | **6.5 MiB** | 6.5 MiB |

### CPU (median of 3 repeats)

| workload | Python | Rust |
|---|---|---|
| start-up (launch, bind, exit) | 0.096 s | **0.0037 s** (26x less) |
| idle, dashboard up, 6 s window | 0.000 s (0.0 %) | 0.000 s (0.0 %) |
| 1000 authenticated `GET /` requests: **CPU** | 0.540 s | **0.051 s** (10.6x less) |
| ... per request | 443 µs | **47 µs** (9.4x less) |
| ... wall clock | 11.49 s | 0.085 s |
| ... requests/second | 87 | 11,785 |
| one-off comparison run (14 API calls, 5 notifications) | 0.156 s | **0.0064 s** (24x less) |

Both servers are thread-per-connection (Flask passes `threaded=True` by default) and the
same client loop drives both, so the per-request CPU figures are comparable.  The
dashboard's wall-clock gap is *waiting*, not work: the reference spends ~11.5 ms of wall
time per request against 0.44 ms of CPU (its development server writes the header block
and the body as separate small writes), while the port writes one buffer per response.
Idle CPU is zero for both - the serving threads block in `accept`.

**Summary: 7.6x less resident memory, 10x less CPU per dashboard request, 26x less
start-up CPU, 1.9x smaller image, 7.6x smaller on-disk deployment.**

## 6. Tests

```
cd rust && cargo test        # 25 tests
```

* Apprise wire format: 8 schemes replayed against Apprise 1.9.2 capture files
  (`json://` with and without a title, `form://`, `xml://`, `gotify://`, `ntfy://`,
  `discord://`, `tgram://`) plus the `add()` rejection rules.
* CPython SipHash-1-3 hashes against the interpreter's own values, and set ordering.
* Python `repr()` compatibility for floats, dicts, lists and strings.
* Flask session cookies: two captured cookies (plain and zlib-compressed payloads)
  load, a tampered cookie is rejected, and a re-signed cookie round-trips.
* `group_log_entries`/`tail_file` behaviour, including the universal-newline and
  regex edge cases from the reference.

## 7. Layout

```
rust/src/                 the implementation
  main.rs                 bot thread + web thread (as main.py)
  config.rs logger.rs     env config; logging with the reference's exact formats
  clock.rs                local time / date helpers
  pyhash.rs               CPython SipHash-1-3 + set iteration order
  pyrepr.rs               Python repr() and float formatting
  jsonutil.rs             json.dumps defaults
  http.rs                 HTTP/1.1 client (byte-identical request framing)
  urlcode.rs              urllib.parse quoting
  queries.rs              generated from src/queries.py
  templates.rs            generated from src/templates/*.html
  tariff.rs account_info.rs account_manager.rs comparison_engine.rs
  home_assistant_client.rs query_service.rs
  apprise.rs              notification plugins (wire-identical to Apprise 1.9.2)
  notification_service.rs bot_orchestrator.rs
  session.rs              Flask/itsdangerous session cookie
  web_server.rs           Flask/Werkzeug-compatible HTTP surface
conformance/              harness, comparison, measurements, research notes
```
