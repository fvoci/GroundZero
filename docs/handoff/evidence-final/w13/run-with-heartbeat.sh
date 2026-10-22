#!/usr/bin/env bash
# usage: run-with-heartbeat.sh <log> <cmd>...  — runs cmd, logs output, sends an Orca heartbeat every 5 minutes
log=$1; shift
( while sleep 300; do orca orchestration send --from term_7a6c6291-d764-4090-a757-eed02995b555 --type heartbeat --subject alive --task-id task_9820d26251fc --dispatch-id ctx_31c269c9615a --phase testing >/dev/null 2>&1; done ) & hb=$!
start=$(date -u +%FT%TZ); echo "# $(git rev-parse --short=12 HEAD) $* start $start" > "$log"
"$@" >> "$log" 2>&1; rc=$?
echo "# rc=$rc end $(date -u +%FT%TZ)" >> "$log"; kill $hb 2>/dev/null; echo "rc=$rc"; exit $rc
