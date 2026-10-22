#!/usr/bin/env bash
# #60 step (new in W16; #60 a811f92 moved since W13):
#   messages/common/{en,ko}.json: #59's common_discard_* keys and #60's common_error_too_large both added at the end -> keep both.
#   DocumentEditor.svelte edited(): #57 replaced storeDraft(...) with keepDraft() (keeps a reserved, offered draft);
#     #60 a811f92 guards the autosave with `if (!accountChanged)` -> keepDraft(); if (!accountChanged) schedule(...).
#   pinned-value fix (W13 ec7ffe5): #60's save() asks api.me() before sending; #57's two draft tests did not mock it.
set -eu
d=$(cd "$(dirname "$0")" && pwd)
"$d/../../take-resolved.sh" "$d"
cd "$(git rev-parse --show-toplevel)"
git apply --index "$d/pr60-api-me.diff"
