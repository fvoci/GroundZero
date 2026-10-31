#!/usr/bin/env bash
# Run after every fixture connection has closed. Each arm/repeat gets a new
# physical copy of the same untouched T6 database; UPDATE rollback is not a reset.
set -euo pipefail
umask 077
actor=${1:?actor}
source_db=${2:?source database}
out=${3:?output directory}
shift 3
compose=("$@")
probe=
seed=
control() { "${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d postgres "$@"; }
drop_probe() {
  control -v probe="$probe" <<'SQL'
DROP DATABASE :"probe" WITH (FORCE);
SQL
}
cleanup() {
  local status=$? drop_status=0
  trap - EXIT
  if [[ -n "$probe" ]]; then drop_probe >> "$out/index-write-cleanup.log" 2>&1 || drop_status=$?; fi
  if [[ -n "$seed" ]]; then
    control -v probe="$seed" >> "$out/index-write-cleanup.log" 2>&1 <<'SQL' || drop_status=$?
DROP DATABASE :"probe" WITH (FORCE);
SQL
  fi
  if (( status == 0 )); then status=$drop_status; fi
  exit "$status"
}
trap cleanup EXIT
# Freeze one physical template. Autovacuum and clients cannot change it between
# arms; only the per-invocation clones accept connections.
seed="fvoci_home_seed_$$"
control -v seed="$seed" -v source="$source_db" > "$out/index-write-seed.log" <<'SQL'
CREATE DATABASE :"seed" WITH TEMPLATE :"source" ALLOW_CONNECTIONS false;
SQL
for repeat in 1 2 3; do
  for arm in before after; do
    probe="fvoci_home_write_$$_${arm}_$repeat"
    control -v probe="$probe" -v source="$seed" > "$out/index-write-$arm-$repeat-setup.log" <<'SQL'
CREATE DATABASE :"probe" WITH TEMPLATE :"source" ALLOW_CONNECTIONS true;
SQL
    indexed=false
    if [[ "$arm" == after ]]; then indexed=true; fi
    "${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 -U fvoci -d "$probe" \
      -v actor="$actor" -v indexed="$indexed" < tools/perf-home-index-write.sql > "$out/index-write-$arm-$repeat.log"
    drop_probe >> "$out/index-write-cleanup.log" 2>&1
    probe=
  done
done
