#!/usr/bin/env bash
# #69 step (new in W16): docs/development.md — #60 a811f92 appends section 96 (expected user) and #69 4355447 appends
# section 95 (code-wide audit) at the same place, before "참고 자료". Keep both, in section order: 95 (theirs) then 96 (ours).
# Recorded file first; when only our side moved (e.g. the #64 wording fix), carry the first build's resolution
# (w16-build1 = 5ddece5) forward with a 3-way merge of only the newer change.
set -eu
d=$(cd "$(dirname "$0")" && pwd)
"$d/../../take-resolved.sh" "$d" || "$d/../../reuse-ref-rebased.sh" 5ddece5da62ade2aa4baddb0dcb5130195f83505 docs/development.md
