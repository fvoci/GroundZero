#!/usr/bin/env bash
# Runs the recurrence test binary N times against the stack xtask prepared (env inherited).
# W9_PROBE=1: first runs the temporary FV001 probe once (W9_PROBE_SPARE ids of slack).
set -u
n=${W9_REPEAT_N:-30}
tag=${W9_REPEAT_TAG:-run}
out=${W9_REPEAT_OUT:?}
mkdir -p "$out"
cargo test -p fvoci-server --locked --test recurrence --no-run >/dev/null 2>&1
if [ "${W9_PROBE:-0}" = 1 ]; then
  plog="$out/$tag-probe.log"
  if cargo test -p fvoci-server --locked --test recurrence -- --exact w9_probe_bound_growth_between_ids_and_tick --show-output >"$plog" 2>&1; then
    echo "$tag probe PASS"
  else
    echo "$tag probe FAIL"
  fi
  grep -E 'W9DIAG|FV001|panicked' "$plog" | sed 's/\x1b\[[0-9;]*m//g' | cut -c1-300
fi
fail=0
for i in $(seq 1 "$n"); do
  log="$out/$tag-$i.log"
  if cargo test -p fvoci-server --locked --test recurrence -- --skip w9_probe --show-output >"$log" 2>&1; then
    echo "$tag $i PASS $(grep -h -o 'W9DIAG lock-failure.*' "$log" | head -1) $(grep -o 'finished in [0-9.]*s' "$log")"
    rm -f "$log"
  else
    fail=$((fail+1))
    echo "$tag $i FAIL ($(grep -c FV001 "$log") FV001 warnings) $(grep -o 'panicked at [^ ]*' "$log" | tr '\n' ' ') $(grep -o 'finished in [0-9.]*s' "$log")"
  fi
done
echo "$tag SUMMARY: $fail/$n failed"
