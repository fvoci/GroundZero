#!/usr/bin/env bash
# #62 step: #62 drops TaskView.predecessor_ids; #58's item-screen-form fixture (merged before) still sets it (W13 bdb35e3).
set -eu
cd "$(git rev-parse --show-toplevel)"
git apply --index "$(dirname "$0")/pr62.diff"
