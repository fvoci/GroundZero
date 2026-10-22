#!/usr/bin/env bash
# usage: build.sh [first-pr]  — run step.sh for every pinned head in merge order (pinned-heads.txt order),
# starting at <first-pr> (default: the first). Check out the previous step commit before starting midway.
set -u
W=$(cd "$(dirname "$0")" && pwd)
from=${1:-}; started=0
[ -z "$from" ] && started=1
while read -r n _; do
  [ "$n" = "$from" ] && started=1
  [ $started = 1 ] || continue
  "$W/step.sh" "$n" < /dev/null | grep -E '^[0-9]+ ' || exit 1
done < "$W/pinned-heads.txt"
