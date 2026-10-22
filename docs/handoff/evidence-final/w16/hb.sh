#!/usr/bin/env bash
# usage: hb.sh <phase> — one Orca heartbeat for this dispatch
orca orchestration send --from term_c52e516c-d4c5-48f5-a3e4-b1cf369ec6a0 --type heartbeat --subject alive --task-id task_2c03044b9bfe --dispatch-id ctx_18cd3e255878 --phase "$1" >/dev/null 2>&1
