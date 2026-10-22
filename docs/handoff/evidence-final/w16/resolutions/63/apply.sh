#!/usr/bin/env bash
# #63 step: every conflicted file has both sides byte-identical to W13 step b52b08971172's parents -> reuse its resolution.
set -eu
"$(cd "$(dirname "$0")/../.." && pwd)/reuse-ref-resolution.sh" b52b08971172
