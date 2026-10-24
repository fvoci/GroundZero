#!/bin/sh
case "$0" in
  */cargo) case "$*" in
    *'db bootstrap') step=bootstrap ;;
    *'db migrate') step=migrate ;;
    *) exit 91 ;;
  esac ;;
  */docker) case "$*" in
    *'stop supavisor') step=stop ;;
    *'up --detach --wait db rest') step=database ;;
    *'up --detach --wait supavisor') step=pooler ;;
    *'port db 5432') printf '127.0.0.1:30001\n'; exit 0 ;;
    *'ps --all --quiet '*) printf 'diagnose\n' >> "$SETUP_LOG"; exit 0 ;;
    *) exit 92 ;;
  esac ;;
esac
printf '%s\n' "$step" >> "$SETUP_LOG"
# Every Docker mutation must be scoped to the handle's project.
case "$0" in */docker)
  case "$*" in *'--project-name fvoci-synthetic-setup '*) ;; *) exit 93 ;; esac ;;
esac
if [ "$FAIL_AT" = "$step" ]; then
  printf 'deliberate %s failure\n' "$step" >&2
  exit 42
fi
