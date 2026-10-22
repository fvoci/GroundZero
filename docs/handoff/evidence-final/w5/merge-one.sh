#!/usr/bin/env bash
# usage: m.sh <pr>  — merge current remote head of PR into HEAD with --no-ff
set -u
S=${S:-.}  # directory holding pr-heads.txt
n=$1
read _ b sha _ _ < <(grep "^$n " $S/pr-heads.txt)
if git merge --no-ff --no-edit -m "Merge #$n ($b) in merge order" "$sha" > $S/merge-$n.out 2>&1; then
  echo "$n $b $sha OK $(git rev-parse HEAD)"
else
  echo "$n $b $sha CONFLICT"; cat $S/merge-$n.out | grep -i conflict
  exit 1
fi
