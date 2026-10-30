#!/usr/bin/env bash
# Inspect sharing measurements in the database held by measure-240.sh hold.
set -euo pipefail
out=${1:?measurement directory}
sql=${2:?measurement SQL file}
test -f "$out/READY"
test ! -f "$out/STOP"
project=$(jq -r .project "$out/runtime.json")
database=$(jq -r .source_db "$out/state.json")
variables=()
while IFS= read -r binding; do variables+=(-v "$binding"); done < <(
  jq -r 'to_entries[] | select(.value | type == "string") | "\(.key)=\(.value)"' "$out/sharing-fixture.json"
)
actor=$(jq -er --arg key "${3:-owner}" '.[$key]' "$out/sharing-fixture.json")
docker compose -p "$project" -f deploy/compose.test.yaml -f "$out/postgres.yaml" exec -T postgres \
  psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$database" "${variables[@]}" -v actor="$actor" < "$sql"
