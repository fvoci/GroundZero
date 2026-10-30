#!/usr/bin/env bash
# Regression checks for #264 measurement evidence; no DB or product server required.
# Optional argument: a recorded screens.json instead of the synthetic sample.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
fixture=${1:-tools/fixtures/perf-screens-sample.json}
mkdir -p "$root/target"
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
valid_depth() {
  jq -f tools/perf-depth.jq "$fixture" > "$out/depth.json" || return 1
  jq -e 'group_by(.screen) |
    map({key: .[0].screen, value: (map(.depth) | unique)}) | from_entries == {
      home: [6], "my-tasks": [3], board: [6], gantt: [6], "task-detail": [7],
      "document-tree": [5], "document-open": [4], calendar: [3], activity: [6]
    }' "$out/depth.json"
}
check valid-depth valid_depth
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
dirty_web_inputs() {
  local dirty_input=${1:-apps/web/src/App.svelte}
  local status=0
  bash -euo pipefail -s -- "$root" "$dirty_input" > "$out/dirty-input-result.log" 2>&1 <<'DIRTY' || status=$?
  cd "$1"
  export test_dirty_input=$2
  git() {
    if [[ "${1:-}" == status ]]; then
      [[ "${2:-}" == --porcelain && "${3:-}" == -- ]] || return 1
      shift 3
      local pathspec
      for pathspec in "$@"; do
        if [[ "$test_dirty_input" == "$pathspec" || "$test_dirty_input" == "$pathspec/"* ]]; then
          printf ' M %s\n' "$test_dirty_input"
          return 0
        fi
      done
    else
      command git "$@"
    fi
  }
  docker() { printf 'unexpected Docker invocation\n' >&2; return 99; }
  export -f git docker
  bash tools/measure-perf.sh screens
DIRTY
  if [[ "$status" -ne 2 ]]; then
    printf 'expected dirty input exit 2, got %s\n' "$status"
    return 1
  fi
  grep -Fx 'web inputs are dirty' "$out/dirty-input-result.log"
}
check dirty-web-inputs dirty_web_inputs
check dirty-tsconfig-base dirty_web_inputs tsconfig.base.json
sharing_lifecycle() {
  local scenario=$1
  local case_root="$out/sharing-$scenario"
  mkdir -p "$case_root/tools" "$case_root/crates/server/tests" "$case_root/crates/db/migrations" "$case_root/target/release"
  cp tools/measure-240.sh tools/perf240-run.sh tools/perf240-summary.jq "$case_root/tools/"
  cp crates/server/tests/perf240.rs "$case_root/crates/server/tests/"
  cp crates/db/migrations/*.sql "$case_root/crates/db/migrations/"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$case_root/target/release/fvoci-server"
  chmod +x "$case_root/target/release/fvoci-server"
  local status=0
  bash -euo pipefail -s -- "$case_root" "$scenario" "$(git rev-parse HEAD)" > "$case_root/run.log" 2>&1 <<'SHARING' || status=$?
  export test_sharing_root=$1 test_sharing_scenario=$2 test_sharing_head=$3
  # Inject only the lock pathname into an isolated copy; HOME is never changed.
  script_source=$(cat "$test_sharing_root/tools/measure-240.sh")
  script_source=${script_source/'measure_lock="$HOME/.cache/fvoci-measure.lock"'/'measure_lock="$test_sharing_root/measure.lock"'}
  git() {
    case "$*" in
      'rev-parse --show-toplevel') printf '%s\n' "$test_sharing_root" ;;
      'rev-parse HEAD') printf '%s\n' "$test_sharing_head" ;;
      *) return 99 ;;
    esac
  }
  docker() {
    case " $* " in
      *' down '*)
        if [[ "$test_sharing_scenario" == cleanup || "$test_sharing_scenario" == prior-failure ]]; then return 23; fi ;;
      *' up '*) if [[ "$test_sharing_scenario" == prior-failure ]]; then return 17; fi ;;
      *' port postgres '*) printf '127.0.0.1:1\n' ;;
      *' port meilisearch '*) printf '127.0.0.1:2\n' ;;
    esac
  }
  sleep() {
    if [[ "$test_sharing_scenario" == wait && "$1" == 2 && -e "$test_sharing_root/deferred-build-lock" ]]; then
      local builds=0 waits=0
      if [[ -f "$test_sharing_root/cargo.log" ]]; then builds=$(wc -l < "$test_sharing_root/cargo.log"); fi
      if [[ -f "$test_sharing_root/wait-observed" ]]; then waits=$(wc -l < "$test_sharing_root/wait-observed"); fi
      if [[ "$builds" -ne "$waits" ]]; then touch "$test_sharing_root/premature-build"; fi
      printf 'wait\n' >> "$test_sharing_root/wait-observed"
      rm "$test_sharing_root/measure.lock"
      rm "$test_sharing_root/deferred-build-lock"
    else
      command sleep "$@"
    fi
  }
  cargo() {
    printf '%s\n' "$*" >> "$test_sharing_root/cargo.log"
    if [[ "$test_sharing_scenario" == wait && $(wc -l < "$test_sharing_root/cargo.log") -lt 3 ]]; then
      (set -o noclobber; printf 'another measurement\n' > "$test_sharing_root/measure.lock")
      touch "$test_sharing_root/deferred-build-lock"
    fi
    if [[ " $* " == *' --test perf240 shares '* ]]; then
      if [[ "$test_sharing_scenario" == contamination ]]; then
        touch "$test_sharing_root/build-running"
        local deadline=$((SECONDS + 10))
        until [[ -e "$test_sharing_root/build-observed" ]] &&
          grep -q 'compiler_or_node_build=true' "$FVOCI_MEASURE_DIR/concurrency.log"; do
          if (( SECONDS >= deadline )); then return 98; fi
          command sleep 0.01
        done
        rm "$test_sharing_root/build-running"
      fi
      printf '[]\n' > "$FVOCI_MEASURE_DIR/sharing.json"
    fi
  }
  pgrep() {
    if [[ -e "$test_sharing_root/build-running" ]]; then
      touch "$test_sharing_root/build-observed"
      return 0
    fi
    return 1
  }
  ps() { return 0; }
  export -f git docker sleep cargo pgrep ps
  if [[ "$test_sharing_scenario" == wait ]]; then
    (set -o noclobber; printf 'other measurement\n' > "$test_sharing_root/measure.lock")
    touch "$test_sharing_root/deferred-build-lock"
  fi
  bash -c "$script_source"
SHARING
  case "$scenario" in
    cleanup) test "$status" -eq 23 || return 1 ;;
    prior-failure) test "$status" -eq 17 || return 1 ;;
    contamination)
      test "$status" -eq 1 || return 1
      grep -F 'compiler or node build overlapped sharing measurements' "$case_root/run.log" || return 1 ;;
    clean|wait) test "$status" -eq 0 || return 1 ;;
  esac
  test ! -e "$case_root/measure.lock" || return 1
  if [[ "$scenario" == wait ]]; then
    test -f "$case_root/wait-observed" && test ! -e "$case_root/premature-build" || return 1
    test "$(wc -l < "$case_root/wait-observed")" -eq 3 || return 1
  fi
}
check sharing-clean sharing_lifecycle clean
check sharing-cleanup-failure sharing_lifecycle cleanup
check sharing-prior-failure sharing_lifecycle prior-failure
check sharing-waits-before-build sharing_lifecycle wait
check sharing-rejects-overlapping-build sharing_lifecycle contamination
test "$failures" -eq 0
