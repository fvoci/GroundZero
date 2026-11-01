#!/usr/bin/env bash
# #291: measurement contamination is attributed to actual screen sample windows.
set -euo pipefail
screen_test_root=$(git rev-parse --show-toplevel)
cd "$screen_test_root"
screen_test_out=$(mktemp -d "$screen_test_root/target/perf-screen-overlap-test-XXXXXX")
cat > "$screen_test_out/screens.json" <<'JSON'
[
  {"screen":"board","repeat":1,"sampleStart":1000,"sampleEnd":1500},
  {"screen":"board","repeat":2,"sampleStart":2000,"sampleEnd":2500},
  {"screen":"gantt","repeat":1,"sampleStart":3000,"sampleEnd":3500}
]
JSON
cat > "$screen_test_out/builds.json" <<'JSON'
[
  {"epochMs":1200,"compiler":true,"nodeBuild":false},
  {"epochMs":1400,"compiler":false,"nodeBuild":true},
  {"epochMs":2200,"compiler":false,"nodeBuild":false},
  {"epochMs":2700,"compiler":true,"nodeBuild":false},
  {"epochMs":3500,"compiler":true,"nodeBuild":false}
]
JSON
jq -s -f tools/perf-screen-overlap.jq "$screen_test_out/screens.json" "$screen_test_out/builds.json" > "$screen_test_out/result.json"
jq -e 'map({screen,repeat,count:(.buildSamples|length)}) == [
  {screen:"board",repeat:1,count:2},{screen:"gantt",repeat:1,count:1}
]' "$screen_test_out/result.json" > /dev/null
jq -n '[]' > "$screen_test_out/quiet.json"
jq -s -f tools/perf-screen-overlap.jq "$screen_test_out/screens.json" "$screen_test_out/quiet.json" | jq -e 'length==0' > /dev/null
printf '[{"epochMs":2200,"compiler":false,"nodeBuild":false,"heavyCommand":true}]\n' > "$screen_test_out/heavy.json"
jq -s -f tools/perf-screen-overlap.jq "$screen_test_out/screens.json" "$screen_test_out/heavy.json" |
  jq -e 'map({screen,repeat})==[{screen:"board",repeat:2}]' > /dev/null
printf '[{"epochMs":2200,"compiler":false,"nodeBuild":false,"heavyCommand":"false"}]\n' > "$screen_test_out/invalid-heavy.json"
if jq -s -f tools/perf-screen-overlap.jq "$screen_test_out/screens.json" "$screen_test_out/invalid-heavy.json" > /dev/null 2>&1; then
  printf 'FAIL invalid heavy-command monitor value accepted\n' >&2
  exit 1
fi
for screen_invalid in missing reversed; do
  jq --arg kind "$screen_invalid" '.[0] |= (if $kind=="missing" then del(.sampleStart) else .sampleEnd=0 end)' \
    "$screen_test_out/screens.json" > "$screen_test_out/invalid.json"
  if jq -s -f tools/perf-screen-overlap.jq "$screen_test_out/invalid.json" "$screen_test_out/builds.json" > /dev/null 2>&1; then
    printf 'FAIL invalid measurement window accepted: %s\n' "$screen_invalid" >&2
    exit 1
  fi
done
for screen_replay_case in clean once twice heavy; do
  bash -euo pipefail -s -- "$screen_test_out/$screen_replay_case" "$screen_replay_case" <<'REPLAY'
out=$1
scenario=$2
mkdir -p "$out"
jq -n '[ ["board","gantt"][] as $name | range(1;4) as $repeat |
  {screen:$name,repeat:$repeat,origin:"original",
   sampleStart:($repeat*200 + (if $name=="gantt" then 2000 else 0 end))} |
  .+{sampleEnd:(.sampleStart+100)} ]' > "$out/screens.json"
initial_busy=false
initial_heavy=false
case "$scenario" in heavy) initial_heavy=true ;; clean) ;; *) initial_busy=true ;; esac
printf '{"epochMs":250,"compiler":%s,"nodeBuild":false,"heavyCommand":%s}\n' "$initial_busy" "$initial_heavy" > "$out/concurrency.log"
printf '0\n' > "$out/calls"
busy_build() { return 1; }
wait_for_builds() { return 0; }
stop_monitor() { return 0; }
start_monitor() {
  local calls busy=false
  calls=$(cat "$out/calls")
  if [[ "$scenario" == twice && "$calls" == 0 ]]; then busy=true; fi
  printf '{"epochMs":%s,"compiler":false,"nodeBuild":%s}\n' "$(( (calls+1)*10000+250 ))" "$busy" >> "$out/concurrency.log"
}
pnpm() {
  [[ "$FVOCI_MEASURE_SCREENS" == board ]]
  local calls
  calls=$(( $(cat "$out/calls")+1 ))
  printf '%s\n' "$calls" > "$out/calls"
  jq -n --argjson call "$calls" '[range(1;4) as $repeat |
    {screen:"board",repeat:$repeat,origin:("replacement-"+($call|tostring)),
     sampleStart:($call*10000+$repeat*200)} | .+{sampleEnd:(.sampleStart+100)} ]' > "$out/screens.json"
}
# Exercise the production replay block with only browser/monitor boundaries replaced.
replay_source=$(awk '/^  mkdir -p .*screen-attempts/ {copy=1} copy && /^  monitor_phase=db$/ {exit} copy {print}' tools/measure-perf.sh)
test -n "$replay_source"
eval "$replay_source"
case "$scenario" in clean) expected=0 ;; once|heavy) expected=1 ;; twice) expected=2 ;; esac
test "$(cat "$out/calls")" -eq "$expected"
jq -e --argjson calls "$expected" '
  length==6 and (group_by(.screen)|all(.[]; length==3 and map(.repeat)==[1,2,3])) and
  all(.[]|select(.screen=="gantt"); .origin=="original") and
  all(.[]|select(.screen=="board"); .origin==(if $calls==0 then "original" else "replacement-"+($calls|tostring) end))
' "$out/screens.json" > /dev/null
jq -e 'length==6 and all(.[]; .origin=="original")' "$out/screen-attempts/1.json" > /dev/null
jq -e --argjson calls "$expected" '.attempts==$calls+1 and (.discarded|length)==$calls and all(.discarded[]; .screen=="board" and .repeat==1 and (.buildSamples|length)>0)' "$out/replay-summary.json" > /dev/null
REPLAY
done
printf 'PASS screen build overlap: attribution, invalid windows, quiet, selective replay and repeated contamination\n'
