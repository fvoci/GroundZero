#!/usr/bin/env bash
# Regression checks for #264 measurement evidence; no DB or product server required.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
fixture=${1:-target/perf-screens-10-SS7FC9/screens.json}
out=$(mktemp -d "$root/target/perf-tools-test-XXXXXX")
failures=0
printf 'evidence: %s\n' "$out"
check() {
  local name=$1
  shift
  if "$@" > "$out/$name.log" 2>&1; then
    printf 'PASS %s\n' "$name"
  else
    printf 'FAIL %s\n' "$name"
    failures=$((failures + 1))
  fi
}
reject_gate() {
  local kind=$1
  local path=$2
  if [[ "$kind" == missing ]]; then
    jq --arg path "$path" '.[0].requests |= map(select(.path != $path))' "$fixture"
  else
    jq --arg path "$path" '
      (.[0].requests | map(select(.path == "/api/me")) | min_by(.start).end) as $end |
      .[0].requests |= map(if .path == $path then .start = ($end - 1) else . end)' "$fixture"
  fi > "$out/input.json"
  if jq -f tools/perf-depth.jq "$out/input.json" > /dev/null 2> "$out/gate-error.log"; then
    return 1
  fi
  grep -E 'missing session gate request|session gate order changed' "$out/gate-error.log"
}
cleanup_dead_monitor() {
  bash -euo pipefail -s -- "$root" "$out" <<'CLEANUP'
  cd "$1"
  out=$2
  # Evaluate the real cleanup function with a recording Compose stand-in.
  cleanup_source=$(awk '/^cleanup\(\) \{$/ {copy=1} copy {print} copy && /^}$/ {exit}' tools/measure-perf.sh)
  test -n "$cleanup_source"
  server_pid=
  sleep 0 &
  monitor_pid=$!
  wait "$monitor_pid"
  fake_compose() { printf '%s\n' "$*" > "$out/compose-called"; }
  compose=(fake_compose)
  eval "$cleanup_source"
  cleanup
  test "$(cat "$out/compose-called")" = 'down --volumes'
CLEANUP
}
check valid-depth jq -f tools/perf-depth.jq "$fixture"
check missing-workspaces reject_gate missing /api/workspaces
check missing-organizations reject_gate missing /api/me/organizations
check early-workspaces reject_gate early /api/workspaces
check early-organizations reject_gate early /api/me/organizations
check cleanup-dead-monitor cleanup_dead_monitor
replay_arguments() {
  jq -n '[
    {screen:"home",repeat:1,requests:[{method:"GET",path:"/api/me/home",start:1,requestId:"home-evidence",query:{organization_id:"org-home",tz:"America/New_York"}}]},
    {screen:"my-tasks",repeat:1,requests:[{method:"GET",path:"/api/me/tasks",start:2,requestId:"tasks-evidence",query:{organization_id:"org-tasks",today:"2031-12-02"}}]},
    {screen:"calendar",repeat:1,requests:[{method:"GET",path:"/api/workspaces/ws/calendar",start:3,requestId:"calendar-evidence",query:{from:"2031-11-24",to:"2032-01-04",tz:"Europe/Berlin",module_id:"module",item_id:"item"}}]}
  ]' > "$out/replay-input.json"
  jq -f tools/perf-arguments.jq "$out/replay-input.json" > "$out/replay-arguments.json" || return 1
  jq -e '.home_org == "org-home" and .home_tz == "America/New_York" and
    .tasks_org == "org-tasks" and .today == "2031-12-02" and
    .calendar_workspace == "ws" and .calendar_from == "2031-11-24" and
    .calendar_to == "2032-01-04" and .calendar_tz == "Europe/Berlin" and
    .calendar_module == "module" and .calendar_item == "item" and
    .evidence.calendar == "calendar-evidence"' "$out/replay-arguments.json" || return 1
  jq '.[1].requests[0].query |= del(.today)' "$out/replay-input.json" > "$out/replay-missing.json"
  if jq -f tools/perf-arguments.jq "$out/replay-missing.json" > /dev/null 2> "$out/replay-error.log"; then
    return 1
  fi
  grep -F 'missing replay argument: today' "$out/replay-error.log"
}
check browser-replay-arguments replay_arguments
test "$failures" -eq 0
