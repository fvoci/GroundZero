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
      home: [4], "my-tasks": [3], board: [6], gantt: [6], "task-detail": [7],
      "document-tree": [5], "document-open": [4], calendar: [3], activity: [4]
    }' "$out/depth.json"
}
check valid-depth valid_depth
check missing-workspaces reject_gate missing /api/workspaces
check missing-organizations reject_gate missing /api/me/organizations
check early-workspaces reject_gate early /api/workspaces
check early-organizations reject_gate early /api/me/organizations
reject_branch() {
  local screen=$1 pattern=$2 kind=$3
  jq --arg screen "$screen" --arg pattern "$pattern" --arg kind "$kind" '
    map(if .screen == $screen then
      .requests |= (if $kind == "missing" then map(select(.path | test($pattern) | not))
        else map(if .path | test($pattern) then .start = 0 else . end) end)
    else . end)' "$fixture" > "$out/branch-input.json"
  if jq -f tools/perf-depth.jq "$out/branch-input.json" > /dev/null 2> "$out/branch-error.log"; then
    return 1
  fi
  grep -F 'dependency witness' "$out/branch-error.log"
}
reject_join() {
  local screen=$1 pattern=$2
  jq --arg screen "$screen" --arg pattern "$pattern" '
    map(if .screen == $screen then
      .requests |= map(if .path | test($pattern) then .end = 999999999999999 else . end)
    else . end)' "$fixture" > "$out/join-input.json"
  if jq -f tools/perf-depth.jq "$out/join-input.json" > /dev/null 2> "$out/join-error.log"; then
    return 1
  fi
  grep -F 'dependency witness' "$out/join-error.log"
}
for kind in missing early; do
  for screen in home activity; do
    check "$kind-$screen-calendar" reject_branch "$screen" '/calendar$' "$kind"
    check "$kind-$screen-notifications" reject_branch "$screen" '^/api/me/notifications$' "$kind"
    check "$kind-$screen-activity" reject_branch "$screen" '^/api/me/activity$' "$kind"
  done
  for screen in board gantt task-detail document-tree; do
    check "$kind-$screen-item" reject_branch "$screen" '^/api/items/[^/]+$' "$kind"
    check "$kind-$screen-documents" reject_branch "$screen" '^/api/items/[^/]+/documents$' "$kind"
    check "$kind-$screen-module" reject_branch "$screen" '^/api/modules/[^/]+$' "$kind"
  done
done
for screen in board gantt task-detail document-tree; do
  check "unfinished-$screen-documents" reject_join "$screen" '^/api/items/[^/]+/documents$'
  check "unfinished-$screen-module" reject_join "$screen" '^/api/modules/[^/]+$'
done
reject_serialized() {
  local screen=$1 kind=$2
  local input="$out/serialized-$screen-$kind.json"
  jq --arg screen "$screen" --arg kind "$kind" '
    def home_children:
      to_entries | map(select(.value.path | test("^/api/(workspaces/[^/]+/calendar|me/(notifications|activity))$")));
    map(if .screen != $screen then . else
      .requests |= (
        . as $requests |
        if $kind == "documents" then
          (to_entries | map(select(.value.path | test("^/api/items/[^/]+$"))) | min_by(.value.start)) as $item |
          (to_entries | map(select(.value.path | test("^/api/items/[^/]+/documents$")))) as $documents |
          (map(select(.path | test("^/api/modules/[^/]+$"))) | min_by(.start)) as $module |
          ([$documents[].value.end, $module.end] | max) as $old_join |
          (reduce $documents[] as $document ($requests;
            ($document.value.end - $document.value.start) as $duration |
            .[$document.key].start = $item.value.end |
            .[$document.key].end = ($item.value.end + $duration))) as $delayed |
          ([$documents[] | .key as $key | $delayed[$key].end] + [$module.end] | max) as $new_join |
          # Move downstream calls with the join: rejection must be for lost
          # overlap, rather than accidentally breaking an existing parent edge.
          $delayed | to_entries | map(
            if (.key as $key | any($documents[]; .key == $key)) or .value.start < $old_join
            then .value
            else .value | .start += ($new_join - $old_join) | .end += ($new_join - $old_join) end)
        else
          home_children as $children |
          (map(select(.path == "/api/me/home")) | min_by(.start).end) as $parent_end |
          if $kind == "home" then
            ($children | sort_by(if .value.path | endswith("/calendar") then 0
              elif .value.path == "/api/me/notifications" then 1 else 2 end)) as $ordered |
            reduce $ordered[] as $child ({requests: $requests, after: $parent_end};
              ($child.value.end - $child.value.start) as $duration |
              .requests[$child.key].start = .after |
              .requests[$child.key].end = (.after + $duration) |
              .after += $duration) | .requests
          else
            ($children | map(select(.value.path | endswith("/" + (if $kind == "first" then "calendar" else $kind end)))) | min_by(.value.start)) as $target |
            if $kind == "first" then
              ($parent_end + $target.value.end - $target.value.start) as $first_end |
              reduce $children[] as $child ($requests;
                ($child.value.end - $child.value.start) as $duration |
                (if $child.key == $target.key then $parent_end else $first_end end) as $start |
                .[$child.key].start = $start | .[$child.key].end = ($start + $duration))
            else
              ($children | map(select(.key != $target.key) | .value.end) | max) as $siblings_end |
              .[$target.key].start = $siblings_end |
              .[$target.key].end = ($siblings_end + $target.value.end - $target.value.start)
            end
          end
        end)
    end)' "$fixture" > "$input"
  if jq -f tools/perf-depth.jq "$input" > /dev/null 2> "$out/serialized-error.log"; then
    printf 'accepted serialized fixture: %s %s\n' "$screen" "$kind"
    return 1
  fi
  grep -F 'non-overlapping dependency witness' "$out/serialized-error.log"
}
for screen in home activity; do
  check "serialized-$screen" reject_serialized "$screen" home
  # An overlapping pair must not hide another branch that runs by itself.
  for branch in calendar notifications activity first; do
    check "isolated-$screen-$branch" reject_serialized "$screen" "$branch"
  done
done
for screen in board gantt task-detail document-tree; do
  check "serialized-$screen-documents" reject_serialized "$screen" documents
done
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
test "$failures" -eq 0
