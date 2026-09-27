#!/bin/sh
# Check the Python implementation out of the v1.1.0 tag so the conformance suite can
# still run the reference next to the Rust port.
#
#     sh conformance/fetch_reference.sh
#
# Creates reference/python/{src,requirements.txt} plus a virtualenv at
# reference/python/.venv, which is what conformance/harness.py expects for --impl py.
set -eu

REPO=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TAG=${OCTO_REFERENCE_TAG:-v1.1.0}
DEST="$REPO/reference/python"
# The reference's wire bytes depend on the interpreter: from Python 3.14 (or any build
# with a `zstd` module) urllib3 advertises `zstd` in Accept-Encoding, while the shipped
# image (python:3.9-slim) and the recorded evidence (Python 3.11.15) send exactly
# "gzip, deflate".  Prefer an interpreter that matches the shipped image.
if [ -n "${OCTO_PYTHON:-}" ]; then
    PYTHON="$OCTO_PYTHON"
else
    PYTHON=""
    for candidate in python3.11 python3.12 python3.10 python3.9 python3.13 python3; do
        if command -v "$candidate" >/dev/null 2>&1; then PYTHON=$candidate; break; fi
    done
fi
if [ -z "$PYTHON" ]; then
    echo "no python3 interpreter found" >&2
    exit 1
fi

mkdir -p "$DEST"
git -C "$REPO" archive "$TAG" src requirements.txt | tar -x -C "$DEST"

if [ ! -x "$DEST/.venv/bin/python" ]; then
    echo "creating the reference virtualenv with $PYTHON"
    "$PYTHON" -m venv "$DEST/.venv"
fi

case "$("$DEST/.venv/bin/python" -c 'import sys; print("%d.%d" % sys.version_info[:2])')" in
    3.14|3.15|3.16)
        echo "warning: this interpreter advertises zstd in Accept-Encoding, so the" >&2
        echo "         reference's HTTP headers will differ from the recorded evidence." >&2
        ;;
esac
"$DEST/.venv/bin/pip" install -q -r "$DEST/requirements.txt"

echo "reference ($TAG) ready at $DEST"
echo "run the comparison with:"
echo "  python3 conformance/run_all.py --impl py --out /tmp/art/py"
echo "  python3 conformance/run_all.py --impl rs --out /tmp/art/rs"
echo "  python3 conformance/compare.py --reference /tmp/art/py --candidate /tmp/art/rs"
