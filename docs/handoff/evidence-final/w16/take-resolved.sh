#!/usr/bin/env bash
# usage: take-resolved.sh <resolutions/N dir>
# Hand resolutions recorded in <dir>/blobs.txt ("<path> <ours blob> <theirs blob>") and <dir>/files/<path>:
# copy the recorded file only while both sides of the open merge still have exactly the recorded blobs.
set -u
d=$1; ok=1; mh=$(git rev-parse MERGE_HEAD)
while read -r f ours theirs; do
  a=$(git rev-parse "HEAD:$f" 2>/dev/null); c=$(git rev-parse "$mh:$f" 2>/dev/null)
  if [ "$a" = "$ours" ] && [ "$c" = "$theirs" ]; then cp "$d/files/$f" "$f"; git add "$f"; echo "RESOLVED(recorded) $f"
  else echo "DIFFERS $f ours=$([ "$a" = "$ours" ] && echo same || echo diff) theirs=$([ "$c" = "$theirs" ] && echo same || echo diff)"; ok=0; fi
done < "$d/blobs.txt"
[ $ok = 1 ]
