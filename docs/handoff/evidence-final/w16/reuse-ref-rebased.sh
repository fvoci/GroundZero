#!/usr/bin/env bash
# usage: reuse-ref-rebased.sh <ref-merge-commit> <file>...
# For a conflicted file where exactly one side differs from the ref merge's parents, carry the ref resolution
# forward with a 3-way merge-file instead of re-resolving by hand:
#   our side differs   (e.g. #89 changed the file earlier): ours = HEAD,          base = ref^1, theirs = ref result
#   their side differs (the PR head moved since the ref):    ours = ref result,    base = ref^2, theirs = MERGE_HEAD
# A clean result is the ref resolution plus only the newer changes. Anything else is left for hand resolution.
set -u
ref=$1; shift; mh=$(git rev-parse MERGE_HEAD); ok=1
tmp=$(mktemp -d)
for f in "$@"; do
  a=$(git rev-parse "HEAD:$f"); b=$(git rev-parse "$ref^1:$f"); c=$(git rev-parse "$mh:$f"); d=$(git rev-parse "$ref^2:$f")
  git show "$ref:$f" > $tmp/res
  if [ "$c" = "$d" ] && [ "$a" != "$b" ]; then
    git show "HEAD:$f" > $tmp/x; git show "$ref^1:$f" > $tmp/base; git merge-file -p $tmp/x $tmp/base $tmp/res > $tmp/out; rc=$?; how=ours-newer
  elif [ "$a" = "$b" ] && [ "$c" != "$d" ]; then
    git show "$ref^2:$f" > $tmp/base; git show "$mh:$f" > $tmp/y; git merge-file -p $tmp/res $tmp/base $tmp/y > $tmp/out; rc=$?; how=theirs-newer
  else echo "NOT ONE-SIDED $f"; ok=0; continue; fi
  if [ $rc = 0 ]; then cp $tmp/out "$f"; git add "$f"; echo "REBASED($how) $f"; else echo "REBASE CONFLICT($how) $f"; ok=0; fi
done
rm -rf $tmp; [ $ok = 1 ]
