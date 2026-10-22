#!/usr/bin/env bash
# usage: monitor.sh <outprefix>  — samples this checkout's test meilisearch container until killed
out=$1
docker events --filter label=com.docker.compose.service=meilisearch --format '{{.Time}} {{.Action}} {{.Actor.Attributes.name}} exitCode={{.Actor.Attributes.exitCode}}' \
  | grep --line-buffered 'w8-meili' | grep --line-buffered -v 'exec_' > "$out.events.log" &
while true; do
  c=$(docker ps --format '{{.Names}}' | grep -E '^fvoci-w8-meili-[0-9a-f]+-test-meilisearch-1$' | head -1)
  if [ -n "$c" ]; then
    ts=$(date -u +%H:%M:%S)
    st=$(docker exec "$c" sh -c 'cat /sys/fs/cgroup/memory.current /sys/fs/cgroup/memory.peak; grep -E "^(anon|shmem|file) " /sys/fs/cgroup/memory.stat | tr "\n" " "; du -sm /meili_data | cut -f1; cat /sys/fs/cgroup/memory.events | tr "\n" " "' 2>/dev/null | tr '\n' ' ')
    echo "$ts $c cur/peak/anon/shmem/file/du_mb/events: $st" >> "$out.mem.log"
  fi
  sleep 1
done
