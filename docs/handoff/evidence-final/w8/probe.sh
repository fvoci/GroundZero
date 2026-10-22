#!/usr/bin/env bash
# usage: probe.sh <label> [docker run args...] — start a 512m meili, index small docs, print idle and peak memory
label=$1; shift
img=getmeili/meilisearch:v1.37.0@sha256:9cde2774864e6c19cdd09fcd2950bc01ac9a0454b6f343fffafa14faae6eb7ea
name=w8-meili-probe-$label
docker rm -f "$name" >/dev/null 2>&1
docker run -d --name "$name" -m 512m --tmpfs /meili_data -p 127.0.0.1::7700 \
  -e MEILI_ENV=development -e MEILI_MASTER_KEY=<redacted> -e MEILI_NO_ANALYTICS=true "$@" "$img" >/dev/null
port=$(docker port "$name" 7700 | head -1 | cut -d: -f2)
H=(-H 'Authorization: Bearer <redacted>' -H 'Content-Type: application/json')
until curl -sf "http://127.0.0.1:$port/health" >/dev/null; do sleep 0.3; done
sleep 2
idle=$(docker exec "$name" sh -c 'grep "^anon " /sys/fs/cgroup/memory.stat | cut -d" " -f2')
curl -s "${H[@]}" -X POST "http://127.0.0.1:$port/indexes" -d '{"uid":"probe","primaryKey":"uid"}' >/dev/null
sleep 1
for i in $(seq 1 6); do
  curl -s "${H[@]}" -X POST "http://127.0.0.1:$port/indexes/probe/documents" \
    -d "[{\"uid\":\"d$i\",\"title\":\"hello $i\",\"body\":\"some body text $i\",\"workspace_id\":\"w$i\"}]" >/dev/null &
done
wait
sleep 6
tasks=$(curl -s "${H[@]}" "http://127.0.0.1:$port/tasks?indexUids=probe" | grep -o "\"status\":\"[a-z]*\"" | sort | uniq -c | tr -s " \n" " ")
peak=$(docker exec "$name" cat /sys/fs/cgroup/memory.peak 2>/dev/null || echo dead)
st=$(docker inspect -f '{{.State.Status}} oomkilled={{.State.OOMKilled}} exit={{.State.ExitCode}}' "$name")
threads=$(docker exec "$name" sh -c 'ls /proc/1/task | wc -l' 2>/dev/null)
echo "$label idle_anon_mb=$((idle/1048576)) peak_mb=$(( ${peak/dead/0} / 1048576 )) threads=$threads state=[$st] tasks=[$tasks] args=[$*]"
docker rm -f "$name" >/dev/null
