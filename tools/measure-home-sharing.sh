#!/usr/bin/env bash
# #245 / #210: perf240's unchanged product fixture, N=100/1000 home decomposition.
# Prebuild release server, it, perf240. The caller reserves the measurement
# marker before fvoci-db-test.lock + fvoci-slot.sh and holds it through cleanup.
set -euo pipefail
umask 077
for key in "${!FVOCI_@}" "${!DATABASE_@}"; do
  if [[ -n "$key" ]]; then unset "$key"; fi
done
root=$(git rev-parse --show-toplevel)
cd "$root"
out=$(mktemp -d "$root/target/perf-home-sharing-XXXXXX")
project="fvoci-home-sharing-$$"
compose=(docker compose -p "$project" -f deploy/compose.test.yaml -f "$out/postgres.yaml")
worker=
monitor=
cleanup() {
  local status=$? down_status=0
  trap - EXIT
  if [[ -n "$worker" ]]; then kill "$worker" 2>/dev/null || true; wait "$worker" || true; fi
  if [[ -n "$monitor" ]]; then kill "$monitor" 2>/dev/null || true; wait "$monitor" || true; fi
  "${compose[@]}" down --volumes > "$out/cleanup.log" 2>&1 || down_status=$?
  if (( status == 0 )); then status=$down_status; fi
  exit "$status"
}
trap cleanup EXIT
cat > "$out/postgres.yaml" <<'YAML'
services:
  postgres:
    command: ['postgres', '-c', 'max_connections=200', '-c', 'shared_preload_libraries=pg_stat_statements', '-c', 'pg_stat_statements.track=all', '-c', 'track_functions=all']
YAML
printf '%s\n' "$out"
git rev-parse HEAD > "$out/sha.txt"
sha256sum crates/db/migrations/*.sql > "$out/sql-sha256.txt"
"${compose[@]}" up -d --wait postgres meilisearch > "$out/compose.log" 2>&1
port=$("${compose[@]}" port postgres 5432)
search_port=$("${compose[@]}" port meilisearch 7700)
export DATABASE_URL="postgres://fvoci_app:fvoci-test-app-password@$port/fvoci"
export FVOCI_TEST_ADMIN_URL="postgres://fvoci:fvoci-test-password@$port/fvoci"
export FVOCI_TEST_MIGRATION_URL="postgres://fvoci_migrator:fvoci-test-migrator-password@$port/fvoci"
export DATABASE_ADMIN_URL="$FVOCI_TEST_ADMIN_URL"
export DATABASE_MIGRATION_URL="$FVOCI_TEST_MIGRATION_URL"
export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password
export FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
export FVOCI_TEST_SEARCH_URL="http://$search_port"
export FVOCI_TEST_SEARCH_KEY=fvoci-test-meili-master-key-16chars
export FVOCI_MEASURE_DIR="$out"
target/release/fvoci-server db bootstrap > "$out/bootstrap.log" 2>&1
target/release/fvoci-server db migrate > "$out/migrate.log" 2>&1
cargo test --release --locked -p fvoci-server --test it restore_measure::restore_measure_generate -- --ignored --exact --nocapture > "$out/generate.log" 2>&1
busy_build() {
  pgrep -x 'rustc|cc1|cc1plus|ld|rust-lld' >/dev/null ||
    ps -eo comm,ppid,args | awk '$1 ~ /^(node|MainThread)$/ && /vite|vitest|storybook|svelte-check|eslint|tsc/ {
      if ($2 == 1 && /\/vitest\/dist\/workers\/forks\.js$/) next;
      found=1
    } END { exit !found }'
}
while busy_build; do sleep 15; done
# A disconnected Vitest worker can remain asleep after its parent exits. Keep
# its CPU counters in the overlap guard instead of waiting forever on its name.
declare -A orphan_cpu=()
while IFS= read -r pid; do
  if [[ -r "/proc/$pid/stat" ]]; then
    orphan_cpu[$pid]=$(awk '{print $14+$15}' "/proc/$pid/stat")
    printf '%s %s\n' "$pid" "${orphan_cpu[$pid]}" >> "$out/idle-orphans.log"
  fi
done < <(ps -eo pid,ppid,comm,args | awk '$2 == 1 && $3 ~ /^(node|MainThread)$/ && /\/vitest\/dist\/workers\/forks\.js$/ {print $1}')
(
  while true; do
    if busy_build; then touch "$out/build-overlap"; busy=true; else busy=false; fi
    for pid in "${!orphan_cpu[@]}"; do
      if [[ -r "/proc/$pid/stat" ]]; then
        current_cpu=$(awk '{print $14+$15}' "/proc/$pid/stat")
        if [[ "$current_cpu" != "${orphan_cpu[$pid]}" ]]; then touch "$out/build-overlap"; busy=true; fi
      fi
    done
    printf '%s compiler_or_node_build=%s\n' "$(date -u +%FT%TZ)" "$busy"
    sleep 1
  done
) > "$out/concurrency.log" &
monitor=$!
database=$(jq -r .source_db "$out/state.json")
psql_test() { "${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$database" "$@"; }
FVOCI_PERF240_MAX_SHARES=1000 FVOCI_PERF240_HOME_PROBE=1 cargo test --release --locked -p fvoci-server --test perf240 shares -- --ignored --exact --nocapture > "$out/shares.log" 2>&1 &
worker=$!
for n in 100 1000; do
  for ((attempt=0; attempt<3600; attempt++)); do
    [[ -f "$out/home-$n-ready.json" ]] && break
    kill -0 "$worker"
    sleep 1
  done
  test -s "$out/home-$n-ready.json"
  actor=$(jq -r .actor "$out/home-$n-ready.json")
  org=$(jq -r .organization "$out/home-$n-ready.json")
  today=$(psql_test -At -c "SELECT (now() AT TIME ZONE 'Asia/Seoul')::date")
  psql_test -v actor="$actor" -v home_org="$org" -v home_tz=Asia/Seoul -v today="$today" < tools/perf-home.sql > "$out/home-$n.log"
  psql_test -At -c "SELECT json_agg(s) FROM (SELECT funcname,calls,total_time,self_time FROM pg_stat_user_functions WHERE schemaname='fvoci' ORDER BY total_time DESC) s" > "$out/functions-$n.json"
  if (( n == 1000 )); then
    workspace=$(jq -r .workspace "$out/state.json")
    owner=$(psql_test -At -c "SELECT created_by FROM fvoci.workspaces WHERE id='$workspace'::uuid")
    psql_test -v actor="$owner" -v home_org="$org" < tools/perf-home-index.sql > "$out/index.log"
  fi
  touch "$out/home-$n-continue"
done
wait "$worker"
worker=
kill "$monitor"
wait "$monitor" || true
monitor=
test ! -f "$out/build-overlap"
sha256sum -c "$out/sql-sha256.txt" > "$out/sql-unchanged.log"
printf 'PASS home sharing decomposition: %s\n' "$out"
