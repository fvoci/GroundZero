# 객체와 호출 근거

기준 커밋은 `3db81d1deae24b16478581ebd6a06de2b2a2d8c9`다(#92에 `fvoci/sql-policy` `b01d74a`를 병합한 커밋, 마이그레이션 디렉터리는 `b01d74a`와 같다).
아래 수치는 그 커밋의 97개 마이그레이션을 PostgreSQL 18.3의 빈 DB에 적용한 직후이며 아래 [재생성 명령](#재생성-명령)으로 만들었다. 최종 통합 기준선 승인과 구분한다.
처음 목록은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`(93개)에서 Worker B 소유 `sqlprep-b-cb65cc31-postgres-1`로 만들었다. 그 수치는 아래에 이력으로만 남긴다.
정책 반영으로 바뀐 객체의 목록과 호출 위치는 [analysis-a.md](../analysis-a.md#정책-반영으로-바뀐-객체)에 있다.

## 읽는 순서

| 파일                                              | 내용과 한계                                                                                                                                                                                                                                               |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `catalog-postgres.jsonl`                          | 객체별 실제 카탈로그. `kind`, `identity`, `definition`의 JSON Lines. `catalog-inventory.sql`(B의 `verification/catalog.sql` 초기판)로 얻은 원본 행을 JSON으로 변환. SQLx 원장과 DB 기본 역할·확장을 포함하므로 객체 이름을 구분해서 센다                  |
| `function-history.jsonl`                          | 현재 소스의 함수 CREATE 438건, CREATE OR REPLACE 189건, DROP 138건, 총 765건. 각 선언의 원본 파일·시작 줄·전체 인자 목록. OUT 인자로 반환형을 표현한 함수도 포함                                                                                          |
| `function-map.jsonl`                              | 최종 앱 함수 300개의 서명·소유자·SECURITY DEFINER·search_path·ACL, 마지막 선언 위치, SQL 본문의 다른 최종 함수 이름, 소스 등장 위치                                                                                                                       |
| `retired-signatures.jsonl`                        | 138개 과거 DROP 선언과 같은 이름으로 현재 남은 서명. 같은 이름의 새 서명이 있으므로 이름 전체를 삭제하라는 목록이 아님                                                                                                                                    |
| `source-occurrences.jsonl`                        | `crates/server/src`, `crates/server/tests`, `xtask/src`, `deploy/postgres`, `docs`의 app_* 등장 633건. 실제 호출, 문자열 Job, 문서·주석이 원문과 경로로 구분됨. `xtask/src` 1건은 #92 비교 도구의 변조 시험 문자열(`sql_compare.rs`)이며 제품 호출이 아님 |
| `direct-table-occurrences.jsonl`                  | 같은 경로에서 최종 테이블에 대한 FROM/JOIN/INTO/UPDATE/TABLE 문맥 354건. 직접 조회·변경·테스트 fixture·문서의 근거이며 SQL parser 결과는 아님. `xtask/src/sql_compare*` 3건은 #92 비교 도구의 시험 문자열                                                 |
| `object-function-references.jsonl`                | CHECK·정책·트리거에서 최종 함수 이름을 참조하는 객체 72개. 테이블 RLS와 SQL 내부 호출을 외부 Rust 호출과 함께 보기 위한 목록                                                                                                                              |
| `dependencies.sql`, `dependencies-postgres.jsonl` | pg_depend가 기록한 의존성 3091건. 행 타입·테이블·제약·트리거·함수 사이의 구조적 의존성. 물리 OID 대신 객체 식별자를 사용                                                                                                                                  |
| `tables.md`                                       | 55개 테이블의 실제 RLS/강제 RLS 상태와 테이블 ACL. 열 단위 권한은 원본 카탈로그의 column 항목으로 확인                                                                                                                                                    |

모든 JSON 목록은 객체 하나를 한 줄에 담는 JSON Lines다. `jq -s`로 읽으면 행 순서를 유지한 배열이 된다.
배열 형식에서 변환한 여섯 목록은 변환 전후 객체 키를 정렬한 배열의 SHA-256과 행 수를 대조하여 데이터가 같음을 확인했다.
최초 추출(`cb65cc3`)의 원본은 B worktree의 `target/sql-compare-baseline-export/`에, 현재 목록의 원본 행·덤프는 이 체크아웃의 `target/sql-regen-3db81d1deae2-20261023t065319z/`에만 보존한다.
PR에서 검토할 카탈로그 근거는 위 `catalog-postgres.jsonl`이며, 로컬 원본 경로를 Git에 포함된 파일로 해석하지 않는다.

함수 지도에서 `body_name_references`는 주석을 뺀 본문을 최종 함수 이름 목록과 대조한 **이름 참조**다.
문자열 본문은 pg_depend에 모두 기록되지 않으므로 이 목록과 구조적 의존성을 함께 본다.
이름 참조를 인자 타입까지 바인딩한 호출 그래프나 실행 증거로 주장하지 않는다. 현재 앱 함수는 이름당 서명이 하나여서
중복 이름으로 어느 오버로드인지 모호한 경우는 0건이다. 모든 최종 함수에 마지막 선언 위치가 연결된다.
호출 실행과 인자 계약은 B의 기존 통합 테스트와 카탈로그 비교로 검증한다.

## 최종 객체 수

SQLx 원장을 뺀 앱 객체를 센다. PostgreSQL 18은 NOT NULL을 pg_constraint에 노출하므로 이전 major와 단순 숫자 비교하지 않는다.

| 종류                                                     |                            수 |
| -------------------------------------------------------- | ----------------------------: |
| 테이블 / 실제 테이블 열                                  |                      55 / 497 |
| 함수 / SECURITY DEFINER                                  |                     300 / 278 |
| PK / UNIQUE / FK / CHECK / NOT NULL / 제약 트리거        | 54 / 31 / 138 / 144 / 385 / 1 |
| 인덱스                                                   |                           161 |
| 사용자 트리거(제약 트리거 포함)                          |                            25 |
| RLS 정책 / RLS 활성 테이블 / 강제 RLS 테이블             |                   51 / 51 / 0 |
| 명시 sequence / view / materialized view / enum / domain |             0 / 0 / 0 / 0 / 0 |
| 앱 역할 명시 기본 ACL 레코드                             |                             0 |
| 마이그레이션 직후 필수 초기 데이터 행                    |                             0 |

카탈로그 파일에는 인덱스의 pg_attribute 행도 있으므로 전체 column 행 774개를 실제 테이블 열 497개로 잘못 해석하지 않는다.
테이블마다 자동 생성되는 composite/array 타입과 SQLx 원장 타입도 type 항목에 포함되어 있다.
PK가 54개인 것은 원본 상태다. 후보에서 임의로 추가하거나 삭제하지 않는다.

이전 수치(이력, `cb65cc3` 93개 마이그레이션): 테이블 열 493, 함수 298 / SECURITY DEFINER 277, UNIQUE 30, NOT NULL 382,
인덱스 160, 사용자 트리거 24, 함수 이력 753(431/189/133), 등장 589, 직접 테이블 문맥 345, 참조 객체 71, 의존성 3079.
차이는 [정책 반영으로 바뀐 객체](../analysis-a.md#정책-반영으로-바뀐-객체)의 0095~0098 변경, 정책 구현이 더한 코드·문서의 등장, #92 도구 파일의 등장이다.

## 역할·소유자·권한

- 앱 객체 소유자는 `fvoci_migrator`다. `fvoci_app`·`fvoci_migrator`는 LOGIN/INHERIT이며 이 새 PG18 DB에서
  SUPERUSER/CREATEROLE/CREATEDB/BYPASSRLS/REPLICATION은 모두 거짓이다. bootstrap 자체는 기존 migrator의 CREATEDB를 허용하므로
  이 측정값을 모든 설치에 강제하는 새 규칙으로 바꾸지 않는다.
- 두 역할의 search_path는 `fvoci`이며 다른 역할의 멤버가 아니다. DB 기본 시스템 멤버십 3건은 원본 카탈로그에 별도로 보존했다.
- 278개 SECURITY DEFINER 함수는 모두 `search_path=fvoci, pg_temp`다. PUBLIC EXECUTE는 0건이다.
  순수 도우미의 기존 PUBLIC EXECUTE는 스키마 USAGE와 별개이며 원본대로 유지한다.
- RLS가 꺼진 `document_collab_file_refs`, `oidc_login_states`, `scheduled_runs`, `sessions`에는 앱 역할의 테이블 권한이 없다.
  정의자 함수 경로가 경계를 이룬다. 모든 테이블에 FORCE RLS를 임의로 켜면 소유자 함수의 동작이 달라지므로 하지 않는다.
- column ACL도 보존한다. 예를 들어 `workspaces.name`의 UPDATE 권한을 전체 테이블 UPDATE로 넓히지 않는다.
- 명시 default ACL은 0건이다. NULL ACL은 권한 없음이 아니라 PostgreSQL 기본 ACL을 뜻하므로 카탈로그 조회가 `acldefault`로
  실효 항목을 펼쳤다. 후보에 불필요한 전역 default ACL 정책을 새로 만들지 않는다.

## DB 제공 의존성과 D-88

`uuid_extract_version(uuid)`는 PostgreSQL 17.0부터 있는 `pg_catalog`의 LANGUAGE internal 함수이며 새 UUID를 생성하지 않는다.
최초 17.0 태그의 [카탈로그](https://github.com/postgres/postgres/blob/REL_17_0/src/include/catalog/pg_proc.dat#L8847)와
[구현](https://github.com/postgres/postgres/blob/REL_17_0/src/backend/utils/adt/uuid.c#L445)에서 확인되므로 Supabase 전용 의존성이 아니다.
이 테스트 DB에서 소유자는 클러스터 초기 역할 `fvoci`, 반환형 smallint, IMMUTABLE/PARALLEL SAFE/STRICT/LEAKPROOF다.
카탈로그의 function 항목에 정확한 정의·ACL을 남겼다. `plpgsql` 1.0은 DB 제공 확장으로 `pg_catalog`에 있고,
앱 SQL이 새로 설치하는 확장은 0개다. 후보는 이 시스템 객체를 재정의하지 않는다.

pg_depend에 기록된 fvoci 객체의 대상 중 외부 스키마 객체가 없다는 결과는 시스템 함수 의존성이 없다는 뜻이 아니다.
PostgreSQL의 고정 시스템 객체와 문자열 SQL 본문은 의존성 행에 나타나지 않을 수 있다. 실제 CHECK와 `app_id`가
`uuid_extract_version`을 호출하는 사실을 본문·카탈로그와 함께 확인한다. Supabase PG17의 제공 정의는 B의 별도 환경 기록으로 확인한다.
지원 환경별 어댑터와 정상 재시도는 유지하며, 과거 개발 버전 호환 제거 대상에 넣지 않는다.

## 재생성의 원칙

1. 동일 소스 SHA의 모든 기존 마이그레이션을 빈 격리 DB에 실제 SQLx로 적용한다.
2. `pg_dump --schema-only --schema=fvoci --exclude-table=fvoci._sqlx_migrations`로 마지막 정의와 owner/ACL을 내보낸다.
3. `catalog-inventory.sql`로 별도 카탈로그를 얻고, 모든 테이블의 초기 행을 확인한다. 카탈로그의 비밀번호·비밀값은 수집하지 않는다.
   이 목록 전용 조회는 추출 당시의 `verification/catalog.sql` 초기판이다. 비교기의 현재 `catalog.sql`과 범위·식별자 형식이 다르다.
4. `dependencies.sql`은 읽기 전용 트랜잭션으로 실행한다. 위 JSON Lines 목록과 소스의 CREATE/REPLACE/DROP 선언 및 호출 문맥을 대조한다.
5. 후보는 bootstrap이 만드는 CREATE SCHEMA와 psql 메타 명령을 빼고, 설치용 설정만 SET LOCAL로 한정한다.
   pg_dump의 함수/행 타입/테이블 순서를 보존하며, 파일 번호는 시험 후보의 실제 SQLx 버전이다.
6. B의 원본 대 원본·후보 비교와 기존 앱 역할 테스트를 완료하기 전에는 파일 수나 호출 검색 결과만으로 동일성 PASS를 선언하지 않는다.

## 재생성 명령

후보와 목록을 처음 만든 명령은 작업자 A·B의 실행 기록에 남아 있었다(원본 헬퍼 스크립트는 없음). 아래는 그 명령을
소스 SHA 하나를 입력으로 받게 정리한 것이다. #92 체크아웃 루트에서 실행하며 Compose 파일과 조회 SQL은 이 체크아웃의 것을,
bootstrap·마이그레이션·소스 검색은 `git archive`로 푼 소스 SHA의 것을 쓴다. 출력은 매번 새 `target/sql-regen-*`이고,
DB는 이 실행 전용 Compose 프로젝트의 tmpfs PostgreSQL이며 끝에 `stop`만 한다. Python·새 도구는 쓰지 않는다.

고정 조건과 이유:

- **PostgreSQL 18.3**(`compose.test.pg18.yaml`, cb65cc3 당시 `compose.test.yaml`과 같은 이미지 digest). pg_dump의 출력
  텍스트는 서버·도구 메이저에 따라 다르므로, 17로 만들면 정책 변경이 없어도 후보 파일이 달라진다. 후보의 17·18 설치 검증은 비교기가 따로 한다.
- 카탈로그·의존성·덤프는 시험 관리자 `-U fvoci`와 기본 search_path로 실행한다. `catalog-inventory.sql`의 `regclass`·`regprocedure`
  식별자가 `"$user"` 스키마 규칙으로 미한정(`app_x(uuid)`)이 되는 것이 기존 목록 형식이다. 접속 값은 `deploy/compose.test.yaml`과
  `xtask/src/compose.rs`의 명백한 시험 값이다.
- 소스 검색은 `rg` 15.1.0과 `jq` 1.8.1로 확인했다. 푼 소스는 Git 저장소가 아니고 이 체크아웃의 무시 대상인 `target/` 아래에 있으므로
  `--no-ignore-parent`로 상위 `.gitignore`를 적용하지 않는다. 검색 대상 경로에는 무시 규칙에 걸리는 추적 파일이 없다.
- 후보 머리말 6줄과 끝 4줄은 고정 문구이며, 첫 두 줄의 소스 SHA·마이그레이션 수·PostgreSQL 버전만 입력에서 채운다.
  본문은 `CREATE SCHEMA fvoci;` 다음부터의 덤프에서 psql 메타 명령(`\unrestrict`)을 빼고 두 설치 설정만 `SET LOCAL`로 바꾼다.
- 블록은 커밋하지 않는 새 파일로 저장해 `SRC_SHA=<소스 SHA> bash <파일>`로 실행한다. 표준 입력으로 넘기면 `docker compose exec`가
  나머지 명령을 읽어 버린다.

```sh
set -euo pipefail
: "${SRC_SHA:?40자리 소스 SHA를 지정한다}"
REGEN=$PWD/target/sql-regen-${SRC_SHA:0:12}-$(date -u +%Y%m%dt%H%M%Sz)
INV=$PWD/preparation/sql-consolidation/inventory
mkdir -p target && mkdir "$REGEN" "$REGEN/src" "$REGEN/inventory" # 기존 경로면 실패한다
git archive "$SRC_SHA" | tar -x -C "$REGEN/src"

# 1. 격리 PostgreSQL 18.3에 소스 SHA의 bootstrap과 누적 마이그레이션을 적용한다.
DC=(docker compose --project-name "sqlprep-regen-${REGEN##*/sql-regen-}"
  --file deploy/compose.test.yaml --file deploy/compose.test.pg18.yaml)
trap '"${DC[@]}" stop' EXIT
"${DC[@]}" up --detach --wait postgres
HOST=127.0.0.1:$("${DC[@]}" port postgres 5432 | sed 's/.*://')
(
  cd "$REGEN/src"
  export DATABASE_ADMIN_URL=postgres://fvoci:fvoci-test-password@$HOST/fvoci
  export DATABASE_MIGRATION_URL=postgres://fvoci_migrator:fvoci-test-migrator-password@$HOST/fvoci
  export FVOCI_DB_MIGRATOR_PASSWORD=fvoci-test-migrator-password FVOCI_DB_APP_PASSWORD=fvoci-test-app-password
  cargo run --quiet --package fvoci-server --locked -- db bootstrap
  cargo run --quiet --package fvoci-server --locked -- db migrate
)

# 2. 덤프·카탈로그·의존성·초기 행. 초기 행 파일은 비어 있어야 한다(초기 seed 0행).
"${DC[@]}" exec -T postgres pg_dump -U fvoci -d fvoci --schema-only --schema=fvoci \
  --exclude-table=fvoci._sqlx_migrations > "$REGEN/schema.sql"
PSQL=("${DC[@]}" exec -T postgres psql -X -U fvoci -d fvoci -v ON_ERROR_STOP=1)
"${PSQL[@]}" -A -t < "$INV/catalog-inventory.sql" > "$REGEN/catalog-rows.txt"
"${PSQL[@]}" -q -A -t < "$INV/dependencies.sql" > "$REGEN/inventory/dependencies-postgres.jsonl"
"${PSQL[@]}" -A -t > "$REGEN/initial-rows.jsonl" <<'SQL'
SELECT format('SELECT jsonb_build_object(''table'',%L,''rows'',jsonb_agg(to_jsonb(t))) FROM %I.%I t HAVING count(*)>0;',table_name,table_schema,table_name) FROM information_schema.tables WHERE table_schema='fvoci' AND table_type='BASE TABLE' AND table_name<>'_sqlx_migrations' ORDER BY table_name
\gexec
SQL
test ! -s "$REGEN/initial-rows.jsonl"

# 3. 후보 SQL.
N=$(find "$REGEN/src/crates/server/migrations" -maxdepth 1 -name '*.sql' | wc -l)
PGVER=$(sed -n 's/^-- Dumped from database version \([0-9.]*\) .*/\1/p' "$REGEN/schema.sql")
{
  printf '%s\n' "-- 검토용 최종 상태 후보. 적용 기준: $SRC_SHA." \
    "-- 원본 ${N}개 마이그레이션 적용 후 PostgreSQL $PGVER 카탈로그에서 추출했다." \
    '-- 기존 db bootstrap 뒤 빈 DB에 SQLx의 단일 트랜잭션으로 적용한다.' \
    '-- 이력 연결이 아니다: 마지막 함수/테이블 정의만 있으며 과거 데이터 보정은 포함하지 않는다.' \
    '-- pg_dump 의존성 순서를 보존하여 행 타입을 쓰는 함수 앞에 필요한 테이블이 온다.' \
    '-- 함수 간 순환 참조의 본문 검사는 설치 트랜잭션 중에만 늦춘다.' \
    "SET LOCAL search_path = '';" 'SET LOCAL check_function_bodies = false;' ''
  sed '1,/^CREATE SCHEMA fvoci;$/d' "$REGEN/schema.sql" | sed '1,2{/^$/d}' \
    | sed '/^\\unrestrict /d; s/^SET default_tablespace /SET LOCAL default_tablespace /; s/^SET default_table_access_method /SET LOCAL default_table_access_method /'
  printf '%s\n' '-- SQLx 0.8.6은 커밋 전에 이 트랜잭션 안에서 비정규화 이름 _sqlx_migrations에 기록한다.' \
    '-- bootstrap 계약의 search_path로 돌려 그 원장 쓰기도 같은 스키마를 쓰게 한다.' \
    'SET LOCAL search_path = fvoci;' '-- 모든 SET LOCAL은 SQLx 트랜잭션이 끝나면 기존 세션 설정으로 복구된다.'
} > "$REGEN/0001_final_state.sql"
if grep -n '^\\\|^SET ' "$REGEN/0001_final_state.sql" | grep -v '^[0-9]*:SET LOCAL '; then
  echo 'psql meta command or session-wide SET left in the candidate' >&2
  exit 1
fi

# 4. 목록(JSON Lines)과 tables.md. 소스 경로는 기존 목록과 같다.
(
  cd "$REGEN/src"
  out=$REGEN/inventory
  paths=(crates/server/src crates/server/tests xtask/src deploy/postgres docs)
  jq -Rc 'capture("^(?<kind>[^|]+)\\|(?<identity>[^|]+)\\|(?<definition>.*)$") | .definition |= fromjson' \
    "$REGEN/catalog-rows.txt" > "$out/catalog-postgres.jsonl"
  rg --no-ignore-parent --json -n -o '\bapp_[a-zA-Z0-9_]+\b' "${paths[@]}" \
    | jq -c -s '[.[] | select(.type == "match") | .data as $d | $d.submatches[]
        | {symbol: .match.text, path: $d.path.text, line: $d.line_number, source: ($d.lines.text | rtrimstr("\n"))}]
        | sort_by(.symbol, .path, .line) | unique | .[]' > "$out/source-occurrences.jsonl"
  rg --no-ignore-parent --json -n -U \
      '(?m)^(CREATE(?: OR REPLACE)? FUNCTION|DROP FUNCTION)\s+[a-zA-Z0-9_]+\([^;]*?(?:\)\s*(?:RETURNS|LANGUAGE|;))' \
      crates/server/migrations --glob '*.sql' \
    | jq -c -s '[.[] | select(.type == "match") | .data as $d | $d.submatches[]
        | {path: $d.path.text, line: $d.line_number, statement: (.match.text | sub("\\s+(RETURNS|LANGUAGE)$"; ""))}]
        | sort_by(.path, .line) | .[]' > "$out/function-history.jsonl"
  jq -c -n --slurpfile catalog "$out/catalog-postgres.jsonl" --slurpfile history "$out/function-history.jsonl" \
      --slurpfile occurrences "$out/source-occurrences.jsonl" '
    def name: split("(")[0] | split(".")[-1];
    [$catalog[] | select(.kind == "function" and ((.identity | name) != "uuid_extract_version"))] as $functions
    | [$functions[] | .identity | name] as $names
    | $functions[] | (.identity | name) as $name
    | {identity: .identity, name: $name, owner: .definition.owner,
       security_definer: .definition.security_definer, config: .definition.config, acl: .definition.acl,
       last_definition: ([$history[] | select(.statement | test("^CREATE( OR REPLACE)? FUNCTION " + $name + "\\("))] | last),
       body_name_references: ([.definition.definition | sub("(?s)^.*?AS \\$[^$]*\\$"; "") | gsub("--[^\n]*"; "")
         | scan("[A-Za-z_][A-Za-z_0-9]*\\s*\\(") | sub("\\s*\\($"; "") | select(. as $n | $names | index($n))] | unique),
       external_references: [$occurrences[] | select(.symbol == $name) | {path, line, source}]}' \
    > "$out/function-map.jsonl"
  jq -c -n --slurpfile catalog "$out/catalog-postgres.jsonl" --slurpfile functions "$out/function-map.jsonl" '
    [$functions[].name] as $names
    | $catalog[] | select(.kind == "policy" or .kind == "trigger" or .kind == "constraint")
    | {kind, identity, function_name_references: ([.definition | tostring | scan("[A-Za-z_][A-Za-z_0-9]*\\s*\\(")
        | sub("\\s*\\($"; "") | select(. as $n | $names | index($n))] | unique)}
    | select(.function_name_references | length > 0)' > "$out/object-function-references.jsonl"
  jq -c --slurpfile functions "$out/function-map.jsonl" '
    select(.statement | startswith("DROP FUNCTION")) | . as $drop
    | (.statement | capture("DROP FUNCTION (?<name>[A-Za-z_0-9]+)\\(").name) as $name
    | {historical_drop: $drop, current_signatures: [$functions[] | select(.name == $name) | .identity]}' \
    "$out/function-history.jsonl" > "$out/retired-signatures.jsonl"
  jq -r 'select(.kind == "relation" and .definition.kind == "r" and .identity != "_sqlx_migrations")
    | "\\b(FROM|JOIN|INTO|UPDATE|TABLE)\\s+(fvoci\\.)?" + .identity + "\\b"' \
    "$out/catalog-postgres.jsonl" > "$REGEN/table-patterns.txt"
  rg --no-ignore-parent --json -n -i -U -f "$REGEN/table-patterns.txt" "${paths[@]}" \
    | jq -c -s '[.[] | select(.type == "match") | .data
        | {path: .path.text, line: .line_number, source: (.lines.text | rtrimstr("\n")), matches: [.submatches[].match.text]}]
        | sort_by(.path, .line) | .[]' > "$out/direct-table-occurrences.jsonl"
)
{
  printf '%s\n' '# 테이블 RLS와 ACL' '' \
    '기준과 열·제약·인덱스 상세는 README.md와 catalog-postgres.jsonl을 따른다. 모두 소유자는 fvoci_migrator다.' \
    '아래는 테이블 ACL이다. 열 단위 권한은 별도 column 항목에 있다. ACL 문자는 PostgreSQL 원문을 보존했다.' '' \
    '| 테이블 | RLS | FORCE RLS | 테이블 ACL |' '| --- | --- | --- | --- |'
  jq -s -r '[.[] | select(.kind == "relation" and .definition.kind == "r" and .identity != "_sqlx_migrations")]
    | sort_by(.identity) | .[]
    | "| `" + .identity + "` | " + (if .definition.rls then "켜짐" else "꺼짐" end) + " | "
      + (if .definition.force_rls then "켜짐" else "꺼짐" end) + " | `" + (.definition.acl | join("`, `")) + "` |"' \
    "$REGEN/inventory/catalog-postgres.jsonl"
} | pnpm exec prettier --stdin-filepath "$INV/tables.md" > "$REGEN/inventory/tables.md"

# 5. 비교. 반영 전 차이를 검토한다.
cmp "$REGEN/0001_final_state.sql" preparation/sql-consolidation/candidate/0001_final_state.sql || true
for file in "$REGEN"/inventory/*; do cmp "$file" "$INV/${file##*/}" || true; done
sha256sum "$REGEN/0001_final_state.sql" "$REGEN"/inventory/*
```

반영은 검토 뒤 별도로 한다: `cp "$REGEN/0001_final_state.sql" preparation/sql-consolidation/candidate/`,
`cp "$REGEN"/inventory/* "$INV"/`. 그다음 수치가 들어 있는 문서(이 README의 객체 수와 파일 설명의 건수, `tables.md`의 테이블 수,
`../analysis-a.md`, `../candidate/README.md`의 준비 기준)를 새 목록에서 다시 세어 고친다. 이 문서 갱신은 자동화하지 않았다.
후보가 바뀌면 이전 A/B 결과를 최신 검증으로 재사용하지 않고 비교기를 새 출력 경로로 다시 실행한다.

재현 확인(2026-10-23): 위 블록을 README에서 그대로 뽑아 `SRC_SHA=cb65cc31156c0fd8d28cf856bc132d4a506c8885`로 실행했다.
새 PostgreSQL 18.3 DB에서 만든 후보(SHA-256 `b8172cfb…55725`), JSON Lines 8개, `tables.md`가 모두 커밋된 파일과
**바이트 단위로 같았다**. `dependencies-postgres.jsonl`도 포함된다. 카탈로그 원본 행은 B가 보존한 최초 추출과 같았고 초기 행 파일은 비어 있었다.
출력은 로컬 `target/sql-regen-cb65cc31156c-20261023t033728z/`에 있다. 컨테이너는 정지 상태이며 볼륨은 없다.

정책 반영 재생성(2026-10-23, 최종): 같은 블록을 README에서 그대로 뽑아 `SRC_SHA=3db81d1deae24b16478581ebd6a06de2b2a2d8c9`로 실행했다(종료 코드 0).
이 SHA는 #92에 최종 `fvoci/sql-policy` `b01d74a`를 병합한 커밋이다. 97개 마이그레이션, PostgreSQL 18.3, 초기 행 파일은 비어 있었고 금지 줄 검사도 통과했다.
위 반영 절차대로 후보와 JSON Lines 8개를 복사했다. 후보 SHA-256은 `24a0a3dfaf7b2cf09152a11f80ff2100dd6004990b31cb661bf52a688ae9eea0`이고,
`tables.md`는 테이블·RLS·ACL이 같아 바이트 단위로 같았다. 출력은 로컬 `target/sql-regen-3db81d1deae2-20261023t065319z/`이며 그 컨테이너는 정지 상태이고 마운트·볼륨이 없다.

그 전의 시험 실행(dry run): 정책 브랜치 `874d866`을 병합한 `5c26622`로 같은 블록을 실행했다(후보 `bb9f1063…cf94e`, 출력
`target/sql-regen-5c266229b08e-20261023t055712z/`). 이후 #103이 0096·0097의 판 비교를 `IS DISTINCT FROM`으로 바꿔 최종 결과와는
`app_update_view`·`app_update_item`·`app_set_item_profile` 본문, 그리고 #103이 더한 시험·개발 seed·문서의 등장(623→633, 직접 테이블 문맥 352→354)만 다르다. 이 시험 실행의 후보는 증거로 쓰지 않는다.
비결정적 출력은 없었다. `cb65cc3`는 두 번 실행해 후보·목록이 바이트 단위로 같았고, 이후 `5c26622`·`3db81d1`은 각 한 번 실행했다. 덤프의 `\restrict` 토큰은 후보에 들어가기 전에 지워지고 날짜·시각을 담는 출력 파일은 없다.
