"""Run every apprise capture case against the local raw sink.

Life is bounded: the sink runs with --max-seconds 300 and is terminated in a
finally block; this process also arms signal.alarm(600).
"""
import json
import os
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
AP = os.path.dirname(HERE)
CASES = sys.argv[1] if len(sys.argv) > 1 else os.path.join(AP, "cases.json")
RESULT_FILE = (sys.argv[2] if len(sys.argv) > 2
               else os.path.join(AP, "run_results.json"))
CAP = os.path.join(AP, "captures")
PY = "/Users/chaos/octopus-minmax/.venv-py/bin/python"
signal.alarm(600)


def main():
    cases = json.load(open(CASES))
    sink_log = open(os.path.join(AP, "logs", "sink.log"), "wb")
    sink = subprocess.Popen(
        [PY, os.path.join(HERE, "sink_server.py"),
         "--config", os.path.join(AP, "sink_config.json"),
         "--outdir", CAP, "--max-seconds", "300"],
        stdout=sink_log, stderr=subprocess.STDOUT)
    try:
        # wait until every listener has reported
        want = len(json.load(open(os.path.join(AP, "sink_config.json")))["listeners"])
        for _ in range(100):
            time.sleep(0.2)
            try:
                txt = open(os.path.join(AP, "logs", "sink.log")).read()
            except OSError:
                txt = ""
            if txt.count("LISTEN ") >= want:
                break
        else:
            raise SystemExit("sink did not start")
        env = dict(os.environ)
        env["REQUESTS_CA_BUNDLE"] = os.path.join(AP, "certs", "ca-bundle.pem")
        env.pop("PYTHONPATH", None)
        results = []
        for c in cases:
            cp = os.path.join(AP, "case_current.json")
            op = os.path.join(AP, "captures", "case_%s.json" % c["case"])
            with open(cp, "w") as f:
                json.dump(c, f)
            r = subprocess.run(
                [PY, os.path.join(HERE, "capture_case.py"), cp, CAP, op],
                env=env, capture_output=True, text=True, timeout=180)
            sys.stdout.write("[%s] rc=%d %s\n" % (
                c["case"], r.returncode, (r.stdout or "").strip()[:400]))
            if r.stderr.strip():
                sys.stdout.write("    stderr: %s\n" % r.stderr.strip()[-600:])
            sys.stdout.flush()
            results.append({"case": c["case"], "rc": r.returncode,
                            "stdout": r.stdout, "stderr": r.stderr,
                            "out": op})
        with open(RESULT_FILE, "w") as f:
            json.dump(results, f, indent=1)
    finally:
        sink.terminate()
        try:
            sink.wait(timeout=10)
        except subprocess.TimeoutExpired:
            sink.kill()
        sink_log.close()
        sys.stdout.write("sink stopped rc=%s\n" % sink.returncode)
        sys.stdout.flush()


if __name__ == "__main__":
    main()
