#!/usr/bin/env bash
# usage: run-with-heartbeat.sh <log> <cmd>...  — runs cmd, logs output, sends an Orca heartbeat every 5 minutes
log=$1; shift
d=$(dirname "$0")
( while sleep 300; do "$d/hb.sh" testing; done ) & hb=$!
start=$(date -u +%FT%TZ); echo "# $(git rev-parse HEAD) $* start $start" > "$log"
"$@" >> "$log" 2>&1; rc=$?
echo "# rc=$rc end $(date -u +%FT%TZ)" >> "$log"; kill $hb 2>/dev/null; echo "rc=$rc"; exit $rc
