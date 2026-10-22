#!/usr/bin/env bash
# W14 monitor.sh의 방식(0.25초마다 pg_stat_activity를 역할별로 셈)을 기본 시험 스택(compose.test.yaml의 postgres)에 맞춘 것.
# 사용: monitor-test-pg.sh <worktree 이름(compose 프로젝트 접두)> <출력 접두>
# 컨테이너가 뜰 때까지 기다리고, 컨테이너가 사라지면(psql \watch가 끊기면) 끝난다. 측정 연결 1개(fvoci, superuser)는 `other`에 들어간다.
set -u
base="$1"; out="$2"
find_ctr() {
  docker ps --filter "label=com.docker.compose.service=$1" --format '{{.Names}} {{.Label "com.docker.compose.project"}}' \
    | awk -v b="fvoci-$base-" '$2 ~ "^"b && $2 ~ /-test$/ {print $1; exit}'
}
db=""
until [ -n "$db" ]; do db=$(find_ctr postgres); sleep 1; done
echo "postgres=$db" > "$out-containers.txt"
docker logs -f --timestamps "$db" > "$out-db.log" 2>&1 &
until docker exec "$db" pg_isready -U fvoci -d fvoci >/dev/null 2>&1; do sleep 1; done
docker exec "$db" psql -U fvoci -d fvoci -At -c 'show max_connections' > "$out-max-connections.txt" 2>&1
docker exec "$db" psql -U fvoci -d fvoci -At -c 'show superuser_reserved_connections' >> "$out-max-connections.txt" 2>&1
# 열 순서는 peaks.sh와 같다(app_direct 자리는 0: 풀러 없음).
printf "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','HH24:MI:SS.MS') AS t, count(*) AS total, count(*) FILTER (WHERE usename='fvoci_app') AS app, 0 AS app_direct, count(*) FILTER (WHERE usename LIKE 'fvoci_migrator%%' OR usename='fvoci_owner') AS owner, 0 AS pg_admin, count(*) FILTER (WHERE usename NOT IN ('fvoci_app','fvoci_owner') AND usename NOT LIKE 'fvoci_migrator%%') AS other, count(*) FILTER (WHERE usename='fvoci_app' AND state='active') AS app_active, count(*) FILTER (WHERE usename='fvoci_app' AND state LIKE 'idle in transaction%%') AS app_idle_tx, count(*) FILTER (WHERE usename='fvoci_app' AND wait_event_type='Lock') AS app_lock_wait FROM pg_stat_activity WHERE backend_type='client backend' \\\\watch 0.25\n" \
  | docker exec -i "$db" psql -U fvoci -d fvoci -A -F ' ' -P footer=off -P pager=off > "$out-pg-activity.txt" 2>&1
wait
