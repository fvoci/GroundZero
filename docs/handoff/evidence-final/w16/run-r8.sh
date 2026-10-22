#!/usr/bin/env bash
# 사용: run-r8.sh <이름> <pg|supabase|none> <명령>... — 명령을 돌리며(측정이 있으면 함께) 5분마다 heartbeat를 보낸다.
# 로그 logs/<이름>.log(첫 줄 SHA·명령·시작 UTC, 끝 줄 rc·끝 UTC), 측정 logs/<이름>-*.
name=$1; mon_kind=$2; shift 2
dir=$(cd "$(dirname "$0")" && pwd); log="$dir/logs/$name.log"
( while sleep 300; do "$dir/hb.sh" testing; done ) & hb=$!
mon=""
case $mon_kind in
  pg) "$dir/monitor-test-pg.sh" "$(basename "$PWD" | tr 'A-Z' 'a-z')" "$dir/logs/$name" & mon=$! ;;
  supabase) "$dir/monitor-supabase.sh" "$(basename "$PWD" | tr 'A-Z' 'a-z')" "$dir/logs/$name" & mon=$! ;;
esac
echo "# $(git rev-parse HEAD) $* start $(date -u +%FT%TZ)" > "$log"
"$@" >> "$log" 2>&1; rc=$?
echo "# rc=$rc end $(date -u +%FT%TZ)" >> "$log"
kill $hb 2>/dev/null
if [ -n "$mon" ]; then sleep 3; pkill -P $mon 2>/dev/null; kill $mon 2>/dev/null; fi
sed -i "s#$HOME#<home>#g; s#$PWD#<worktree>#g" "$log"
echo "rc=$rc"; exit $rc
