#!/usr/bin/env bash
# usage: reuse.sh <ref-merge-commit>  — for every conflicted file, reuse the ref merge's result
# only if our side and their side of that file are identical to the ref merge's parents.
set -u
ref=$1; theirs=$(git rev-parse MERGE_HEAD)
ok=1
for f in $(git diff --name-only --diff-filter=U); do
  a=$(git rev-parse "HEAD:$f" 2>/dev/null); b=$(git rev-parse "$ref^1:$f" 2>/dev/null)
  c=$(git rev-parse "$theirs:$f" 2>/dev/null); d=$(git rev-parse "$ref^2:$f" 2>/dev/null)
  if [ "$a" = "$b" ] && [ "$c" = "$d" ]; then git checkout "$ref" -- "$f"; echo "REUSED $f"; else echo "DIFFERS $f ours=$([ "$a" = "$b" ] && echo same || echo diff) theirs=$([ "$c" = "$d" ] && echo same || echo diff)"; ok=0; fi
done
[ $ok = 1 ]
