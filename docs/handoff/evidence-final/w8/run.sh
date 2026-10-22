#!/usr/bin/env bash
# usage: run.sh <label> <xtask test args...> — run from the checkout root with the meili monitor
label=$1; shift
E=target/audit-evidence/w8/logs
<scratchpad>/monitor.sh $E/$label & mon=$!
cargo xtask test "$@" > $E/$label.log 2>&1
rc=$?
echo "exit=$rc" >> $E/$label.log
pkill -P $mon; kill $mon; pkill -f '[d]ocker events --filter label=com.docker.compose.service=meilisearch'
exit $rc
