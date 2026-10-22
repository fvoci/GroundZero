#!/usr/bin/env bash
# #59 step: same two conflicts as W13 step 41fae73.
#   recurrence.rs: both sides byte-identical to 41fae73's parents -> reuse its resolution.
#   development.md: their side identical, our side has #89's newer 01f17fb paragraph -> W13 resolution + that change (3-way).
set -eu
W=$(cd "$(dirname "$0")/../.." && pwd)
"$W/reuse-ref-rebased.sh" 41fae73fef46 docs/development.md
git checkout 41fae73fef46 -- crates/server/tests/recurrence.rs
[ "$(git rev-parse HEAD:crates/server/tests/recurrence.rs)" = "$(git rev-parse 41fae73fef46^1:crates/server/tests/recurrence.rs)" ]
[ "$(git rev-parse MERGE_HEAD:crates/server/tests/recurrence.rs)" = "$(git rev-parse 41fae73fef46^2:crates/server/tests/recurrence.rs)" ]
