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
trap_source=$(awk '/^trap / {print}' tools/measure-perf.sh)
export trap_source
measure_lock="$out/measure.lock"
measure_token="window-test-$$-$RANDOM"
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
test "$(cat "$measure_lock")" = "$measure_token"
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
mkdir "$measure_lock"
if acquire_measure_lock; then exit 1; fi
rmdir "$measure_lock"
: > "$measure_lock"
down_status=0
cleanup
test -f "$measure_lock"
rm "$measure_lock"
failures=0
foreign_token() (
  measure_token="window-test-$$-$RANDOM"
  acquire_measure_lock || exit 1
  printf 'replacement-owner\n' > "$measure_lock"
  cleanup
  test -f "$measure_lock" && test "$(cat "$measure_lock")" = replacement-owner
)
if foreign_token; then
  printf 'PASS replacement token survives cleanup\n'
else
  printf 'FAIL replacement token survives cleanup\n'
  failures=$((failures + 1))
fi
signal_after_write() {
  local signal=$1 expected=$2 status=0
  local signal_lock="$out/signal-$signal.lock"
  export -f wait_for_measurement acquire_measure_lock cleanup
  bash -euo pipefail -s -- "$out" "$signal_lock" "$signal" <<'SIGNAL' || status=$?
  out=$1
  measure_lock=$2
  test_signal=$3
  measure_token="signal-test-$$-$RANDOM"
  server_pid=
  monitor_pid=
  fake_compose() { test -f "$measure_lock"; }
  compose=(fake_compose)
  eval "$trap_source"
  # Inject the signal at the end of the actual write builtin, while its
  # acquisition subshell still runs and before the parent can publish a flag.
  :() { builtin :; kill -s "$test_signal" "$$"; }
  printf() { builtin printf "$@"; kill -s "$test_signal" "$$"; }
  acquire_measure_lock
SIGNAL
  test "$status" -eq "$expected" && test ! -e "$signal_lock"
}
for signal in TERM INT; do
  expected=143
  if [[ "$signal" == INT ]]; then expected=130; fi
  if signal_after_write "$signal" "$expected"; then
    printf 'PASS %s immediately after lock write\n' "$signal"
  else
    printf 'FAIL %s immediately after lock write\n' "$signal"
    failures=$((failures + 1))
  fi
done
test "$failures" -eq 0
printf 'PASS measurement window ownership, wait and cleanup\n'
