#!/usr/bin/env python3
"""Conformance harness: run one implementation through one scenario and save every
byte the implementation produced (requests on the wire, notifications, log file,
stdout/stderr, HTTP responses of its web UI).

    python3 conformance/harness.py run --impl py --scenario octopus-noswitch --out artifacts/py/octopus-noswitch
"""
import argparse
import copy
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
from urllib.parse import urlparse

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import mocks                                    # noqa: E402
from mocks import Fixtures, OCTOPUS_PORT, HA_PORT, SINK_PORT   # noqa: E402
from rawhttp import RawServer, json_response    # noqa: E402
from scenarios import SCENARIOS, WEB_REQUESTS   # noqa: E402

DEFAULT_ENV = {
    "TZ": "Europe/London",
    "API_KEY": "sk_live_test_key",
    "ACC_NUMBER": "A-1234ABCD",
    "BASE_URL": "http://127.0.0.1:{}/v1".format(OCTOPUS_PORT),
    "EXECUTION_TIME": "23:00",
    "SWITCH_THRESHOLD": "2",
    "WEB_USERNAME": "admin",
    "WEB_PASSWORD": "admin",
    "WEB_PORT": "5050",
    "ONE_OFF": "false",
    "DRY_RUN": "false",
    "BATCH_NOTIFICATIONS": "false",
    "NOTIFICATION_URLS": "",
    "CONSUMPTION_SOURCE": "homeassistant",
    "HA_IMPORT_ENTITY": "sensor.test_import_total",
}


class Sink:
    """Records the notification requests an implementation sends."""

    def __init__(self, recorder):
        self.recorder = recorder
        self.items = []

    def handler(self, req):
        return 200, [("Content-Type", "application/json")], json.dumps({"status": "ok"})


def build_octopus_handler(fx, scenario, captures):
    state = {"gql": 0}

    def handler(req):
        path = req.target
        if path.startswith("/v1/graphql/"):
            state["gql"] += 1
            n = state["gql"]
            if scenario.get("gql_401_at") and n in scenario["gql_401_at"]:
                return 401, [("Content-Type", "application/json")], json.dumps({"error": "unauthorized"})
            if scenario.get("gql_kt1124_at") and n in scenario["gql_kt1124_at"]:
                body = {"errors": [{"message": "JWT expired",
                                    "extensions": {"errorCode": "KT-CT-1124"}}]}
                return 200, [("Content-Type", "application/json")], json.dumps(body)
            if scenario.get("gql_error_at") and n in scenario["gql_error_at"]:
                body = {"errors": [{"message": "Unauthorized.",
                                    "locations": [{"line": 2, "column": 3}],
                                    "path": ["account"],
                                    "extensions": {
                                        "errorType": "AUTHORIZATION",
                                        "errorCode": "KT-CT-1111",
                                        "errorDescription": "The viewer is not authorized to "
                                                            "execute the query/mutation. Check "
                                                            "authentication or roles/permissions."}}]}
                return 200, [("Content-Type", "application/json")], json.dumps(body)
            payload = json.loads(req.body.decode("utf-8"))
            return json_response(fx.graphql(payload["query"]))
        path_only = path.split("?")[0]
        if path_only.rstrip("/").endswith("products"):
            return json_response(fx.products())
        if "/standard-unit-rates/" in path:
            code = path.split("/v1/products/")[1].split("/")[0]
            rates = []
            for offset in (-1, 0, 1):
                rates.extend(fx.rates_for(code, offset))
            return json_response({"count": len(rates), "next": None, "previous": None,
                                  "results": rates})
        if path.startswith("/v1/products/"):
            code = path[len("/v1/products/"):].strip("/")
            return json_response(fx.product_detail(code))
        return 404, [("Content-Type", "application/json")], json.dumps({"detail": "Not found"})

    return handler


def build_ha_handler(fx, scenario):
    def handler(req):
        if "/history/period/" in req.target:
            if scenario.get("ha_empty"):
                return json_response([])
            return json_response(fx.ha_history())
        return 404, [("Content-Type", "application/json")], json.dumps({"message": "Not found"})

    return handler


def http_exchange(port, request_bytes, timeout=5.0):
    """Send raw request bytes, read the whole response, return it verbatim."""
    s = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    try:
        s.sendall(request_bytes)
        chunks = []
        while True:
            try:
                data = s.recv(65536)
            except socket.timeout:
                break
            if not data:
                break
            chunks.append(data)
        return b"".join(chunks)
    finally:
        s.close()


def wait_port(port, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=0.5).close()
            return True
        except OSError:
            time.sleep(0.2)
    return False


def wait_port_free(port, timeout=30.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if not port_in_use(port):
            return True
        time.sleep(0.5)
    return False


def port_in_use(port):
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        probe.bind(("127.0.0.1", port))
    except OSError:
        return True
    finally:
        probe.close()
    return False


def run(impl, scenario_name, outdir):
    scenario = SCENARIOS[scenario_name]
    fixture_opts = dict(scenario.get("fixture", {}))
    if scenario.get("gql_stale_at"):
        fixture_opts["gql_stale_at"] = scenario["gql_stale_at"]
    rates_snapshot = copy.deepcopy(mocks.RATES)
    fx = Fixtures(fixture_opts)
    _apply_fixture_overrides(fx, fixture_opts, scenario)

    captures = []
    lock = threading.Lock()

    def recorder(record):
        with lock:
            record["seq"] = len(captures)
            captures.append(record)

    for port in (SINK_PORT, OCTOPUS_PORT, HA_PORT, int(DEFAULT_ENV["WEB_PORT"])):
        if not wait_port_free(port, 20):
            raise RuntimeError("port {} is still in use".format(port))

    sink_server = RawServer(SINK_PORT, Sink(recorder).handler, "sink", recorder)
    octopus_server = RawServer(OCTOPUS_PORT, build_octopus_handler(fx, scenario, captures),
                               "octopus", recorder)
    ha_server = RawServer(HA_PORT, build_ha_handler(fx, scenario), "ha", recorder)

    run_dir = os.path.join(outdir, "run")
    if os.path.exists(run_dir):
        shutil.rmtree(run_dir)
    os.makedirs(os.path.join(run_dir, "logs"))

    env = dict(os.environ)
    env.update(DEFAULT_ENV)
    env.update(scenario["env"])
    if impl == "py":
        # The Python implementation was removed from the tree in v1.2.0; it lives in the
        # v1.1.0 tag.  `conformance/fetch_reference.sh` checks it out under reference/.
        reference = os.environ.get("OCTO_REFERENCE_DIR", os.path.join(REPO, "reference/python"))
        interpreter = os.environ.get(
            "OCTO_REFERENCE_PYTHON", os.path.join(reference, ".venv/bin/python"))
        entry = os.path.join(reference, "src/main.py")
        if not (os.path.exists(entry) and os.path.exists(interpreter)):
            raise RuntimeError(
                "the Python reference is not available; run conformance/fetch_reference.sh "
                "(or set OCTO_REFERENCE_DIR and OCTO_REFERENCE_PYTHON)")
        env["PYTHONHASHSEED"] = "0"
        cmd = [interpreter, "-u", entry]
    else:
        cmd = [os.path.join(REPO, "rust/target/release/octo-minmax")]

    started = time.time()
    stdout_path = os.path.join(outdir, "stdout.txt")
    stderr_path = os.path.join(outdir, "stderr.txt")
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as err:
        proc = subprocess.Popen(cmd, cwd=run_dir, env=env, stdout=out, stderr=err)

    web_port = int(env["WEB_PORT"])
    web_port_ready = wait_port(web_port, 30) and proc.poll() is None
    if not web_port_ready and proc.poll() is None:
        time.sleep(2)
        web_port_ready = wait_port(web_port, 10)

    min_notifications = scenario.get("min_notifications", 0)
    deadline = time.time() + scenario.get("timeout", 60)
    log_path = os.path.join(run_dir, "logs", "octobot.log")
    stable = 0.0
    previous_size = -1

    def log_size():
        try:
            return os.path.getsize(log_path)
        except OSError:
            return 0

    while time.time() < deadline:
        if proc.poll() is not None:
            break
        delivered = len([c for c in captures if c["server"] == "sink"])
        size = log_size()
        if delivered >= min_notifications and size == previous_size and size > 0:
            stable += 0.5
            if stable >= 2.5:
                break
        else:
            stable = 0.0
        previous_size = size
        time.sleep(0.5)
    settle = scenario.get("settle_after")
    if settle:
        time.sleep(settle)

    web_dir = os.path.join(outdir, "web")
    os.makedirs(web_dir, exist_ok=True)
    web_meta = []
    try:
        try:
            _run_web_captures(proc, web_port, web_port_ready, web_dir, web_meta, outdir)
        except Exception as exc:                                   # noqa: BLE001
            web_meta.append({"error": "{}: {}".format(type(exc).__name__, exc)})
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=10)
        octopus_server.stop()
        ha_server.stop()
        sink_server.stop()
    exit_code = proc.returncode
    duration = time.time() - started
    for port in (OCTOPUS_PORT, HA_PORT, SINK_PORT, web_port):
        wait_port_free(port, 10)
    time.sleep(0.3)

    log_path = os.path.join(run_dir, "logs", "octobot.log")
    if os.path.exists(log_path):
        shutil.copy(log_path, os.path.join(outdir, "octobot.log"))

    with open(os.path.join(outdir, "requests.jsonl"), "w") as fh:
        for record in captures:
            fh.write(json.dumps(record) + "\n")

    meta = {
        "impl": impl,
        "scenario": scenario_name,
        "env": {k: v for k, v in sorted(env.items()) if k in DEFAULT_ENV or k in scenario["env"]},
        "exit_code": exit_code,
        "duration_s": round(duration, 3),
        "started": started,
        "today": str(fx.today),
        "web": web_meta,
        "web_port_ready": web_port_ready,
        "notifications": len([c for c in captures if c["server"] == "sink"]),
        "requests": len(captures),
    }
    with open(os.path.join(outdir, "meta.json"), "w") as fh:
        json.dump(meta, fh, indent=2, sort_keys=True)
    _restore_rates(rates_snapshot)
    return meta


def _run_web_captures(proc, web_port, web_port_ready, web_dir, web_meta, outdir):
    if web_port_ready:
        for name, request in WEB_REQUESTS:
            if proc.poll() is not None:
                raise RuntimeError("application exited before the web capture phase (exit {})".format(proc.returncode))
            response = http_exchange(web_port, request)
            with open(os.path.join(web_dir, name + ".request"), "wb") as fh:
                fh.write(request)
            with open(os.path.join(web_dir, name + ".response"), "wb") as fh:
                fh.write(response)
            web_meta.append({"name": name, "bytes": len(response)})
        # config POST + follow-up GET that replays the flash cookie
        # Pick an execution time that is not the current minute, so the bot does not
        # start a scheduled comparison (which sleeps for a random interval) while we
        # are looking at its web UI.
        import datetime as _dt
        safe_time = "23:58" if _dt.datetime.now().strftime("%H:%M") == "23:59" else "23:59"
        form = ("api_key=sk_live_test_key&acc_number=A-1234ABCD&"
                "base_url=http%3A%2F%2F127.0.0.1%3A18080%2Fv1&execution_time=" +
                safe_time.replace(":", "%3A") + "&"
                "switch_threshold=5&tariffs=cosy%2Cagile&notification_urls=&dry_run=on")
        body = form.encode()
        request = (b"POST /config HTTP/1.1\r\nHost: 127.0.0.1\r\n"
                   b"Authorization: Basic YWRtaW46YWRtaW4=\r\n"
                   b"Content-Type: application/x-www-form-urlencoded\r\n"
                   b"Content-Length: " + str(len(body)).encode() + b"\r\nConnection: close\r\n\r\n" + body)
        response = http_exchange(web_port, request)
        with open(os.path.join(web_dir, "11_config_post_ok.request"), "wb") as fh:
            fh.write(request)
        with open(os.path.join(web_dir, "11_config_post_ok.response"), "wb") as fh:
            fh.write(response)
        web_meta.append({"name": "11_config_post_ok", "bytes": len(response)})
        cookie = _extract_cookie(response)
        if cookie:
            request = (b"GET /config HTTP/1.1\r\nHost: 127.0.0.1\r\n"
                       b"Authorization: Basic YWRtaW46YWRtaW4=\r\nCookie: " + cookie.encode() +
                       b"\r\nConnection: close\r\n\r\n")
            response = http_exchange(web_port, request)
            with open(os.path.join(web_dir, "12_config_flash.request"), "wb") as fh:
                fh.write(request)
            with open(os.path.join(web_dir, "12_config_flash.response"), "wb") as fh:
                fh.write(response)
            web_meta.append({"name": "12_config_flash", "bytes": len(response)})
        # Connection reuse: a second request on the same socket.  The Werkzeug dev
        # server always answers with `Connection: close`, so this records whatever
        # the implementation does with the reuse attempt (bytes or an error marker).
        s = socket.create_connection(("127.0.0.1", web_port), timeout=5)
        reuse = {}
        try:
            req1 = b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n"
            s.sendall(req1)
            first = _read_one_response(s)
            with open(os.path.join(web_dir, "13_reuse_first.response"), "wb") as fh:
                fh.write(first)
            req2 = b"GET /logs HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n"
            try:
                s.sendall(req2)
                second = _read_one_response(s)
                outcome = "response"
            except OSError as exc:
                second = b""
                outcome = "error:" + type(exc).__name__
            with open(os.path.join(web_dir, "13_reuse_second.response"), "wb") as fh:
                fh.write(second)
            with open(os.path.join(web_dir, "13_reuse_outcome.txt"), "w") as fh:
                fh.write(outcome + "\n")
            reuse = {"name": "13_reuse", "outcome": outcome, "bytes": len(first) + len(second)}
        finally:
            s.close()
        web_meta.append(reuse)

def _apply_fixture_overrides(fx, opts, scenario):
    if opts.get("rates_override"):
        for code, table in opts["rates_override"].items():
            mocks.RATES[code] = table


def _restore_rates(snapshot):
    mocks.RATES.clear()
    mocks.RATES.update(snapshot)


def _extract_cookie(response):
    head = response.split(b"\r\n\r\n", 1)[0].decode("latin-1")
    for line in head.split("\r\n"):
        if line.lower().startswith("set-cookie:"):
            value = line.split(":", 1)[1].strip()
            return value.split(";")[0]
    return None


def _read_one_response(sock):
    buf = b""
    while b"\r\n\r\n" not in buf:
        data = sock.recv(65536)
        if not data:
            return buf
        buf += data
    head, _, rest = buf.partition(b"\r\n\r\n")
    length = 0
    for line in head.decode("latin-1").split("\r\n"):
        if line.lower().startswith("content-length:"):
            length = int(line.split(":", 1)[1].strip())
    while len(rest) < length:
        data = sock.recv(65536)
        if not data:
            break
        rest += data
    return head + b"\r\n\r\n" + rest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["run"])
    parser.add_argument("--impl", required=True, choices=["py", "rs"])
    parser.add_argument("--scenario", required=True, choices=sorted(SCENARIOS))
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    meta = run(args.impl, args.scenario, args.out)
    print(json.dumps(meta, indent=2))


if __name__ == "__main__":
    main()
