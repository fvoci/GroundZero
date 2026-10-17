#!/bin/sh
# 셀프호스트 첫 설치 때만(빈 데이터 볼륨) 실행된다. Keycloak 프로필을 쓰면 전용 계정과 DB를 만든다.
set -eu
if [ -n "${KEYCLOAK_DB_PASSWORD:-}" ]; then
  psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" -v pw="$KEYCLOAK_DB_PASSWORD" <<'SQL'
CREATE ROLE keycloak LOGIN PASSWORD :'pw';
CREATE DATABASE keycloak OWNER keycloak;
SQL
fi
