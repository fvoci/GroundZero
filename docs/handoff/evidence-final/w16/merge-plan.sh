#!/usr/bin/env bash
# W16 merge plan — DRAFT, NOT EXECUTED by W16. For the coordinator, who merges.
#
# Merges the 27 audit PRs into main in the order of final-steps.tsv (first-parent order of local `audit/final`), so that
# after every step main's tree is exactly that step's tree from the table:
#   clean  step: merge the PR as it is (GitHub's merge of main + pinned head gives the step tree; W16 checked this with
#                `git merge-tree`).
#   update step: first fast-forward the PR branch to an update commit whose tree is the step tree:
#                  git commit-tree <step tree> -p <pinned PR head> -p <origin/main>
#                (origin/main's tree must equal the previous step's tree), push it, wait for CI, then merge the PR.
#                Merging that commit into main gives the step tree again (its merge base with main is main itself).
# After every merge the script checks origin/main^{tree} == step tree and stops otherwise.
# If branch protection requires a PR to be up to date with main, a clean step also needs the update commit (same command,
# its tree is the step tree); the tree checks stay the same.
#
# usage: merge-plan.sh [first-order]   (default 1; to resume, pass the order of the first step not yet merged)
# needs: git, gh (logged in with merge rights), jq; run from a clean checkout of fvoci/GroundZero that has the step
#        commits (fetch `audit/final` or run where it exists: `git cat-file -e <step tree>` must succeed).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
steps="$here/final-steps.tsv"
first=${1:-1}
remote=origin

die() { echo "STOP: $*" >&2; exit 1; }
tree_of() { git rev-parse "$1^{tree}"; }

[ -z "$(git status --porcelain)" ] || die "working tree not clean"

# columns: order pr branch pinned_head step_commit step_tree prev_step_tree kind conflicted_files changed_files
tail -n +2 "$steps" | while IFS=$'\t' read -r order pr branch pinned step_commit step_tree prev_tree kind conflicted changed; do
  [ "$order" -ge "$first" ] || continue
  n=${pr#\#}
  echo "== step $order: $pr $branch ($kind)"
  git cat-file -e "$step_tree" || die "step tree $step_tree not in this repository"

  git fetch --quiet "$remote"
  main=$(git rev-parse "$remote/main")

  # 1. main must be exactly the previous step.
  [ "$(tree_of "$main")" = "$prev_tree" ] || die "$remote/main tree is not the tree of step $((order - 1))"

  # 2. The PR must still be the pinned head, open, and based on main.
  info=$(gh pr view "$n" --json state,headRefName,headRefOid,baseRefName)
  [ "$(jq -r .state <<<"$info")" = OPEN ] || die "$pr is not open"
  [ "$(jq -r .headRefName <<<"$info")" = "$branch" ] || die "$pr head branch is not $branch"
  [ "$(jq -r .headRefOid <<<"$info")" = "$pinned" ] || die "$pr head moved from the pinned $pinned"
  [ "$(git rev-parse "$remote/$branch")" = "$pinned" ] || die "$remote/$branch is not the pinned $pinned"
  if [ "$(jq -r .baseRefName <<<"$info")" != main ]; then
    # Only #62 (based on fix/audit-schedule-links = #53, merged at step 4) is expected here.
    echo "   retarget $pr to main (was $(jq -r .baseRefName <<<"$info"))"
    gh pr edit "$n" --base main
  fi

  head=$pinned
  # 3. Update step: fast-forward the PR branch to <step tree> on top of the pinned head and main.
  if [ "$kind" = update ]; then
    echo "   conflicted: $conflicted"
    echo "   changed:    $changed"
    msg="chore: merge main into $branch for the final integration (step $order)

The tree is the W16 final integration step $order (local audit/final $step_commit):
conflict resolution and fixes listed in final-steps.tsv.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
    head=$(git commit-tree "$step_tree" -p "$pinned" -p "$main" -m "$msg")
    git merge-base --is-ancestor "$pinned" "$head" || die "update commit does not contain the pinned head"
    git push "$remote" "$head:refs/heads/$branch"   # fast-forward only (no --force)
    echo "   pushed $head; waiting for CI"
    sleep 20
    gh pr checks "$n" --watch --fail-fast || die "$pr checks failed on $head"
  fi

  # 4. Merge and check main's tree. A clean step shows the PR's last CI first (not re-run against the new main).
  [ "$kind" = update ] || gh pr checks "$n" || die "$pr checks are not green"
  gh pr merge "$n" --merge --match-head-commit "$head"
  git fetch --quiet "$remote"
  [ "$(tree_of "$remote/main")" = "$step_tree" ] || die "$remote/main tree after $pr is not the step tree $step_tree"
  echo "   merged: $remote/main $(git rev-parse "$remote/main") tree $step_tree"
done

echo "done: $remote/main tree is the final step tree"
