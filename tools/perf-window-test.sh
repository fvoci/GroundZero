#!/usr/bin/env bash
# #291: measurement marker ownership and cleanup; no services or builds.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/target"
out=$(mktemp -d "$root/target/perf-window-test-XXXXXX")
for name in wait_for_measurement acquire_measure_lock cleanup; do
  source_text=$(awk -v name="$name" '$0 == name "() {" {copy=1} copy {print} copy && /^}$/ {exit}' tools/measure-perf.sh)
  test -n "$source_text"
  eval "$source_text"
done
measure_lock="$out/measure.lock"
owns_measure_lock=false
server_pid=
monitor_pid=
fake_compose() { test -f "$measure_lock"; return "${down_status:-0}"; }
compose=(fake_compose)
sleep() {
  test "$1" -eq 15
  test -f "$measure_lock"
  printf 'waited\n' >> "$out/waits"
  rm "$measure_lock"
}
# A preceding measurement must finish before acquisition; it is never overwritten.
printf 'other owner\n' > "$measure_lock"
acquire_measure_lock
test "$owns_measure_lock" = true
test "$(cat "$out/waits")" = waited
test -f "$measure_lock"
cleanup
test ! -e "$measure_lock"
# Failed teardown must still release our marker and propagate the error.
acquire_measure_lock
down_status=7
status=0
cleanup || status=$?
test "$status" -eq 7
test ! -e "$measure_lock"
# An invalid path is rejected, and a marker we do not own survives cleanup.
owns_measure_lock=false
mkdir "$measure_lock"
if acquire_measure_lock; then exit 1; fi
rmdir "$measure_lock"
: > "$measure_lock"
down_status=0
cleanup
test -f "$measure_lock"
rm "$measure_lock"
printf 'PASS measurement window ownership, wait and cleanup\n'
