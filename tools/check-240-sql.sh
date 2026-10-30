#!/usr/bin/env bash
# #240: fresh-install/checksum and timezone decision classification only.
# Run under the host DB lock and fvoci-slot.sh after building this release server.
set -euo pipefail
umask 077
for key in "${!FVOCI_@}" "${!DATABASE_@}"; do
  if [[ -n "$key" ]]; then unset "$key"; fi
done
root=$(git rev-parse --show-toplevel)
cd "$root"
out=$(mktemp -d "$root/target/perf240-sql-XXXXXX")
project="fvoci-perf240-sql-$$"
compose=(docker compose -p "$project" -f deploy/compose.test.yaml)
trap '"${compose[@]}" down --volumes > "$out/cleanup.log" 2>&1' EXIT
printf '%s\n' "$out"
git rev-parse HEAD > "$out/base-head.txt"
sha256sum crates/db/migrations/*.sql tools/check-240-sql.sh tools/perf240-timezone-*.sql > "$out/source.sha256"
"${compose[@]}" up -d --wait postgres > "$out/compose.log" 2>&1
port=$("${compose[@]}" port postgres 5432)
export DATABASE_URL="postgres://fvoci_app:fvoci-test-app-password@$port/fvoci"
export DATABASE_ADMIN_URL="postgres://fvoci:fvoci-test-password@$port/fvoci"
export DATABASE_MIGRATION_URL="postgres://fvoci_migrator:fvoci-test-migrator-password@$port/fvoci"
export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password
export FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
target/release/fvoci-server db bootstrap > "$out/bootstrap.log" 2>&1
target/release/fvoci-server db migrate > "$out/migrate.log" 2>&1
jq -n --arg project "$project" '{project:$project}' > "$out/runtime.json"
printf '{"source_db":"fvoci"}\n' > "$out/state.json"
touch "$out/READY"
bash tools/perf240-test.sh "$out" migration_baseline::concurrent_new_installs_repeat_and_preserve_checksums_without_app_ledger_access > "$out/checksum.log" 2>&1
matrix_rc=0
"${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d fvoci < tools/perf240-timezone-equivalence.sql > "$out/timezone-product.log" 2>&1 || matrix_rc=$?
printf '%s\n' "$matrix_rc" > "$out/timezone-equality-exit.txt"
test "$matrix_rc" = 3
jq -Ren '[inputs | fromjson? | select(.event == "timezone.matrix")] | length == 1 and .[0].canonical_lost == 0 and .[0].differences > 0' "$out/timezone-product.log" > "$out/decision-classification.json"
printf 'PASS checksum and decision classification (timezone equality FAIL): %s\n' "$out"
