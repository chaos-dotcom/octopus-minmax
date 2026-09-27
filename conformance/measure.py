#!/usr/bin/env python3
"""Measure deployment size, resident memory and CPU for both implementations.

    python3 conformance/measure.py --out conformance/measurements.json

Size
  python : src/ + the installed runtime dependency closure of requirements.txt
           (the packages that ship in the image), reported as total bytes
  rust   : the release binary (+ the source tree for reference)

Memory
  Both implementations are started the same way (ONE_OFF run against a closed port,
  web server listening on the chosen port), then the RSS of the process tree is
  sampled while the app sits idle.  Reported: median and peak RSS, and the number of
  samples.

CPU
  Three workloads, each measured as the child's own CPU time (user+sys) from
  `getrusage(RUSAGE_CHILDREN)`:
    1. idle         - the dashboard up, no traffic, 6 s window
    2. dashboard    - 300 sequential authenticated `GET /` requests over fresh
                      connections, reported as wall time, CPU seconds, microseconds
                      per request and requests per second
    3. one-off run  - the full `octopus-noswitch` scenario against the mock Octopus,
                      Home Assistant and Apprise servers (the real work: GraphQL, REST,
                      tar price lookups, comparison, notifications)
"""
import argparse
import json
import os
import re
import resource
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
RUNTIME_PACKAGES = [
    "flask", "werkzeug", "jinja2", "markupsafe", "itsdangerous", "click", "blinker",
    "apprise", "requests", "urllib3", "certifi", "idna", "charset_normalizer",
    "markdown", "yaml", "gql", "aiohttp", "aiosignal", "multidict", "yarl",
    "frozenlist", "attrs", "async_timeout", "tzdata", "requests_oauthlib",
    "oauthlib", "PyJWT", "cryptography", "cffi", "pycparser",
]


def directory_size(path):
    total = 0
    for root, _, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(root, name))
            except OSError:
                pass
    return total


def python_size():
    site = os.path.join(REPO, ".venv-py/lib/python3.11/site-packages")
    packages = 0
    listed = []
    for entry in sorted(os.listdir(site)):
        if entry.endswith(".dist-info") or entry.endswith(".pth") or entry == "__pycache__":
            continue
        path = os.path.join(site, entry)
        if not os.path.isdir(path):
            continue
        name = entry.split("-")[0].lower()
        if name in RUNTIME_PACKAGES:
            size = directory_size(path)
            packages += size
            listed.append({"package": entry, "bytes": size})
    return {
        "source_bytes": directory_size(os.path.join(REPO, "src")),
        "dependency_bytes": packages,
        "total_bytes": directory_size(os.path.join(REPO, "src")) + packages,
        "packages": listed,
    }


def rust_size():
    binary = os.path.join(REPO, "rust/target/release/octo-minmax")
    return {
        "binary_bytes": os.path.getsize(binary) if os.path.exists(binary) else 0,
        "source_bytes": directory_size(os.path.join(REPO, "rust/src")),
    }


def process_rss(pid):
    try:
        out = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)], text=True).strip()
        return int(out or 0) * 1024
    except (subprocess.CalledProcessError, ValueError):
        return None


def sample_memory(cmd, env, seconds=12, interval=0.5):
    workdir = "/tmp/measure-run"
    os.makedirs(os.path.join(workdir, "logs"), exist_ok=True)
    process = subprocess.Popen(cmd, cwd=workdir, env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    samples = []
    deadline = time.time() + seconds
    try:
        time.sleep(3)                       # let start-up finish
        while time.time() < deadline:
            rss = process_rss(process.pid)
            if rss:
                samples.append(rss)
            time.sleep(interval)
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
    if not samples:
        return {"samples": 0}
    return {
        "samples": len(samples),
        "median_bytes": int(statistics.median(samples)),
        "peak_bytes": max(samples),
        "min_bytes": min(samples),
    }


def child_cpu_seconds():
    """CPU time (user+sys) of children reaped so far."""
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return usage.ru_utime + usage.ru_stime


def process_cpu_seconds(pid):
    """CPU time consumed by a live process, from `ps -o time`."""
    try:
        out = subprocess.check_output(["ps", "-o", "time=", "-p", str(pid)], text=True).strip()
    except (subprocess.CalledProcessError, ValueError):
        return None
    match = re.match(r"(?:(\d+)-)?(?:(\d+):)?(\d+):(\d+(?:\.\d+)?)$", out)
    if not match:
        return None
    days, hours, minutes, seconds = match.groups()
    return (int(days or 0) * 86400 + int(hours or 0) * 3600
            + int(minutes or 0) * 60 + float(seconds))


def start_app(cmd, env, workdir, port):
    os.makedirs(os.path.join(workdir, "logs"), exist_ok=True)
    process = subprocess.Popen(cmd, cwd=workdir, env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            import socket as _socket
            _socket.create_connection(("127.0.0.1", port), timeout=0.5).close()
            return process
        except OSError:
            if process.poll() is not None:
                raise RuntimeError("application exited during start-up")
            time.sleep(0.2)
    raise RuntimeError("application did not start listening on port {}".format(port))


def stop_app(process):
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=10)
    return child_cpu_seconds() - stop_app.baseline


stop_app.baseline = 0.0


def dashboard_workload(port, requests_count):
    """`requests_count` sequential authenticated GET / requests, one connection each."""
    import socket as _socket
    request = (b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n"
               b"Authorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n")
    body_bytes = 0
    for _ in range(requests_count):
        connection = _socket.create_connection(("127.0.0.1", port), timeout=10)
        try:
            connection.sendall(request)
            while True:
                chunk = connection.recv(65536)
                if not chunk:
                    break
                body_bytes += len(chunk)
        finally:
            connection.close()
    return body_bytes


def measure_cpu(cmd, env, port, workdir, dashboard_requests, idle_seconds):
    """CPU for four phases, each measured with `getrusage(RUSAGE_CHILDREN)` (microsecond
    resolution, the child's own user+sys time)."""
    result = {}

    def run_phase(seconds=0.0, requests_count=0):
        stop_app.baseline = child_cpu_seconds()
        process = start_app(cmd, env, workdir, port)
        wall_start = time.time()
        if requests_count:
            dashboard_workload(port, requests_count)
        elif seconds:
            time.sleep(seconds)
        wall = time.time() - wall_start
        cpu = stop_app(process)
        return cpu, wall

    # 1. start-up only: launch, wait for the port, stop
    startup_samples = [run_phase()[0] for _ in range(3)]
    result["startup"] = {
        "cpu_seconds": round(statistics.median(startup_samples), 4),
        "samples": [round(value, 4) for value in startup_samples],
    }

    # 2. idle: up and serving, no traffic
    idle_total, _ = run_phase(seconds=idle_seconds)
    idle_cpu = max(0.0, idle_total - result["startup"]["cpu_seconds"])
    result["idle"] = {
        "window_s": idle_seconds,
        "cpu_seconds": round(idle_cpu, 4),
        "cpu_percent": round(100.0 * idle_cpu / idle_seconds, 4),
    }

    # 3. dashboard load, repeated for stability
    runs = []
    for _ in range(3):
        total, wall = run_phase(requests_count=dashboard_requests)
        runs.append({
            "wall_seconds": round(wall, 3),
            "cpu_seconds": round(total, 4),
            "microseconds_cpu_per_request": round(1e6 * (total - result["startup"]["cpu_seconds"]) / dashboard_requests, 1),
            "requests_per_second": round(dashboard_requests / wall, 1),
        })
    result["dashboard"] = {
        "requests": dashboard_requests,
        "wall_seconds": round(statistics.median(run["wall_seconds"] for run in runs), 3),
        "cpu_seconds": round(statistics.median(run["cpu_seconds"] for run in runs), 4),
        "microseconds_cpu_per_request": round(statistics.median(run["microseconds_cpu_per_request"] for run in runs), 1),
        "requests_per_second": round(statistics.median(run["requests_per_second"] for run in runs), 1),
        "repeats": runs,
    }

    # 4. a full one-off comparison run against the mock Octopus/HA/Apprise servers
    import harness
    run_dir = os.path.join(workdir, "scenario")
    os.makedirs(run_dir, exist_ok=True)
    stop_app.baseline = child_cpu_seconds()
    started = time.time()
    meta = harness.run("py" if "python" in cmd[0] else "rs", "octopus-noswitch", run_dir)
    result["one_off_run"] = {
        "cpu_seconds": round(child_cpu_seconds() - stop_app.baseline, 4),
        "wall_seconds": round(time.time() - started, 3),
        "notifications": meta["notifications"],
        "requests": meta["requests"],
    }
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=os.path.join(HERE, "measurements.json"))
    parser.add_argument("--port", type=int, default=5050)
    parser.add_argument("--dashboard-requests", type=int, default=1000)
    parser.add_argument("--idle-seconds", type=int, default=6)
    args = parser.parse_args()

    base_env = dict(os.environ)
    base_env.update({
        "TZ": "Europe/London",
        "API_KEY": "sk_live_test_key",
        "ACC_NUMBER": "A-1234ABCD",
        "BASE_URL": "http://127.0.0.1:9/v1",     # closed port: the one-off run fails fast
        "ONE_OFF": "true",
        "NOTIFICATION_URLS": "",
        "WEB_PORT": str(args.port),
        "CONSUMPTION_SOURCE": "octopus",
    })

    result = {
        "python": {"size": python_size()},
        "rust": {"size": rust_size()},
    }
    result["python"]["memory"] = sample_memory(
        [os.path.join(REPO, ".venv-py/bin/python"), "-u", os.path.join(REPO, "src/main.py")],
        base_env)
    result["rust"]["memory"] = sample_memory(
        [os.path.join(REPO, "rust/target/release/octo-minmax")], base_env)

    cpu_env = dict(base_env)
    result["python"]["cpu"] = measure_cpu(
        [os.path.join(REPO, ".venv-py/bin/python"), "-u", os.path.join(REPO, "src/main.py")],
        cpu_env, args.port, "/tmp/measure-cpu-py", args.dashboard_requests, args.idle_seconds)
    result["rust"]["cpu"] = measure_cpu(
        [os.path.join(REPO, "rust/target/release/octo-minmax")],
        cpu_env, args.port, "/tmp/measure-cpu-rs", args.dashboard_requests, args.idle_seconds)

    with open(args.out, "w") as handle:
        json.dump(result, handle, indent=2)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
