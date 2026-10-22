#!/usr/bin/env bash
# #61 step: #59 (merged before) adds HomeView.up_next_counts; #61's home-week-dates fixture lacks it (W13 548e5ab).
set -eu
cd "$(git rev-parse --show-toplevel)"
git apply --index "$(dirname "$0")/pr61.diff"
