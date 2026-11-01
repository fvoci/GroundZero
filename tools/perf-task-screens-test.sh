#!/usr/bin/env bash
# #291: complement the original source-boundary tests; reject each broken aggregate arm.
set -euo pipefail
task_root=$(git rev-parse --show-toplevel)
cd "$task_root"
task_fixture=tools/fixtures/perf-task-screens-aggregate.json
task_out=$(mktemp -d "$task_root/target/perf-task-screens-test-XXXXXX")
jq -f tools/perf-depth.jq "$task_fixture" > "$task_out/valid.json"
jq -e 'length==3 and all(.[]; .depth==4)' "$task_out/valid.json" > /dev/null
task_cases=0
for task_screen in board gantt task-detail; do
  while IFS= read -r task_path; do
    for task_defect in missing early unfinished duplicate; do
      jq --arg screen "$task_screen" --arg path "$task_path" --arg defect "$task_defect" '
        map(if .screen==$screen then .requests |=
          (if $defect=="missing" then map(select(.path!=$path))
           elif $defect=="duplicate" then . + [.[] | select(.path==$path)]
           else map(if .path==$path then
             if $defect=="early" then .start-=1 else .end=null end
             else . end) end) else . end)
      ' "$task_fixture" > "$task_out/input.json"
      if jq -f tools/perf-depth.jq "$task_out/input.json" > /dev/null 2> "$task_out/error.log"; then
        printf 'FAIL aggregate witness accepted: %s %s %s\n' "$task_screen" "$task_path" "$task_defect"
        exit 1
      fi
      task_cases=$((task_cases+1))
    done
  done < <(jq -r --arg screen "$task_screen" '.[] | select(.screen==$screen) | .requests[].path' "$task_fixture")
done
for task_pattern in 'task-context$' 'detail-read$' '/comments$' '/time$' '/backlinks$' '/marks/task/task$'; do
  jq --arg pattern "$task_pattern" '
    map(if .screen=="task-detail" then .requests |= map(if (.path | test($pattern)) then
      if ($pattern=="task-context$") then .start=30
      elif ($pattern=="detail-read$") then .start=60
      else .start=96 | .end=106 end
      else . end) else . end)
  ' "$task_fixture" > "$task_out/serialized.json"
  if jq -f tools/perf-depth.jq "$task_out/serialized.json" > /dev/null 2> "$task_out/error.log"; then
    printf 'FAIL aggregate witness accepted serialization: %s\n' "$task_pattern"
    exit 1
  fi
  task_cases=$((task_cases+1))
done
printf 'PASS aggregate dependency witnesses: %s rejected mutations\n' "$task_cases"
