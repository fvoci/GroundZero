#!/usr/bin/env bash
# usage: summ.sh <label> — from target/audit-evidence/w8/logs
l=$1
r=$(grep '^exit=' $l.log)
t=$(grep -E '^test result' $l.log | awk '{p+=$4; f+=$6} END {print "binaries="NR, "passed="p, "failed="f}')
m=$(awk '{for(i=1;i<=NF;i++){if($i=="anon"&&$(i+1)+0>a)a=$(i+1); if($i=="max"&&$(i+1)+0>x)x=$(i+1)}; if($5+0>p)p=$5} END {printf "peak_mb=%.0f max_anon_mb=%.0f memory.events_max=%d", p/1048576, a/1048576, x}' $l.mem.log)
e=$(grep -E ' (oom|die) ' $l.events.log | awk '{print $2, $4}' | tr '\n' ';')
echo "$l: $r $t $m events=[$e]"
