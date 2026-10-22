#!/usr/bin/env bash
# usage: step.sh <pr>  — one merge step of audit/final:
#   git merge --no-ff --no-commit <pinned head>, then resolutions/<pr>/apply.sh if present (conflict resolution and
#   pinned-value fixes for that PR's update), then one merge commit. Stops with the merge open when files stay unmerged.
set -u
W=$(cd "$(dirname "$0")" && pwd)
n=$1
read -r _ b sha _ < <(grep "^$n " "$W/pinned-heads.txt")
git merge --no-ff --no-commit "$sha" > "$W/merge-$n.out" 2>&1
grep -i conflict "$W/merge-$n.out" | sed 's/^/  /'
git diff --name-only --diff-filter=U > "$W/resolutions/conflicted-$n.txt"
if [ -x "$W/resolutions/$n/apply.sh" ]; then
  echo "  apply resolutions/$n/apply.sh"
  "$W/resolutions/$n/apply.sh" || { echo "$n apply FAILED"; exit 1; }
fi
if [ -n "$(git diff --name-only --diff-filter=U)" ]; then echo "$n $b UNMERGED: $(git diff --name-only --diff-filter=U | tr '\n' ' ')"; exit 1; fi
git commit -q --no-edit -m "Merge #$n ($b) in merge order" || exit 1
echo "$n $b $sha OK $(git rev-parse HEAD) tree $(git rev-parse "HEAD^{tree}")"
[ -s "$W/resolutions/conflicted-$n.txt" ] || rm -f "$W/resolutions/conflicted-$n.txt"
