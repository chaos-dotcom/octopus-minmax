#!/usr/bin/env python3
"""Measure deployment size and resident memory for both implementations.

    python3 conformance/measure.py --out conformance/measurements.json

Size
  python : src/ + the installed runtime dependency closure of requirements.txt
           (the packages that ship in the image), reported as total bytes
  rust   : the release binary (+ the source tree for reference)

Memory
  Both implementations are started the same way (ONE_OFF run against a closed port,
  web server listening on 5050), then the RSS of the process tree is sampled while
  the app sits idle.  Reported: median and peak RSS, and the number of samples.
"""
import argparse
import json
import os
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=os.path.join(HERE, "measurements.json"))
    parser.add_argument("--port", type=int, default=5050)
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

    with open(args.out, "w") as handle:
        json.dump(result, handle, indent=2)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
