#!/bin/sh
# D-86: fixed tools in this checkout only; no system or runtime installation.
set -eu
cd "$(git rev-parse --show-toplevel)"
if [ ! -x target/font-tools/bin/python ]; then
  uv venv --python python3 --no-managed-python target/font-tools
fi
uv pip sync --python target/font-tools/bin/python --only-binary=:all: tools/font-tools-requirements.txt
export PYTHONDONTWRITEBYTECODE=1
if [ "${1:-}" = --verify ]; then
  exec target/font-tools/bin/python -B -m unittest discover -s tools -p test_font_assets.py
fi
exec target/font-tools/bin/python -B tools/generate_fonts.py "$@"
