#!/usr/bin/env bash
# Replay an experiment in the database held by measure-240.sh hold.
set -euo pipefail
out=${1:?measurement directory}
sql=${2:?experiment SQL file}
test -f "$out/READY"
test ! -f "$out/STOP"
project=$(jq -r .project "$out/runtime.json")
database=${4:-$(jq -r .source_db "$out/state.json")}
variables=()
while IFS= read -r binding; do variables+=(-v "$binding"); done < <(
  jq -r 'to_entries[] | select(.value | type == "string") | "\(.key)=\(.value)"' "$out/sharing-fixture.json"
)
# Supply fresh RFC 9562 UUIDv7 values to product SQL fixtures (PostgreSQL 17 has
# no built-in uuidv7 generator). Node and its crypto module are existing tools.
while IFS= read -r binding; do variables+=(-v "$binding"); done < <(
  node --input-type=module <<'JS' | jq -r 'to_entries[] | "\(.key)=\(.value)"'
import { randomBytes } from 'node:crypto';
const ids = {};
for (let n = 1; n <= 8; n++) {
  const bytes = randomBytes(16);
  bytes.writeUIntBE(Date.now(), 0, 6);
  bytes[6] = (bytes[6] & 15) | 112;
  bytes[8] = (bytes[8] & 63) | 128;
  const h = bytes.toString('hex');
  ids['newid_' + n] = [h.slice(0, 8), h.slice(8, 12), h.slice(12, 16), h.slice(16, 20), h.slice(20)].join('-');
}
process.stdout.write(JSON.stringify(ids));
JS
)
item=$(jq -r '.task_items[0]' "$out/state.json")
document=$(jq -r '.documents[5]' "$out/state.json")
workspace=$(jq -r .workspace "$out/state.json")
actor=$(jq -er --arg key "${3:-owner}" '.[$key]' "$out/sharing-fixture.json")
if [[ $# -ge 5 ]]; then variables+=(-v "candidate=$5"); fi
docker compose -p "$project" -f deploy/compose.test.yaml -f "$out/postgres.yaml" exec -T postgres \
  psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$database" "${variables[@]}" -v actor="$actor" -v item="$item" -v document="$document" -v workspace="$workspace" < "$sql"
