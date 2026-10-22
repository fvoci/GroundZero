#!/usr/bin/env bash
# #82 step (new in W16; #82 4d3ae8a moved since W13):
#   messages/common/{en,ko}.json: #82 adds the same four common_discard_* keys as #59 (identical blob); ours already has them
#     plus #60's common_error_too_large -> take ours.
#   pinned-value fix (W13 fca9be6 + new): #62 dropped TaskDetailView.predecessor_ids; drop it from #82's
#     task-conflict-save-mine fixture and from #82's new task-dialog-discard fixture (svelte-check errors otherwise).
#   The W11 String(estimate_minutes) change that W13 simulated (17df953) is in #82 4d3ae8a itself; nothing simulated here.
set -eu
d=$(cd "$(dirname "$0")" && pwd)
"$d/../../take-resolved.sh" "$d"
cd "$(git rev-parse --show-toplevel)"
git apply --index "$d/pr82-predecessor-ids.diff"
