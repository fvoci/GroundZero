#!/usr/bin/env bash
# 사용: peaks.sh <pg-activity.txt> [시작 HH:MM:SS] [끝 HH:MM:SS] — 구간의 열별 최댓값과 total 상위 표본.
f=$1; from=${2:-00:00:00}; to=${3:-99:99:99}
grep -E '^[0-9]{2}:[0-9]{2}:[0-9]{2}\.' "$f" | awk -v a="$from" -v b="$to" '
  substr($1,1,8)>=a && substr($1,1,8)<=b {
    n++; for(i=2;i<=10;i++) if($i>m[i]){m[i]=$i; at[i]=$1}
  }
  END{split("t total app app_direct owner pg_admin other app_active app_idle_tx app_lock_wait",h," ");
      printf "samples %d\n", n; for(i=2;i<=10;i++) printf "max %-14s %4d at %s\n", h[i], m[i], at[i]}'
