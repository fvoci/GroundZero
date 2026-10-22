#!/usr/bin/env bash
# #55 step: every conflicted file has both sides byte-identical to W13 step e17565c20641's parents -> reuse its resolution.
set -eu
"$(cd "$(dirname "$0")/../.." && pwd)/reuse-ref-resolution.sh" e17565c20641
