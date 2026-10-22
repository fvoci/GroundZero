#!/usr/bin/env bash
# usage: make-steps-tsv.sh <final-commit>  — final-steps.tsv from the first-parent chain a5cda4e..<final>.
# clean      = git merge-tree of (previous step, pinned head) has no conflict and its tree equals the step tree.
# update     = otherwise; conflicted = merge-tree's conflicted files, changed = files whose step blob differs from merge-tree's.
set -eu
W=$(cd "$(dirname "$0")" && pwd)
final=$1
printf 'order\tpr\tbranch\tpinned_head\tstep_commit\tstep_tree\tprev_step_tree\tkind\tconflicted_files\tchanged_files\n'
i=0
git log --reverse --first-parent --format='%H %P' a5cda4e.."$final" | while read -r c p1 p2; do
  i=$((i + 1))
  read -r n b sha _ < <(awk -v h="$p2" '$3 == h' "$W/pinned-heads.txt")
  out=$(git merge-tree --write-tree --name-only "$p1" "$p2" || true)
  mt=$(printf '%s\n' "$out" | sed -n 1p)
  conflicted=$(printf '%s\n' "$out" | sed -n '2,/^$/p' | sed '/^$/d' | sort -u | paste -sd, -)
  t=$(git rev-parse "$c^{tree}")
  changed=$(git diff --name-only "$mt" "$t" | paste -sd, -)
  kind=clean
  [ -n "$conflicted" ] || [ "$mt" != "$t" ] && kind=update
  printf '%s\t#%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "$n" "$b" "$sha" "$c" "$t" "$(git rev-parse "$p1^{tree}")" "$kind" "${conflicted:--}" "${changed:--}"
done
