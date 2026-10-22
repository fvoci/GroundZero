#!/usr/bin/env bash
# #74 step: every conflicted file has both sides byte-identical to W13 step 1783e046597b's parents -> reuse its resolution.
set -eu
"$(cd "$(dirname "$0")/../.." && pwd)/reuse-ref-resolution.sh" 1783e046597b
