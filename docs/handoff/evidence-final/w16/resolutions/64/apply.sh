#!/usr/bin/env bash
# #64 step (new in W16; #60 a811f92 and #64 2acebe9 both moved since W13). Rationale: ../../README.md section 1.2.
#   LiveDocumentEditor.svelte: keepDraft() takes #60's version (stores `latest ?? start.body`, title-only edits too; returns
#     boolean as both sides do). Both new helpers kept (#60 hasPending, #64 unsaved/keepUnsaved). connection-close handles
#     4412 (`account`) with #60's logic before #64's keepUnsaved(), so a 4412 that cannot keep the draft does not reload and
#     says so (#60 live-account-changed test) while 4403/4409 keep #64's kept/lost notices (#64 live-unsaved-close test).
#   http/mod.rs tests: keep #60's save-conflict/expected-user contract test and #64's move/restore conflict tests.
#   pinned-value fix (W13 2427544): #64 makes RevisionView.edited_by_names required; #57's revision-compare-race fixture.
set -eu
d=$(cd "$(dirname "$0")" && pwd)
"$d/../../take-resolved.sh" "$d"
cd "$(git rev-parse --show-toplevel)"
git apply --index "$d/pr64-edited-by-names.diff"
# coordinator follow-up (06:45Z): #64's development.md W1b line read as if the recheck approach were a user decision.
#   The contention test, the bound proof and the unconfirmed-edit check were the user's added instruction; the recheck
#   approach (option B) is the coordinator's judgment (existing principle: revocation lands within the recheck interval).
git apply --index "$d/pr64-w1b-wording.diff"
