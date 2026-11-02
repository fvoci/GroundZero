#!/usr/bin/env bash
# #297 / SR-296-1: preparation builds and the measurement window hand over atomically,
# and a marker left by a dead owner is recovered. Temporary paths only; no builds or services.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
mkdir -p "$root/target"
handoff_out=$(mktemp -d "$root/target/perf-handoff-test-XXXXXX")
script=tools/measure-perf.sh
# Every function of the marker region, as the script defines it.
awk '/^measure_lock=/ {region=1} /^overlay=/ {region=0}
  region && /^[a-z_]+\(\) \{$/ {copy=1} copy {print} copy && /^}$/ {copy=0}' \
  "$script" > "$handoff_out/functions.sh"
test -s "$handoff_out/functions.sh"
# The preparation builds of the script, up to the marker acquisition.
awk '/^# Complete every build before opening the measurement window/ {copy=1}
  copy && /^acquire_measure_lock$/ {exit} copy {print}' "$script" > "$handoff_out/prepare.sh"
grep -q 'cargo build' "$handoff_out/prepare.sh"
grep -q 'cargo test' "$handoff_out/prepare.sh"
lock_paths=$(grep -E '^(measure_lock|measure_token|measure_token_file)=' "$script")
test "$(printf '%s\n' "$lock_paths" | wc -l)" -eq 3
failures=0
report() {
  local name=$1
  shift
  if "$@" > "$handoff_out/$name.log" 2>&1; then
    printf 'PASS %s\n' "$name"
  else
    printf 'FAIL %s (%s)\n' "$name" "$handoff_out/$name.log"
    failures=$((failures + 1))
  fi
}
wait_for_path() {
  local path=$1 deadline=$((SECONDS + $2))
  while [[ ! -e "$path" ]]; do
    if (( SECONDS >= deadline )); then return 1; fi
    command sleep 0.02
  done
}
# A is descheduled right after its idle check, while B opens a window. A's build must not
# start inside B's window, and B's window must not open while A's build runs.
concurrent_preparation() (
  local case_dir="$handoff_out/concurrent"
  mkdir -p "$case_dir/a" "$case_dir/b"
  source "$handoff_out/functions.sh"
  measure_lock="$case_dir/measure.lock"
  mode=rooms
  out="$case_dir/a"
  measure_token="prepare-a-$$"
  measure_token_file="$measure_lock.token-a"
  # Callers may run these stand-ins where errexit is off, so failures exit explicitly.
  sleep() { test "$1" -eq 15 || exit 1; command sleep 0.05; }
  cargo() {
    local fd
    for fd in "/proc/$BASHPID/fd/"*; do
      if [[ "$(readlink "$fd" || true)" == *.prepare ]]; then
        printf 'build inherited the preparation lock\n' >> "$case_dir/violations"
      fi
    done
    if [[ ! -e "$case_dir/a-paused" ]]; then
      touch "$case_dir/a-paused"
      wait_for_path "$measure_lock" 2 || true
    fi
    if [[ -e "$measure_lock" ]]; then printf 'build started inside a window\n' >> "$case_dir/violations"; fi
    touch "$case_dir/building"
    command sleep 0.3
    if [[ -e "$measure_lock" ]]; then printf 'window opened during a build\n' >> "$case_dir/violations"; fi
    rm "$case_dir/building"
    printf '%s\n' "$1" >> "$case_dir/a-builds"
  }
  (
    out="$case_dir/b"
    measure_token="prepare-b-$$"
    measure_token_file="$measure_lock.token-b"
    wait_for_path "$case_dir/a-paused" 5 || exit 1
    acquire_measure_lock || exit 1
    if [[ -e "$case_dir/building" ]]; then printf 'window opened during a build\n' >> "$case_dir/violations"; fi
    touch "$case_dir/b-window"
    command sleep 0.5
    if [[ -e "$case_dir/building" ]]; then printf 'build ran inside the window\n' >> "$case_dir/violations"; fi
    remove_measure_lock
    rm -f "$measure_token_file"
  ) &
  local b_pid=$!
  timeout_after=$((SECONDS + 20))
  source "$handoff_out/prepare.sh"
  wait "$b_pid"
  (( SECONDS < timeout_after ))
  test -e "$case_dir/b-window"
  test "$(cat "$case_dir/a-builds")" = $'build\ntest'
  test ! -e "$measure_lock"
  if [[ -e "$case_dir/violations" ]]; then cat "$case_dir/violations" >&2; exit 1; fi
)
# A marker whose owner PID no longer exists is recovered under the guard, then acquired.
dead_owner_recovered() (
  local case_dir="$handoff_out/dead-owner"
  mkdir -p "$case_dir/home/.cache"
  source "$handoff_out/functions.sh"
  HOME="$case_dir/home"
  eval "$lock_paths"
  [[ "$measure_lock" == "$case_dir/home/.cache/fvoci-measure.lock" ]]
  out="$case_dir"
  local dead_pid stale
  dead_pid=$(bash -c 'printf "%s\n" "$$"')
  if ps -eo pid= | awk -v pid="$dead_pid" '$1 == pid {found=1} END {exit !found}'; then exit 1; fi
  stale="$dead_pid-20261103T000000-1-2"
  printf '%s\n' "$stale" > "$measure_lock.token-$stale"
  ln -T "$measure_lock.token-$stale" "$measure_lock"
  sleep() { printf 'waited for a dead owner\n' >&2; exit 1; }
  rm() {
    if [[ " $* " == *" $measure_lock "* ]]; then
      if flock -n "$measure_lock.guard" true; then printf 'recovery without the guard\n' >&2; return 1; fi
      touch "$case_dir/guarded-recovery"
    fi
    command rm "$@"
  }
  acquire_measure_lock
  test -e "$case_dir/guarded-recovery"
  test "$(cat "$measure_lock")" = "$measure_token"
  test ! -e "$measure_lock.token-$stale"
  remove_measure_lock
  command rm -f "$measure_token_file"
  test ! -e "$measure_lock"
)
# A live owner's marker is never removed; it is recovered only after that owner exits.
live_owner_kept() (
  local case_dir="$handoff_out/live-owner"
  mkdir -p "$case_dir/home/.cache"
  source "$handoff_out/functions.sh"
  HOME="$case_dir/home"
  eval "$lock_paths"
  out="$case_dir"
  local owner live
  command sleep 30 &
  owner=$!
  trap 'kill "$owner" 2>/dev/null || true' EXIT
  live="$owner-20261103T000000-3-4"
  printf '%s\n' "$live" > "$measure_lock.token-$live"
  ln -T "$measure_lock.token-$live" "$measure_lock"
  sleep() {
    test "$1" -eq 15 || exit 1
    test "$(cat "$measure_lock")" = "$live" || exit 1
    printf 'waited\n' >> "$case_dir/waits"
    if [[ "$(wc -l < "$case_dir/waits")" -eq 2 ]]; then
      kill "$owner"
      wait "$owner" || true
    elif [[ "$(wc -l < "$case_dir/waits")" -gt 2 ]]; then
      printf 'kept waiting after the owner exited\n' >&2
      exit 1
    fi
  }
  acquire_measure_lock
  test "$(cat "$case_dir/waits")" = $'waited\nwaited'
  test "$(cat "$measure_lock")" = "$measure_token"
  test ! -e "$measure_lock.token-$live"
  remove_measure_lock
  rm -f "$measure_token_file"
)
# A preparation build also recovers a dead owner's marker before it starts.
prepare_after_dead_owner() (
  local case_dir="$handoff_out/prepare-dead-owner"
  mkdir -p "$case_dir/home/.cache"
  source "$handoff_out/functions.sh"
  HOME="$case_dir/home"
  eval "$lock_paths"
  out="$case_dir"
  mode=rooms
  local dead_pid stale
  dead_pid=$(bash -c 'printf "%s\n" "$$"')
  stale="$dead_pid-20261103T000000-5-6"
  printf '%s\n' "$stale" > "$measure_lock.token-$stale"
  ln -T "$measure_lock.token-$stale" "$measure_lock"
  sleep() { printf 'waited for a dead owner\n' >&2; exit 1; }
  cargo() { test ! -e "$measure_lock" || exit 1; printf '%s\n' "$1" >> "$case_dir/builds"; }
  source "$handoff_out/prepare.sh"
  test "$(cat "$case_dir/builds")" = $'build\ntest'
)
export handoff_out lock_paths
export -f wait_for_path concurrent_preparation dead_owner_recovered live_owner_kept prepare_after_dead_owner
# Each case runs in its own errexit shell; a stuck handoff fails at the timeout.
report concurrent-preparation timeout 30 bash -euo pipefail -c concurrent_preparation
report dead-owner-recovered timeout 30 bash -euo pipefail -c dead_owner_recovered
report live-owner-kept timeout 30 bash -euo pipefail -c live_owner_kept
report prepare-after-dead-owner timeout 30 bash -euo pipefail -c prepare_after_dead_owner
test "$failures" -eq 0
printf 'PASS measurement handoff: concurrent preparation, dead and live owners\n'
