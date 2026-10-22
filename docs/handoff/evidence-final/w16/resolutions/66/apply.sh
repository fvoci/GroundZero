#!/usr/bin/env bash
# #66 step: every conflicted file has both sides byte-identical to W13 step 03fbde7bf491's parents -> reuse its resolution.
set -eu
"$(cd "$(dirname "$0")/../.." && pwd)/reuse-ref-resolution.sh" 03fbde7bf491
