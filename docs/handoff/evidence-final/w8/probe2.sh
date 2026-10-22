#!/usr/bin/env bash
# usage: probe2.sh <label> [docker run args...] — mimic search.rs: parallel settings PATCH, doc adds, filtered searches
label=$1; shift
img=getmeili/meilisearch:v1.37.0@sha256:9cde2774864e6c19cdd09fcd2950bc01ac9a0454b6f343fffafa14faae6eb7ea
name=w8-meili-probe-$label
docker rm -f "$name" >/dev/null 2>&1
docker run -d --name "$name" -m 512m --memory-swap 512m --tmpfs /meili_data -p 127.0.0.1::7700 \
  -e MEILI_ENV=development -e MEILI_MASTER_KEY=<redacted> -e MEILI_NO_ANALYTICS=true "$@" "$img" >/dev/null
port=$(docker port "$name" 7700 | head -1 | cut -d: -f2)
U=http://127.0.0.1:$port
H=(-H 'Authorization: Bearer <redacted>' -H 'Content-Type: application/json')
until curl -sf "$U/health" >/dev/null; do sleep 0.3; done
sleep 1
curl -s "${H[@]}" -X POST "$U/indexes" -d '{"uid":"fvoci","primaryKey":"uid"}' >/dev/null
SET='{"searchableAttributes":["title","reference","text"],"filterableAttributes":["kind","workspace_id","owner_user_id","id","module_id","status","assignee_ids","day"],"displayedAttributes":["kind","id","title","reference","workspace_id","item_id","text"]}'
for r in 1 2 3; do
  for i in $(seq 1 8); do
    curl -s "${H[@]}" -X PATCH "$U/indexes/fvoci/settings" -d "$SET" >/dev/null &
    curl -s "${H[@]}" -X POST "$U/indexes/fvoci/documents?primaryKey=uid" \
      -d "[{\"uid\":\"doc_$r$i\",\"kind\":\"doc\",\"id\":\"$r$i\",\"title\":\"zq$r$i title\",\"text\":\"body text zq$r$i lorem ipsum\",\"workspace_id\":\"w$i\",\"assignee_ids\":[\"a$i\"],\"day\":\"2026-10-2$i\",\"status\":\"todo\"}]" >/dev/null &
    curl -s "${H[@]}" -X POST "$U/indexes/fvoci/search" -d "{\"q\":\"zq$r$i\",\"filter\":\"workspace_id IN [w1,w2] AND kind = doc\",\"attributesToCrop\":[\"text\"],\"cropLength\":24}" >/dev/null &
  done
  wait
  sleep 2
done
sleep 4
peak=$(docker exec "$name" cat /sys/fs/cgroup/memory.peak 2>/dev/null || echo 0)
ev=$(docker exec "$name" sh -c 'grep -E "^(max|oom_kill) " /sys/fs/cgroup/memory.events | tr "\n" " "' 2>/dev/null)
tasks=$(curl -s "${H[@]}" "$U/tasks?limit=1000" | grep -o '"status":"[a-z]*"' | sort | uniq -c | tr -s ' \n' ' ')
st=$(docker inspect -f '{{.State.Status}} oomkilled={{.State.OOMKilled}}' "$name")
echo "$label peak_mb=$((peak/1048576)) events=[$ev] state=[$st] tasks=[$tasks] args=[$*]"
docker rm -f "$name" >/dev/null
