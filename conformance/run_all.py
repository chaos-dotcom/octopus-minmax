#!/usr/bin/env python3
"""Run every scenario for one implementation and write the artifacts."""
import argparse
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness          # noqa: E402
from scenarios import SCENARIOS   # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--impl", required=True, choices=["py", "rs"])
    parser.add_argument("--out", required=True)
    parser.add_argument("--only", default=None)
    args = parser.parse_args()

    names = [args.only] if args.only else sorted(SCENARIOS)
    summary = {}
    for name in names:
        outdir = os.path.join(args.out, "{}".format(name))
        os.makedirs(outdir, exist_ok=True)
        started = time.time()
        print("== {} {}".format(args.impl, name), flush=True)
        with open(os.path.join(args.out, "progress.txt"), "a") as handle:
            handle.write("start {} {}\n".format(args.impl, name))
        meta = None
        error = None
        for attempt in (1, 2):
            try:
                meta = harness.run(args.impl, name, outdir)
                break
            except Exception as exc:                              # noqa: BLE001
                error = "{}: {}".format(type(exc).__name__, exc)
                print("   ERROR (attempt {})".format(attempt), error, flush=True)
                time.sleep(3)
        if meta is None:
            summary[name] = {"error": error}
        else:
            summary[name] = meta
        print("   done in {:.1f}s".format(time.time() - started), flush=True)
    with open(os.path.join(args.out, "summary.json"), "w") as fh:
        json.dump(summary, fh, indent=2, sort_keys=True)


if __name__ == "__main__":
    main()
