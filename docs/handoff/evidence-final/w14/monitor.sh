#!/usr/bin/env bash
# 시험 중 셀프호스트 Supabase db의 연결 수(pg_stat_activity)와 Supavisor 로그를 받는다.
# 사용: monitor.sh <worktree 이름(compose 프로젝트 접두)> <출력 접두>
# 컨테이너가 뜰 때까지 기다리고, 컨테이너가 사라지면 끝난다.
set -u
base="$1"; out="$2"
find_ctr() {
  docker ps --filter "label=com.docker.compose.service=$1" --format '{{.Names}} {{.Label "com.docker.compose.project"}}' \
    | awk -v b="fvoci-$base-" '$2 ~ "^"b && $2 ~ /-supabase$/ {print $1; exit}'
}
db=""; sv=""
until [ -n "$db" ] && [ -n "$sv" ]; do db=$(find_ctr db); sv=$(find_ctr supavisor); sleep 1; done
echo "db=$db supavisor=$sv" > "$out-containers.txt"
docker logs -f --timestamps "$sv" > "$out-supavisor.log" 2>&1 &
docker logs -f --timestamps "$db" > "$out-db.log" 2>&1 &
# DB가 받을 때까지 기다린 뒤 0.25초마다 역할·출처별 연결 수를 적는다.
until docker exec "$db" pg_isready -U postgres >/dev/null 2>&1; do sleep 1; done
docker exec "$db" psql -U supabase_admin -d postgres -At -c 'show max_connections' > "$out-max-connections.txt" 2>&1
docker exec "$db" psql -U supabase_admin -d postgres -At -c 'show superuser_reserved_connections' >> "$out-max-connections.txt" 2>&1
printf "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','HH24:MI:SS.MS') AS t, count(*) AS total, count(*) FILTER (WHERE usename='fvoci_app') AS app, count(*) FILTER (WHERE usename='fvoci_app' AND client_addr::text <> '%s') AS app_direct, count(*) FILTER (WHERE usename LIKE 'fvoci_migrator%%' OR usename='fvoci_owner') AS owner, count(*) FILTER (WHERE usename='postgres') AS pg_admin, count(*) FILTER (WHERE usename NOT IN ('fvoci_app','postgres') AND usename NOT LIKE 'fvoci_migrator%%') AS other, count(*) FILTER (WHERE usename='fvoci_app' AND state='active') AS app_active, count(*) FILTER (WHERE usename='fvoci_app' AND state LIKE 'idle in transaction%%') AS app_idle_tx, count(*) FILTER (WHERE usename='fvoci_app' AND wait_event_type='Lock') AS app_lock_wait FROM pg_stat_activity WHERE backend_type='client backend' \\\\watch 0.25\n" \
  "$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}' "$sv" | awk '{print $1}')" \
  | docker exec -i "$db" psql -U supabase_admin -d postgres -A -F ' ' -P footer=off -P pager=off > "$out-pg-activity.txt" 2>&1
wait
