#!/usr/bin/env bash
# #56 step: #62 (merged before) dropped TaskDetailView.predecessor_ids; #56's task-csv-import fixture still sets it (W13 5e188f9).
set -eu
cd "$(git rev-parse --show-toplevel)"
git apply --index "$(dirname "$0")/pr56.diff"
