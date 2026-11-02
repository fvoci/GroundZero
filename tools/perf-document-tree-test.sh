#!/usr/bin/env bash
# #291 D1: independent witnesses for both parallel arms and one consumption.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
mkdir -p "$root/target"
out=$(mktemp -d "$root/target/perf-document-tree-test-XXXXXX")
fixture=tools/fixtures/perf-document-tree-parallel.json
jq -f tools/perf-depth.jq "$fixture" > "$out/valid.json"
jq -e 'length==1 and .[0].depth==4 and .[0].chain[-1].path=="/api/items/item/backlinks"' "$out/valid.json" > /dev/null
jq '.[0].requests |= map(if .path=="/api/modules/module" then .end=80 else . end)' "$fixture" > "$out/module-last.json"
jq -f tools/perf-depth.jq "$out/module-last.json" > "$out/module-last-depth.json"
jq -e '.[0].depth==4 and .[0].chain[-1].path=="/api/modules/module"' "$out/module-last-depth.json" > /dev/null
jq '.[0].requests |= map(if .path=="/api/items/item/documents" then .end=90 else . end)' "$fixture" > "$out/documents-last.json"
jq -f tools/perf-depth.jq "$out/documents-last.json" > "$out/documents-last-depth.json"
jq -e '.[0].depth==4' "$out/documents-last-depth.json" > /dev/null
cases=0
reject() {
  if jq -f tools/perf-depth.jq "$out/input.json" > /dev/null 2> "$out/error.log"; then
    printf 'FAIL document tree witness accepted: %s\n' "$*" >&2
    exit 1
  fi
  grep -F 'dependency witness' "$out/error.log" > /dev/null
  cases=$((cases+1))
}
for path in /api/items/item /api/items/item/documents /api/modules/module /api/items/item/backlinks; do
  for defect in missing early unfinished duplicate; do
    jq --arg path "$path" --arg defect "$defect" '
      .[0].requests |= (if $defect=="missing" then map(select(.path!=$path))
        elif $defect=="duplicate" then .+[.[] | select(.path==$path)]
        else map(if .path==$path then
          if $defect=="early" then .start=0 else .end=null end
          else . end) end)
    ' "$fixture" > "$out/input.json"
    reject "$path" "$defect"
  done
done
for path in /api/modules/module /api/items/item/backlinks; do
  jq --arg path "$path" '.[0].requests |= map(if .path==$path then .start=100 | .end=110 else . end)' "$fixture" > "$out/input.json"
  reject "$path" serialized
done
for suffix in documents backlinks; do
  jq --arg suffix "$suffix" '.[0].requests |= map(if .path==("/api/items/item/"+$suffix) then .path=("/api/items/other/"+$suffix) else . end)' "$fixture" > "$out/input.json"
  reject "$suffix" wrong-item
done
jq '.[0].requests |= map(if .path=="/api/items/item/documents" then .start=40 else . end)' "$fixture" > "$out/input.json"
reject documents serialized
printf 'PASS document tree dependency witnesses: %s rejected mutations\n' "$cases"
