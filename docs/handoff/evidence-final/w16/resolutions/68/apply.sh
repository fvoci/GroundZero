#!/usr/bin/env bash
# #68 step: same six conflicts as W13 step 66e131f.
#   five test files: both sides byte-identical to 66e131f's parents -> reuse its resolution.
#   development.md: their side identical, our side newer (#89 01f17fb, #60 a811f92, #64 2acebe9 sections) -> W13 resolution
#     plus our newer changes (3-way merge-file).
set -eu
W=$(cd "$(dirname "$0")/../.." && pwd)
"$W/reuse-ref-rebased.sh" 66e131fc6841 docs/development.md
for f in comments home members time_tracking trash; do
  p=crates/server/tests/$f.rs
  [ "$(git rev-parse HEAD:$p)" = "$(git rev-parse 66e131fc6841^1:$p)" ]
  [ "$(git rev-parse MERGE_HEAD:$p)" = "$(git rev-parse 66e131fc6841^2:$p)" ]
  git checkout 66e131fc6841 -- "$p"
done
