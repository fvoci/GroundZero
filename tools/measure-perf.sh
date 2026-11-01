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
mkdir -p "$root/target"
out=$(mktemp -d "$root/target/perf-${mode}-${count}-XXXXXX")
project="fvoci-perf-w1-${mode}-${count}-$$"
measure_lock="$HOME/.cache/fvoci-measure.lock"
# Initialize ownership before acquisition: EXIT can run as soon as the write ends.
measure_token="$$-$(date -u +%Y%m%dT%H%M%S)-$RANDOM-$RANDOM"
wait_for_measurement() {
  while [[ -e "$measure_lock" || -L "$measure_lock" ]]; do
    if [[ ! -f "$measure_lock" || -L "$measure_lock" ]]; then
      printf 'invalid measurement lock path\n' >&2
      return 1
    fi
    sleep 15
  done
}
acquire_measure_lock() {
  mkdir -p "$(dirname "$measure_lock")"
  while true; do
    wait_for_measurement || return
    if (set -C; printf '%s\n' "$measure_token" > "$measure_lock") 2> "$out/lock-error.log"; then
      return 0
    fi
    if [[ ! -f "$measure_lock" || -L "$measure_lock" ]]; then
      cat "$out/lock-error.log" >&2
      return 1
    fi
  done
}
busy_build() {
  ps -eo comm,args | awk '
    $1 ~ /^(cargo|rustc|cc1|cc1plus|ld|rust-lld)$/ { found=1 }
    $1 ~ /^(node|MainThread)$/ && /vite|vitest|storybook|svelte-check|eslint|tsc/ { found=1 }
    $1 ~ /^(docker|docker-buildx|buildctl)$/ && /build/ { found=1 }
    END { exit !found }'
}
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
cleanup() {
  local status=$? cleanup_status=0
  if [[ -n "$server_pid" ]]; then
    touch "$out/STOP"
    wait "$server_pid" || true
  fi
  if [[ -n "$monitor_pid" ]]; then kill "$monitor_pid" 2>/dev/null || true; wait "$monitor_pid" || true; fi
  "${compose[@]}" down --volumes > "$out/cleanup.log" 2>&1 || cleanup_status=$?
  if [[ -f "$measure_lock" && ! -L "$measure_lock" ]] && [[ "$(cat "$measure_lock")" == "$measure_token" ]]; then
    rm -f "$measure_lock" || cleanup_status=$?
  fi
  if (( status != 0 )); then return "$status"; fi
  return "$cleanup_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf '%s\n' "$out"
git rev-parse HEAD > "$out/sha.txt"
sha256sum crates/db/migrations/*.sql > "$out/sql-sha256.txt"
# Complete every build before opening the measurement window (#291).
wait_for_measurement
cargo build --release --locked -p fvoci-server > "$out/build-server.log" 2>&1
wait_for_measurement
cargo test --release --locked -p fvoci-server --test it --test screen_measure --no-run > "$out/build-tests.log" 2>&1
if [[ "$mode" == screens ]]; then
  wait_for_measurement
  web_inputs_before=$(check_web_inputs)
  pnpm --filter @fvoci/web run build > "$out/web-build.log" 2>&1
  check_web_inputs "$web_inputs_before"
  test -s "$root/apps/web/dist/index.html"
  test "$(git rev-parse HEAD)" = "$(cat "$out/sha.txt")"
  cp "$out/sha.txt" "$root/apps/web/dist/.perf-head"
  cp "$out/sha.txt" "$out/web-build-sha.txt"
fi
acquire_measure_lock
while busy_build; do sleep 15; done
date -u +%FT%TZ > "$out/window-start.txt"
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
(
  while true; do
    # The runner's no-op Cargo invocations execute already-built test binaries.
    # Observe actual compilers/builders here, avoiding that runner's Cargo parent.
    if ps -eo comm,args | awk '
      $1 ~ /^(rustc|cc1|cc1plus|ld|rust-lld)$/ { found=1 }
      $1 ~ /^(node|MainThread)$/ && /vite|vitest|storybook|svelte-check|eslint|tsc/ { found=1 }
      $1 ~ /^(docker|docker-buildx|buildctl)$/ && /build/ { found=1 }
      END { exit !found }'; then busy=true; else busy=false; fi
    printf '%s compiler_or_node_or_docker_build=%s\n' "$(date -u +%FT%TZ)" "$busy"
    sleep 1
  done
) > "$out/concurrency.log" &
monitor_pid=$!
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
  jq -e -f tools/perf-arguments.jq "$out/screens.json" > "$out/db-arguments.json"
  sql_variables=()
  while IFS= read -r binding; do
    sql_variables+=(-v "$binding")
  done < <(jq -r 'to_entries[] | select(.value | type == "string") | "\(.key)=\(.value)"' "$out/db-arguments.json")
  psql_test -At -c "SELECT json_agg(s) FROM (SELECT queryid::text, calls, total_exec_time, mean_exec_time, rows, shared_blks_hit, shared_blks_read, query FROM pg_stat_statements WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) ORDER BY total_exec_time DESC LIMIT 40) s" > "$out/queries.json"
  actor=$(jq -r .user "$out/browser.json")
  item=$(jq -r .item "$out/browser.json")
  task=$(jq -r .task "$out/browser.json")
  for repeat in 1 2 3; do
    workspace=$(jq -r .workspace "$out/browser.json")
    psql_test -v actor="$actor" -v item="$item" -v task="$task" -v workspace="$workspace" "${sql_variables[@]}" < tools/perf-db.sql > "$out/db-$repeat.log"
    psql_test -v actor="$actor" "${sql_variables[@]}" < tools/perf-home.sql > "$out/home-$repeat.log"
  done
  touch "$out/STOP"
  wait "$server_pid"
  server_pid=
  jq -Rn '[inputs | fromjson? | select(.event == "http.request")]' "$out/server.log" > "$out/access.json"
  jq -s -f tools/perf-summary.jq "$out/screens.json" "$out/access.json" > "$out/screen-summary.json"
  jq -f tools/perf-depth.jq "$out/screens.json" > "$out/depth.json"
  printf 'PASS screens: %s\n' "$out"
fi
sha256sum -c "$out/sql-sha256.txt" > "$out/sql-unchanged.log"

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
