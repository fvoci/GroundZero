#!/usr/bin/env bash
# #291 / D-42: detector errors and dead monitors cannot certify quiet samples.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
monitor_test_out=$(mktemp -d "$(pwd)/target/perf-monitor-test-XXXXXX")
awk '/^build_state\(\)/ {copy=1} copy && (/^wait_for_builds$/ || /^start_monitor$/) {exit} copy {print}' \
  tools/measure-perf.sh > "$monitor_test_out/functions.sh"
test -s "$monitor_test_out/functions.sh"
run_case() (
  scenario=$1
  out="$monitor_test_out/$scenario"
  mkdir -p "$out"
  pgrep() { return 1; }
  ps() { printf 'bash bash idle\n'; }
  # The old implementation's initial while is harmless with these quiet boundaries.
  source "$monitor_test_out/functions.sh"
  monitor_pid=
  monitor_phase=screen
  trap 'if [[ -n "$monitor_pid" ]]; then kill "$monitor_pid" 2>/dev/null || true; wait "$monitor_pid" 2>/dev/null || true; fi' EXIT
  sleep() {
    if [[ "$1" == 15 ]]; then touch "$out/wait-observed"; fi
    command sleep 0.01
  }
  case "$scenario" in
    quiet)
      wait_for_builds
      start_monitor
      command sleep 0.05
      stop_monitor
      test -s "$out/concurrency.log" && test ! -e "$out/monitor-error"
      return ;;
    busy-then-quiet)
      printf '0\n' > "$out/calls"
      ps() {
        local count
        count=$(cat "$out/calls")
        printf '%s\n' "$((count+1))" > "$out/calls"
        if [[ "$count" == 0 ]]; then printf 'cargo cargo xtask check\n'; else printf 'bash bash idle\n'; fi
      }
      wait_for_builds
      test -e "$out/wait-observed" && test "$(cat "$out/calls")" -eq 2
      return ;;
    preflight-ps|monitor-ps) ps() { return 23; } ;;
    preflight-awk|monitor-awk) awk() { return 23; } ;;
    preflight-jq|monitor-jq) jq() { return 23; } ;;
    monitor-invalid) awk() { printf '{"compiler":false,"nodeBuild":false,"heavyCommand":"false"}\n'; } ;;
    monitor-record-jq)
      printf '0\n' > "$out/jq-calls"
      jq() {
        local count
        count=$(( $(cat "$out/jq-calls")+1 ))
        printf '%s\n' "$count" > "$out/jq-calls"
        if (( count==3 )); then return 23; fi
        command jq "$@"
      } ;;
    monitor-write) mkdir "$out/concurrency.log" ;;
    monitor-restart)
      ps() {
        if [[ -e "$out/become-busy" ]]; then printf 'cargo cargo xtask check\n'; else printf 'bash bash idle\n'; fi
      }
      start_monitor
      command sleep 0.05
      stop_monitor
      test ! -e "$out/build-overlap"
      monitor_phase=db
      start_monitor
      command sleep 0.03
      touch "$out/become-busy"
      command sleep 0.15
      stop_monitor
      test -e "$out/db-build-overlap"
      command jq -se 'any(.[]; .heavyCommand)' "$out/concurrency.log" > /dev/null
      return ;;
    monitor-external-exit)
      start_monitor
      for ((attempt=0;attempt<100;attempt++)); do
        [[ -s "$out/concurrency.log" ]] && break
        command sleep 0.01
      done
      test -s "$out/concurrency.log"
      kill -TERM "$monitor_pid"
      command sleep 0.02
      if stop_monitor; then printf 'FAIL terminated monitor accepted\n' >&2; return 1; fi
      test -e "$out/monitor-error"
      return ;;
  esac
  if [[ "$scenario" == preflight-* ]]; then
    if wait_for_builds; then printf 'FAIL detector error accepted: %s\n' "$scenario" >&2; return 1; fi
  else
    start_monitor
    command sleep 0.05
    if stop_monitor; then printf 'FAIL monitor error accepted: %s\n' "$scenario" >&2; return 1; fi
    test -e "$out/monitor-error"
  fi
)
for monitor_case in monitor-ps quiet busy-then-quiet preflight-ps preflight-awk preflight-jq monitor-awk monitor-jq monitor-invalid monitor-record-jq monitor-write monitor-restart monitor-external-exit; do
  run_case "$monitor_case"
done
printf 'PASS monitor lifecycle: 13 quiet/busy/error/restart/termination cases\n'
