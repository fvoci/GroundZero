#!/usr/bin/env bash
# Run a named integration test against a prepared isolated measurement stack.
# Invoke under fvoci-slot.sh with the host DB lock held by its lifecycle runner.
set -euo pipefail
for key in "${!FVOCI_@}" "${!DATABASE_@}"; do
  if [[ -n "$key" ]]; then unset "$key"; fi
done
out=$(realpath "${1:?measurement directory}")
name=${2:?exact integration test name}
test -f "$out/READY"
test ! -f "$out/STOP"
project=$(jq -r .project "$out/runtime.json")
database=$(jq -r .source_db "$out/state.json")
port=$(docker compose -p "$project" -f deploy/compose.test.yaml -f "$out/postgres.yaml" port postgres 5432)
export DATABASE_URL="postgres://fvoci_app:fvoci-test-app-password@$port/$database"
export FVOCI_TEST_DIRECT_APP_URL="$DATABASE_URL"
export FVOCI_TEST_ADMIN_URL="postgres://fvoci:fvoci-test-password@$port/$database"
export FVOCI_TEST_MIGRATION_URL="postgres://fvoci_migrator:fvoci-test-migrator-password@$port/$database"
export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password
export FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
export FVOCI_STORAGE_DIR="$out/source-files"
cargo test --locked -p fvoci-server --test it "$name" -- --exact --nocapture
