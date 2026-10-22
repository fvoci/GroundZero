#!/usr/bin/env bash
# 사용: run.sh <이름> <cargo xtask test 인자>... — 시험을 돌리며 연결 수·Supavisor 로그를 받고, 5분마다 heartbeat를 보낸다.
# 로그는 logs/<이름>.log, 측정은 logs/<이름>-*.
name=$1; shift
dir=$(cd "$(dirname "$0")" && pwd); log="$dir/logs/$name.log"
( while sleep 300; do orca orchestration send --from term_84f804d9-bc5f-4fb5-ae35-d2ae1e74af0c --type heartbeat --subject alive --task-id task_0509ffec6947 --dispatch-id ctx_86fb710ebf75 --phase testing >/dev/null 2>&1; done ) & hb=$!
"$dir/monitor.sh" "$(basename "$PWD" | tr 'A-Z' 'a-z')" "$dir/logs/$name" & mon=$!
echo "# $(git rev-parse --short=12 HEAD) cargo xtask test $* start $(date -u +%FT%TZ)" > "$log"
cargo xtask test "$@" >> "$log" 2>&1; rc=$?
echo "# rc=$rc end $(date -u +%FT%TZ)" >> "$log"
kill $hb 2>/dev/null; sleep 3; kill $mon 2>/dev/null; pkill -P $mon 2>/dev/null
sed -i "s#$HOME#<home>#g; s#$PWD#<worktree>#g" "$log"
echo "rc=$rc"; exit $rc
