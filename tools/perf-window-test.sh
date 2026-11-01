#!/usr/bin/env bash
# #291: measurement marker ownership and cleanup; no services or builds.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/target"
out=$(mktemp -d "$root/target/perf-window-test-XXXXXX")
for name in wait_for_measurement publish_measure_lock remove_measure_lock acquire_measure_lock cleanup; do
  source_text=$(awk -v name="$name" '$0 == name "() {" {copy=1} copy {print} copy && /^}$/ {exit}' tools/measure-perf.sh)
  test -n "$source_text"
  eval "$source_text"
done
trap_source=$(awk '/^trap / {print}' tools/measure-perf.sh)
export trap_source
measure_lock="$out/measure.lock"
measure_token="window-test-$$-$RANDOM"
measure_token_file="$measure_lock.token"
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
  export -f wait_for_measurement publish_measure_lock remove_measure_lock acquire_measure_lock cleanup
  bash -euo pipefail -s -- "$out" "$signal_lock" "$signal" <<'SIGNAL' || status=$?
  out=$1
  measure_lock=$2
  test_signal=$3
  measure_token="signal-test-$$-$RANDOM"
  measure_token_file="$measure_lock.token"
  server_pid=
  monitor_pid=
  fake_compose() { return 0; }
  compose=(fake_compose)
  eval "$trap_source"
  # Interrupt after completing the private token, before its publication.
  :() { builtin :; kill -s "$test_signal" "$$"; }
  printf() { builtin printf "$@"; kill -s "$test_signal" "$$"; }
  acquire_measure_lock
SIGNAL
  test "$status" -eq "$expected" && test ! -e "$signal_lock" && test ! -e "$signal_lock.token"
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
signal_before_write() {
  local signal=$1 expected=$2 status=0
  local signal_lock="$out/before-write-$signal.lock"
  export -f wait_for_measurement publish_measure_lock remove_measure_lock acquire_measure_lock cleanup
  bash -euo pipefail -s -- "$out" "$signal_lock" "$signal" "$expected" <<'BEFORE' || status=$?
  out=$1
  measure_lock=$2
  test_signal=$3
  expected=$4
  measure_token="before-write-$$-$RANDOM"
  measure_token_file="$measure_lock.token"
  server_pid=
  monitor_pid=
  fake_compose() { return 0; }
  compose=(fake_compose)
  eval "$trap_source"
  printf() {
    kill -s "$test_signal" "$$"
    kill -s "$test_signal" "$BASHPID"
    exit "$expected"
  }
  acquire_measure_lock
BEFORE
  test "$status" -eq "$expected" && test ! -e "$signal_lock" && test ! -e "$signal_lock.token"
}
for signal in TERM INT; do
  expected=143
  if [[ "$signal" == INT ]]; then expected=130; fi
  if signal_before_write "$signal" "$expected"; then
    printf 'PASS %s to parent and writer before token write\n' "$signal"
  else
    printf 'FAIL %s to parent and writer before token write\n' "$signal"
    failures=$((failures + 1))
  fi
done
replacement_after_comparison() (
  local case_dir="$out/replace-after-read"
  mkdir "$case_dir"
  measure_lock="$case_dir/measure.lock"
  measure_token=original-owner
  acquire_measure_lock || exit 1
  printf 'replacement-owner\n' > "$case_dir/replacement-token"
  cat() {
    command cat "$@" || return
    bash -euo pipefail -s -- "$case_dir" "$measure_lock" 9>&- > "$case_dir/worker.log" 2>&1 <<'REPLACE' &
    case_dir=$1
    measure_lock=$2
    (
      if ! flock -n 9; then
        touch "$case_dir/blocked"
        flock -x -w 5 9
      fi
      rm -f "$measure_lock"
      ln -T "$case_dir/replacement-token" "$measure_lock"
      touch "$case_dir/replaced"
    ) 9>> "$measure_lock.guard"
REPLACE
    local deadline=$((SECONDS + 5))
    while [[ ! -e "$case_dir/blocked" && ! -e "$case_dir/replaced" ]]; do
      if (( SECONDS >= deadline )); then return 1; fi
      command sleep 0.01
    done
  }
  cleanup || exit 1
  local deadline=$((SECONDS + 5))
  while [[ ! -e "$case_dir/replaced" ]]; do
    if (( SECONDS >= deadline )); then exit 1; fi
    command sleep 0.01
  done
  test -f "$measure_lock" && test "$(command cat "$measure_lock")" = replacement-owner
)
if replacement_after_comparison; then
  printf 'PASS replacement after comparison survives cleanup\n'
else
  printf 'FAIL replacement after comparison survives cleanup\n'
  failures=$((failures + 1))
fi
publication_guarded() (
  measure_lock="$out/publication.lock"
  measure_token=publication-owner
  measure_token_file="$measure_lock.token"
  ln() {
    if flock -n "$measure_lock.guard" true; then return 1; fi
    touch "$out/publication-guarded"
    command ln "$@"
  }
  acquire_measure_lock || exit 1
  test -f "$out/publication-guarded" || exit 1
  cleanup || exit 1
  test -f "$measure_lock.guard" && test ! -e "$measure_lock" && test ! -e "$measure_token_file"
)
if publication_guarded; then
  printf 'PASS publication holds the same persistent guard\n'
else
  printf 'FAIL publication holds the same persistent guard\n'
  failures=$((failures + 1))
fi
signal_after_publication() {
  local signal=$1 expected=$2 status=0
  local signal_lock="$out/after-link-$signal.lock"
  timeout 5 bash -euo pipefail -s -- "$out" "$signal_lock" "$signal" "$expected" <<'PUBLISHED' || status=$?
  out=$1
  measure_lock=$2
  test_signal=$3
  expected=$4
  measure_token="published-$$-$RANDOM"
  measure_token_file="$measure_lock.token"
  server_pid=
  monitor_pid=
  fake_compose() { return 0; }
  compose=(fake_compose)
  eval "$trap_source"
  ln() {
    command ln "$@" || return
    kill -s "$test_signal" "$$"
    kill -s "$test_signal" "$BASHPID"
    exit "$expected"
  }
  acquire_measure_lock
PUBLISHED
  test "$status" -eq "$expected" && test ! -e "$signal_lock" && test ! -e "$signal_lock.token"
}
for signal in TERM INT; do
  expected=143
  if [[ "$signal" == INT ]]; then expected=130; fi
  if signal_after_publication "$signal" "$expected"; then
    printf 'PASS %s after publication releases guard and marker\n' "$signal"
  else
    printf 'FAIL %s after publication releases guard and marker\n' "$signal"
    failures=$((failures + 1))
  fi
done
test "$failures" -eq 0
printf 'PASS measurement window ownership, wait and cleanup\n'
