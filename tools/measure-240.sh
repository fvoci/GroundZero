#!/usr/bin/env bash
# #240 / #245 sharing measurements. Run under DB lock and fvoci-slot.sh.
# The original T6 generator is used without changing its shape or implementation.
set -euo pipefail
umask 077
for key in "${!FVOCI_@}" "${!DATABASE_@}"; do
  if [[ -n "$key" ]]; then unset "$key"; fi
done
root=$(git rev-parse --show-toplevel)
cd "$root"
out=$(mktemp -d "$root/target/perf240-XXXXXX")
project="fvoci-perf240-$$"
measure_lock="$HOME/.cache/fvoci-measure.lock"
owns_measure_lock=false
cat > "$out/postgres.yaml" <<'YAML'
services:
  postgres:
    command: ['postgres', '-c', 'max_connections=200', '-c', 'shared_preload_libraries=pg_stat_statements', '-c', 'pg_stat_statements.track=all', '-c', 'track_functions=all']
YAML
compose=(docker compose -p "$project" -f deploy/compose.test.yaml -f "$out/postgres.yaml")
monitor_pid=
cleanup() {
  if [[ -n "$monitor_pid" ]]; then kill "$monitor_pid" 2>/dev/null || true; wait "$monitor_pid" || true; fi
  "${compose[@]}" down --volumes > "$out/cleanup.log" 2>&1
  if "$owns_measure_lock"; then rm "$measure_lock"; fi
}
trap cleanup EXIT
printf '%s\n' "$out"
git rev-parse HEAD > "$out/sha.txt"
sha256sum tools/measure-240.sh tools/perf240* crates/server/tests/perf240.rs > "$out/tool-sha256.txt"
sha256sum crates/db/migrations/*.sql > "$out/sql-sha256.txt"
# Build the embedded migrations and both test binaries before sampling. This
# command is sufficient on its own; it never reuses an unverified server binary.
cargo build --release --locked -p fvoci-server --bin fvoci-server > "$out/build-server.log" 2>&1
cargo test --release --locked -p fvoci-server --test it --no-run > "$out/build-it.log" 2>&1
cargo test --release --locked -p fvoci-server --test perf240 --no-run > "$out/build-shares.log" 2>&1
until (set -o noclobber; printf '%s\n' "$project" > "$measure_lock") 2>/dev/null; do sleep 2; done
owns_measure_lock=true
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
  pgrep -x 'rustc|cc1|cc1plus|ld|rust-lld' > /dev/null ||
    ps -eo comm,args | awk '$1 ~ /^(node|MainThread)$/ && /vite|vitest|storybook|svelte-check|eslint|tsc/ { found=1 } END { exit !found }'
}
while busy_build; do sleep 2; done
(
  while true; do
    if busy_build; then busy=true; else busy=false; fi
    printf '%s compiler_or_node_build=%s\n' "$(date -u +%FT%TZ)" "$busy"
    sleep 1
  done
) > "$out/concurrency.log" &
monitor_pid=$!
cargo test --release --locked -p fvoci-server --test perf240 shares -- --ignored --exact --nocapture > "$out/shares.log" 2>&1
jq -Rn '[inputs | fromjson? | select(.event == "http.request")]' "$out/shares.log" > "$out/access.json"
jq -Ren '[inputs | fromjson? | select(.event == "search.unavailable" or .event == "search.paging_failed")] | length == 0' "$out/shares.log" > "$out/search-health.json"
jq -s -f tools/perf240-summary.jq "$out/sharing.json" "$out/access.json" > "$out/sharing-summary.json"
sha256sum -c "$out/sql-sha256.txt" > "$out/sql-unchanged.log"
sha256sum -c "$out/tool-sha256.txt" > "$out/tools-unchanged.log"
printf 'PASS #240 sharing measurements: %s\n' "$out"
if [[ ${1:-} == hold ]]; then
  jq -n --arg project "$project" --arg out "$out" '{project:$project,out:$out}' > "$out/runtime.json"
  touch "$out/READY"
  while [[ ! -f "$out/STOP" ]]; do sleep 1; done
fi
