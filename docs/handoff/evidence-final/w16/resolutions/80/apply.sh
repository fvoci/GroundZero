#!/usr/bin/env bash
# #80 step: every conflicted file has both sides byte-identical to W13 step b71fce05206f's parents -> reuse its resolution.
set -eu
"$(cd "$(dirname "$0")/../.." && pwd)/reuse-ref-resolution.sh" b71fce05206f
