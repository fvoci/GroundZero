#!/usr/bin/env bash
# D-131 / #210 / #181. Run under the host DB lock and fvoci-slot.sh.
# Usage: bash tools/measure-perf.sh rooms|screens [10|100|1000]
set -euo pipefail
umask 077
# Do not inherit a development DB, storage backend, OIDC or search service.
for key in "${!FVOCI_@}" "${!DATABASE_@}"; do
  if [[ -n "$key" ]]; then unset "$key"; fi
done
mode=${1:?rooms or screens}
count=${2:-10}
case "$mode" in rooms|screens) ;; *) exit 2 ;; esac
case "$count" in 10|100|1000) ;; *) exit 2 ;; esac
root=$(git rev-parse --show-toplevel)
cd "$root"
check_web_inputs() {
  local current_inputs
  current_inputs=$(git status --porcelain -- apps/web package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json) || return
  if [[ -n "$current_inputs" || "$current_inputs" != "${1:-}" ]]; then
    printf 'web inputs are dirty\n' >&2
    exit 2
  fi
  printf '%s' "$current_inputs"
}
if [[ "$mode" == screens ]]; then
  check_web_inputs
fi
out=$(mktemp -d "$root/target/perf-${mode}-${count}-XXXXXX")
project="fvoci-perf-w1-${mode}-${count}-$$"
overlay="$out/postgres.yaml"
if [[ "$mode" == screens ]]; then
  cat > "$overlay" <<'YAML'
services:
  postgres:
    command: ['postgres', '-c', 'max_connections=200', '-c', 'shared_preload_libraries=pg_stat_statements', '-c', 'pg_stat_statements.track=all', '-c', 'track_functions=all']
YAML
else
  printf 'services: {}\n' > "$overlay"
fi
compose=(docker compose -p "$project" -f deploy/compose.test.yaml -f "$overlay")
server_pid=
monitor_pid=
measure_lock="$HOME/.cache/fvoci-measure.lock"
owns_measure_lock=false
cleanup() {
  local status=$? cleanup_status=0
  trap - EXIT
  if [[ -n "$server_pid" ]]; then
    touch "$out/STOP"
    wait "$server_pid" || true
  fi
  if [[ -n "$monitor_pid" ]]; then kill "$monitor_pid" 2>/dev/null || true; wait "$monitor_pid" || true; fi
  "${compose[@]}" down --volumes > "$out/cleanup.log" 2>&1 || cleanup_status=$?
  if "$owns_measure_lock"; then rm -f "$measure_lock" || cleanup_status=$?; fi
  if (( status == 0 && cleanup_status != 0 )); then status=$cleanup_status; fi
  exit "$status"
}
trap cleanup EXIT
printf '%s\n' "$out"
git rev-parse HEAD > "$out/sha.txt"
wait_for_measurement() {
  while [[ -e "$measure_lock" || -L "$measure_lock" ]]; do sleep 15; done
}
wait_for_measurement
# Warm every executable before acquiring the sampling marker, including serve_t6.
cargo build --release --locked -p fvoci-server --bin fvoci-server > "$out/build-server.log" 2>&1
cargo test --release --locked -p fvoci-server --test it --no-run > "$out/build-it.log" 2>&1
if [[ "$mode" == screens ]]; then
  cargo test --release --locked -p fvoci-server --test screen_measure --no-run > "$out/build-screen.log" 2>&1
fi
"${compose[@]}" up -d --wait postgres > "$out/compose.log" 2>&1
port=$("${compose[@]}" port postgres 5432)
export DATABASE_URL="postgres://fvoci_app:fvoci-test-app-password@$port/fvoci"
export FVOCI_TEST_ADMIN_URL="postgres://fvoci:fvoci-test-password@$port/fvoci"
export FVOCI_TEST_MIGRATION_URL="postgres://fvoci_migrator:fvoci-test-migrator-password@$port/fvoci"
export FVOCI_MEASURE_DIR="$out"
# Only the explicit test credentials from deploy/compose.test.yaml are used.
export DATABASE_ADMIN_URL="$FVOCI_TEST_ADMIN_URL"
export DATABASE_MIGRATION_URL="$FVOCI_TEST_MIGRATION_URL"
export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password
export FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
target/release/fvoci-server db bootstrap > "$out/bootstrap.log" 2>&1
target/release/fvoci-server db migrate > "$out/migrate.log" 2>&1
cargo test --release --locked -p fvoci-server --test it restore_measure::restore_measure_generate -- --ignored --exact --nocapture > "$out/generate.log" 2>&1
if [[ "$mode" == screens ]]; then
  web_inputs_before=$(check_web_inputs)
  pnpm --filter @fvoci/web run build > "$out/web-build.log" 2>&1
  check_web_inputs "$web_inputs_before"
  test -s "$root/apps/web/dist/index.html"
  test "$(git rev-parse HEAD)" = "$(cat "$out/sha.txt")"
  cp "$out/sha.txt" "$root/apps/web/dist/.perf-head"
  cp "$out/sha.txt" "$out/web-build-sha.txt"
fi
mkdir -p "$(dirname "$measure_lock")"
while true; do
  wait_for_measurement
  if (set -o noclobber; : > "$measure_lock") 2> "$out/lock-error.log"; then
    owns_measure_lock=true
    printf '%s\n' "$project" > "$measure_lock"
    break
  fi
  if [[ ! -f "$measure_lock" || -L "$measure_lock" ]]; then
    cat "$out/lock-error.log" >&2
    exit 1
  fi
done
build_state() {
  local state
  state=$(ps -eo comm,args | awk -f tools/perf-build-state.awk) || return 2
  jq -ce 'if type=="object" and (.compiler|type)=="boolean" and
    (.nodeBuild|type)=="boolean" and (.heavyCommand|type)=="boolean"
    then . else error("invalid build detector state") end' <<< "$state" || return 2
}
busy_build() {
  local state
  state=$(build_state) || return 2
  jq -e '.compiler or .nodeBuild or .heavyCommand' <<< "$state" > /dev/null
}
wait_for_builds() {
  local status
  while true; do
    if busy_build; then
      sleep 15 || return 2
    else
      status=$?
      if (( status==1 )); then return 0; fi
      printf 'FAIL build detector; refusing to treat an error as idle\n' >&2
      return 2
    fi
  done
}
start_monitor() {
  rm -f "$out/monitor-stop"
  (
    local state status
    monitor_failed() {
      touch "$out/monitor-error"
      printf 'FAIL build monitor detector or recording error\n' >&2
      exit 2
    }
    while true; do
      state=$(build_state) || monitor_failed
      if jq -e '.compiler or .nodeBuild or .heavyCommand' <<< "$state" > /dev/null; then
        touch "$out/build-overlap" || monitor_failed
        if [[ "$monitor_phase" == db ]]; then touch "$out/db-build-overlap" || monitor_failed; fi
      else
        status=$?
        if (( status!=1 )); then monitor_failed; fi
      fi
      jq -c --argjson epochMs "$(date -u +%s%3N)" '.+{epochMs:$epochMs}' <<< "$state" || monitor_failed
      [[ -e "$out/monitor-stop" ]] && break
      sleep 1 || monitor_failed
    done
  ) >> "$out/concurrency.log" &
  monitor_pid=$!
}
stop_monitor() {
  local status=0
  if [[ -n "$monitor_pid" ]]; then
    touch "$out/monitor-stop" || return 1
    if wait "$monitor_pid"; then :; else status=$?; fi
    monitor_pid=
    if (( status!=0 )) || [[ -e "$out/monitor-error" || ! -s "$out/concurrency.log" ]]; then
      touch "$out/monitor-error"
      printf 'FAIL build monitor stopped abnormally or recorded no samples\n' >&2
      return 1
    fi
  fi
}
wait_for_builds
monitor_phase=screen
start_monitor
if [[ "$mode" == rooms ]]; then
  FVOCI_MEASURE_ROOMS="$count" cargo test --release --locked -p fvoci-server --test it room_memory_measure::room_memory_measure -- --ignored --exact --nocapture > "$out/rooms.log" 2>&1
  cat "$out/room-memory.json"
else
  database=$(jq -r .source_db "$out/state.json")
  psql_test() { "${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$database" "$@"; }
  psql_test -c 'CREATE EXTENSION pg_stat_statements; ANALYZE; SELECT pg_stat_statements_reset();' > "$out/analyze.log"
  cargo test --release --locked -p fvoci-server --test screen_measure serve_t6 -- --ignored --exact --nocapture > "$out/server.log" 2>&1 &
  server_pid=$!
  for ((i=0; i<120; i++)); do
    [[ -f "$out/browser.json" ]] && break
    kill -0 "$server_pid"
    sleep 1
  done
  test -f "$out/browser.json"
  export FVOCI_BROWSER_BASE_URL
  FVOCI_BROWSER_BASE_URL=$(jq -r .origin "$out/browser.json")
  export FVOCI_BROWSER_RAW="$out"
  pnpm --filter @fvoci/web exec playwright test perf-baseline.spec.ts --project desktop > "$out/playwright.log" 2>&1
  stop_monitor
  mkdir -p "$out/screen-attempts"
  cp "$out/screens.json" "$out/screen-attempts/1.json"
  cp "$out/screens.json" "$out/current-screens.json"
  printf '[]\n' > "$out/discarded-screens.json"
  screen_attempt=1
  while true; do
    jq -s . "$out/concurrency.log" > "$out/screen-attempts/$screen_attempt-builds.json"
    jq -s -f tools/perf-screen-overlap.jq "$out/current-screens.json" "$out/screen-attempts/$screen_attempt-builds.json" > "$out/screen-attempts/$screen_attempt-discarded.json"
    replay_screens=$(jq -r 'map(.screen)|unique|join(",")' "$out/screen-attempts/$screen_attempt-discarded.json")
    if [[ -z "$replay_screens" ]]; then break; fi
    jq --argjson attempt "$screen_attempt" 'map(.+{attempt:$attempt})' "$out/screen-attempts/$screen_attempt-discarded.json" > "$out/discarded-new.json"
    jq -s 'add' "$out/discarded-screens.json" "$out/discarded-new.json" > "$out/discarded-next.json"
    mv "$out/discarded-next.json" "$out/discarded-screens.json"
    printf 'RERUN contaminated screens (fresh triplets): %s\n' "$replay_screens"
    wait_for_builds
    screen_attempt=$((screen_attempt+1))
    start_monitor
    FVOCI_MEASURE_SCREENS="$replay_screens" pnpm --filter @fvoci/web exec playwright test perf-baseline.spec.ts --project desktop > "$out/playwright-$screen_attempt.log" 2>&1
    stop_monitor
    cp "$out/screens.json" "$out/screen-attempts/$screen_attempt.json"
    jq -s '.[1] as $new | [.[0][] | select(.screen as $name | all($new[]; .screen!=$name))] + $new | sort_by(.screen,.repeat)' "$out/current-screens.json" "$out/screens.json" > "$out/next-screens.json"
    mv "$out/next-screens.json" "$out/current-screens.json"
  done
  cp "$out/current-screens.json" "$out/screens.json"
  jq -n --argjson attempts "$screen_attempt" --slurpfile discarded "$out/discarded-screens.json" '{attempts:$attempts,discarded:$discarded[0]}' > "$out/replay-summary.json"
  monitor_phase=db
  start_monitor
  jq -e -f tools/perf-arguments.jq "$out/screens.json" > "$out/db-arguments.json"
  sql_variables=()
  while IFS= read -r binding; do
    sql_variables+=(-v "$binding")
  done < <(jq -r 'to_entries[] | select(.value | type == "string") | "\(.key)=\(.value)"' "$out/db-arguments.json")
  psql_test -At -c "SELECT json_agg(s) FROM (SELECT queryid::text, calls, total_exec_time, mean_exec_time, rows, shared_blks_hit, shared_blks_read, query FROM pg_stat_statements WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) ORDER BY total_exec_time DESC LIMIT 40) s" > "$out/queries.json"
  actor=$(jq -r .user "$out/browser.json")
  item=$(jq -r .item "$out/browser.json")
  for repeat in 1 2 3; do
    workspace=$(jq -r .workspace "$out/browser.json")
    psql_test -v actor="$actor" -v item="$item" -v workspace="$workspace" "${sql_variables[@]}" < tools/perf-db.sql > "$out/db-$repeat.log"
    psql_test -v actor="$actor" "${sql_variables[@]}" < tools/perf-home.sql > "$out/home-$repeat.log"
  done
  touch "$out/STOP"
  wait "$server_pid"
  server_pid=
  jq -Rn '[inputs | fromjson? | select(.event == "http.request")]' "$out/server.log" > "$out/access.json"
  jq -s -f tools/perf-summary.jq "$out/screens.json" "$out/access.json" > "$out/screen-summary.json"
  jq -f tools/perf-depth.jq "$out/screens.json" > "$out/depth.json"
fi

stop_monitor
# Screen overlaps are preserved and replaced above. Other measurement phases still fail.
if [[ ( "$mode" == rooms && -e "$out/build-overlap" ) || -e "$out/db-build-overlap" ]]; then
  printf 'FAIL concurrent build during measurement: %s\n' "$out" >&2
  exit 1
fi
if [[ "$mode" == screens ]]; then printf 'PASS screens: %s\n' "$out"; fi

# Replay additional pg_stat_statements hotspots after the final room measurement.
# This runs after all RSS/time snapshots, in that run's untouched T6 database.
if [[ "$mode" == rooms && "$count" == 1000 ]]; then
  database=$(jq -r .source_db "$out/state.json")
  room_psql() { "${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$database" "$@"; }
  actor=$(room_psql -At -c 'SELECT id FROM fvoci.users ORDER BY created_at, id LIMIT 1')
  item=$(jq -r '.task_items[0]' "$out/state.json")
  document=$(jq -r '.documents[5]' "$out/state.json")
  for repeat in 1 2 3; do
    room_psql -v actor="$actor" -v item="$item" -v document="$document" < tools/perf-slow.sql > "$out/slow-$repeat.log"
  done
fi
