#!/bin/sh
# Read only approved Git inputs; write candidate fonts/evidence under ignored target/.
# Uses the installed Python and uv; never installs system packages or a Python runtime.
set -eu
cd "$(git rev-parse --show-toplevel)"
if [ ! -x target/font-tools/bin/python ]; then
  uv venv --python python3 --no-managed-python target/font-tools
fi
uv pip sync --python target/font-tools/bin/python --only-binary=:all: tools/font-tools-requirements.txt
exec target/font-tools/bin/python tools/probe-font-woff2.py
