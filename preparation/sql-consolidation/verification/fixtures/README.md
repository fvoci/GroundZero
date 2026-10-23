# 사전 검사 거부 시험 재현

> **역사 기록이며 현재 상태를 나타내지 않음.** 본문 판단·상태·결과는 명시된 조사/실행 SHA에만 적용한다. 관련 결함이나 SHA를 조사할 때만 참조하며, 현재 기준과 분리 검증은 [증거 안내](../../evidence.md)를 따른다.

기존 Compose, 서버 `db bootstrap`·`db migrate`, 이미지의 `psql`만 사용했다. 아래는 같은 시험을 새 전용 프로젝트에 재현하는 절차다. 원래 프로젝트 이름·소스 SHA·오류·전후 카탈로그는 [PG16](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/preflight/pg16), [PG17 ACL](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/preflight/pg17-acl), [PG17 인증](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/preflight/pg17-credentials)에 있다. 원래 서버 소스는 `9c122173e4c6ec5d33c6be261975f68753008015`이고 사전 검사 구현은 `c41d589`와 같다.

## 공통 준비

저장소 루트의 Bash에서 실행한다. 프로젝트명은 체크아웃 경로 해시와 실행 UUID로 분리한다. 자격 증명은 `deploy/compose.test.yaml`·`xtask/src/compose.rs`의 기존 시험 값이다. 출력에 환경 변수나 비밀번호를 찍지 않는다.

```sh
set -euo pipefail
cargo build --package fvoci-server --locked
proof="target/preflight-$(cat /proc/sys/kernel/random/uuid)"
mkdir "$proof"
project="sqlprep-b-preflight-$(printf %s "$PWD" | sha256sum | cut -c1-12)-$(cat /proc/sys/kernel/random/uuid)"
compose=(docker compose --project-name "$project" --file deploy/compose.test.yaml)
# PG16 거부 시험에만 다음 override를 추가한다. PG17은 기본 구성 그대로 쓴다.
# compose+=(--file preparation/sql-consolidation/verification/fixtures/compose.pg16.yaml)
trap '"${compose[@]}" stop >> "$proof/orchestration.txt" 2>&1' EXIT
"${compose[@]}" up --detach --wait postgres > "$proof/orchestration.txt" 2>&1
port=$("${compose[@]}" port postgres 5432)
port=${port##*:}
export DATABASE_ADMIN_URL="postgres://fvoci:fvoci-test-password@127.0.0.1:$port/fvoci"
export DATABASE_MIGRATION_URL="postgres://fvoci_migrator:fvoci-test-migrator-password@127.0.0.1:$port/fvoci"
export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password
export FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
pg=("${compose[@]}" exec -T postgres psql -XAt -U fvoci -d fvoci -v ON_ERROR_STOP=1)
catalog=preparation/sql-consolidation/verification/fixtures/preflight-catalog.sql
```

## PostgreSQL 16: DDL·원장 생성 전 거부

빈 PG16 fixture에서 migration URL만 fixture 관리자 URL로 지정한다. 이는 버전 검사 이전에 SQLx 이력을 쓰지 않는지 확인하는 시험이며 migrator 권한 증명은 아니다.

```sh
export DATABASE_MIGRATION_URL="$DATABASE_ADMIN_URL"
"${pg[@]}" < "$catalog" | jq . > "$proof/before.json"
for action in bootstrap migrate; do
  set +e
  target/debug/fvoci-server db "$action" > "$proof/$action.txt" 2>&1
  status=$?
  set -e
  test "$status" -eq 1
  rg -q 'PostgreSQL 17 or later is required.*160013' "$proof/$action.txt"
  "${pg[@]}" < "$catalog" | jq . > "$proof/after-$action.json"
  cmp "$proof/before.json" "$proof/after-$action.json"
done
```

전후 JSON의 제품 역할 목록·관계 목록은 비어 있고 스키마·원장은 null이다. `sql-compare --postgres-major`는 17·18만 받으며 16은 [CLI에서 거부](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/preflight/cli-selection.txt)한다.

## PostgreSQL 17: 필수 권한 부족과 원복

새 PG17 fixture에서 다음 세 권한을 **하나씩** 시험했다. 각 경우 PUBLIC 권한을 REVOKE한 뒤 전후 카탈로그를 비교하고 GRANT로 되돌렸다. 기본 이미지에는 migrator에 직접 준 추가 권한이 없으므로 PUBLIC 철회 후 실제 migrator 권한도 없다.

| 권한 문자열                                                 | 기대 Config 메시지                                    |
| ----------------------------------------------------------- | ----------------------------------------------------- |
| `USAGE ON LANGUAGE sql`                                     | `requires USAGE on language sql`                      |
| `USAGE ON LANGUAGE plpgsql`                                 | `requires USAGE on language plpgsql`                  |
| `EXECUTE ON FUNCTION pg_catalog.uuid_extract_version(uuid)` | `requires EXECUTE on pg_catalog.uuid_extract_version` |

각 경우의 명령 형태는 다음과 같다. 새 fixture에서 `action=bootstrap`을 세 번 시험한 뒤 정상 bootstrap으로 역할을 만들고, 기존 역할에서 `bootstrap`·`migrate` 각각 세 번을 시험했다. 합계 9개 거부 경로다. 아래 요청 비밀번호는 기존 값과 서로 바꾼 고정 시험 값이다.

```sh
permission='USAGE ON LANGUAGE sql' # 위 표의 각 항목
expected='requires USAGE on language sql'
capability=sql                    # sql / plpgsql / uuid
stage=fresh                       # 역할 생성 뒤에는 existing
action=bootstrap                  # 기존 역할에서는 migrate도 시험
case_dir="$proof/$stage-$capability-$action"
mkdir "$case_dir"                 # 기존 증거 경로면 실패; 덮어쓰지 않음
"${pg[@]}" -c "REVOKE $permission FROM PUBLIC"
"${pg[@]}" < "$catalog" | jq . > "$case_dir/before.json"
set +e
FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-app-password \
FVOCI_DB_APP_PASSWORD=fvoci-test-migrator-password \
target/debug/fvoci-server db "$action" > "$case_dir/rejection.txt" 2>&1
status=$?
set -e
test "$status" -eq 1
rg -Fq "$expected" "$case_dir/rejection.txt"
"${pg[@]}" < "$catalog" | jq . > "$case_dir/after.json"
cmp "$case_dir/before.json" "$case_dir/after.json"
"${pg[@]}" -c "GRANT $permission TO PUBLIC"
```

권한을 모두 되돌린 상태에서 정상 bootstrap과 migrate를 실행해 이력 `count=93`, `max(version)=94`, `bool_and(success)=true`를 확인했다.

## 실제 비밀번호 인증 확인

첫 ACL 진단에서 `psql -h 127.0.0.1`은 이미지의 trust 규칙을 사용했다. 그 연결은 비밀번호 원복 증거가 아니며 `pg17-acl/catalog-result.txt`에는 카탈로그 판정만 남겼다. `local-original-artifacts.sha256`은 주장 줄을 제외하기 전 로컬 원본을 식별한다.

별도 PG17 fixture에서는 정상 bootstrap 후 Compose DNS `postgres`를 사용했다. [pg_hba_file_rules](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/preflight/pg17-credentials/auth-rules.json)의 일반 host 규칙은 `scram-sha-256`이다. 아래 음성 대조를 **두 역할 모두** 실행해 exit 2와 `password authentication failed`를 확인했다.

```sh
for role in fvoci_app fvoci_migrator; do
  set +e
  "${compose[@]}" exec -T -e PGPASSWORD=fvoci-test-deliberately-incorrect postgres \
    psql -h postgres -XAt -U "$role" -d fvoci -c 'SELECT current_user' > "$proof/wrong-$role.txt" 2>&1
  status=$?
  set -e
  test "$status" -eq 2
  rg -q 'password authentication failed' "$proof/wrong-$role.txt"
done
```

그 뒤 위의 권한 철회·변경된 비밀번호를 요청하는 bootstrap·거부·전후 비교를 세 권한 각각에 대해 반복했다. 각 거부 뒤 아래 두 연결이 원래 비밀번호로 성공했다(총 6회). 이 결과만 자격 증명 보존 PASS로 채택했다.

```sh
"${compose[@]}" exec -T -e PGPASSWORD=fvoci-test-migrator-password postgres \
  psql -h postgres -XAt -U fvoci_migrator -d fvoci -v ON_ERROR_STOP=1 -c 'SELECT current_user'
"${compose[@]}" exec -T -e PGPASSWORD=fvoci-test-app-password postgres \
  psql -h postgres -XAt -U fvoci_app -d fvoci -v ON_ERROR_STOP=1 -c 'SELECT current_user'
```

각 fixture는 종료 시 자기 프로젝트의 `stop`만 실행했다. 컨테이너·네트워크·볼륨 제거 명령은 쓰지 않았다. 이 fixture의 권한 철회는 시험용 superuser 작업이며 Cloud에서 수행하지 않았다. 비-superuser 설치의 기능 검증은 [별도 제한 역할 실행](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/pg17-restricted/result.json)이다.
