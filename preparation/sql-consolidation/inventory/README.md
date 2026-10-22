# 객체와 호출 근거

기준 커밋은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`다. 아래 수치는 그 커밋의 93개 마이그레이션을
PostgreSQL 18.3의 빈 DB에 적용한 직후다. 최종 통합 기준선 승인과 구분한다.
Worker B가 소유한 `sqlprep-b-cb65cc31-postgres-1`을 사용했다. A는 DB를 바꾸지 않았고 의존성 쿼리만 `BEGIN READ ONLY`로 실행했다.

## 읽는 순서

| 파일                                              | 내용과 한계                                                                                                                                                                                                       |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `catalog-postgres.jsonl`                          | 객체별 실제 카탈로그. `kind`, `identity`, `definition`의 JSON Lines. B의 `verification/catalog.sql` 초기판으로 얻은 원본 행을 JSON으로 변환. SQLx 원장과 DB 기본 역할·확장을 포함하므로 객체 이름을 구분해서 센다 |
| `function-history.jsonl`                          | 현재 소스의 함수 CREATE 431건, CREATE OR REPLACE 189건, DROP 133건, 총 753건. 각 선언의 원본 파일·시작 줄·전체 인자 목록. OUT 인자로 반환형을 표현한 함수도 포함                                                  |
| `function-map.jsonl`                              | 최종 앱 함수 298개의 서명·소유자·SECURITY DEFINER·search_path·ACL, 마지막 선언 위치, SQL 본문의 다른 최종 함수 이름, 소스 등장 위치                                                                               |
| `retired-signatures.jsonl`                        | 133개 과거 DROP 선언과 같은 이름으로 현재 남은 서명. 같은 이름의 새 서명이 있으므로 이름 전체를 삭제하라는 목록이 아님                                                                                            |
| `source-occurrences.jsonl`                        | `crates/server/src`, `crates/server/tests`, `xtask/src`, `deploy/postgres`, `docs`의 app_* 등장 589건. 실제 호출, 문자열 Job, 문서·주석이 원문과 경로로 구분됨                                                    |
| `direct-table-occurrences.jsonl`                  | 같은 경로에서 최종 테이블에 대한 FROM/JOIN/INTO/UPDATE/TABLE 문맥 345건. 직접 조회·변경·테스트 fixture·문서의 근거이며 SQL parser 결과는 아님                                                                     |
| `object-function-references.jsonl`                | CHECK·정책·트리거에서 최종 함수 이름을 참조하는 객체 71개. 테이블 RLS와 SQL 내부 호출을 외부 Rust 호출과 함께 보기 위한 목록                                                                                      |
| `dependencies.sql`, `dependencies-postgres.jsonl` | pg_depend가 기록한 의존성 3079건. 행 타입·테이블·제약·트리거·함수 사이의 구조적 의존성. 물리 OID 대신 객체 식별자를 사용                                                                                          |
| `tables.md`                                       | 55개 테이블의 실제 RLS/강제 RLS 상태와 테이블 ACL. 열 단위 권한은 원본 카탈로그의 column 항목으로 확인                                                                                                            |

모든 JSON 목록은 객체 하나를 한 줄에 담는 JSON Lines다. `jq -s`로 읽으면 행 순서를 유지한 배열이 된다.
배열 형식에서 변환한 여섯 목록은 변환 전후 객체 키를 정렬한 배열의 SHA-256과 행 수를 대조하여 데이터가 같음을 확인했다.
초기 추출의 원본은 B worktree의 `target/sql-compare-baseline-export/`에만 보존한다.
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
| 테이블 / 실제 테이블 열                                  |                      55 / 493 |
| 함수 / SECURITY DEFINER                                  |                     298 / 277 |
| PK / UNIQUE / FK / CHECK / NOT NULL / 제약 트리거        | 54 / 30 / 138 / 144 / 382 / 1 |
| 인덱스                                                   |                           160 |
| 사용자 트리거(제약 트리거 포함)                          |                            24 |
| RLS 정책 / RLS 활성 테이블 / 강제 RLS 테이블             |                   51 / 51 / 0 |
| 명시 sequence / view / materialized view / enum / domain |             0 / 0 / 0 / 0 / 0 |
| 앱 역할 명시 기본 ACL 레코드                             |                             0 |
| 마이그레이션 직후 필수 초기 데이터 행                    |                             0 |

카탈로그 파일에는 인덱스의 pg_attribute 행도 있으므로 전체 column 행 768개를 실제 테이블 열 493개로 잘못 해석하지 않는다.
테이블마다 자동 생성되는 composite/array 타입과 SQLx 원장 타입도 type 항목에 포함되어 있다.
PK가 54개인 것은 원본 상태다. 후보에서 임의로 추가하거나 삭제하지 않는다.

## 역할·소유자·권한

- 앱 객체 소유자는 `fvoci_migrator`다. `fvoci_app`·`fvoci_migrator`는 LOGIN/INHERIT이며 이 새 PG18 DB에서
  SUPERUSER/CREATEROLE/CREATEDB/BYPASSRLS/REPLICATION은 모두 거짓이다. bootstrap 자체는 기존 migrator의 CREATEDB를 허용하므로
  이 측정값을 모든 설치에 강제하는 새 규칙으로 바꾸지 않는다.
- 두 역할의 search_path는 `fvoci`이며 다른 역할의 멤버가 아니다. DB 기본 시스템 멤버십 3건은 원본 카탈로그에 별도로 보존했다.
- 277개 SECURITY DEFINER 함수는 모두 `search_path=fvoci, pg_temp`다. PUBLIC EXECUTE는 0건이다.
  순수 도우미의 기존 PUBLIC EXECUTE는 스키마 USAGE와 별개이며 원본대로 유지한다.
- RLS가 꺼진 `document_collab_file_refs`, `oidc_login_states`, `scheduled_runs`, `sessions`에는 앱 역할의 테이블 권한이 없다.
  정의자 함수 경로가 경계를 이룬다. 모든 테이블에 FORCE RLS를 임의로 켜면 소유자 함수의 동작이 달라지므로 하지 않는다.
- column ACL도 보존한다. 예를 들어 `workspaces.name`의 UPDATE 권한을 전체 테이블 UPDATE로 넓히지 않는다.
- 명시 default ACL은 0건이다. NULL ACL은 권한 없음이 아니라 PostgreSQL 기본 ACL을 뜻하므로 카탈로그 조회가 `acldefault`로
  실효 항목을 펼쳤다. 후보에 불필요한 전역 default ACL 정책을 새로 만들지 않는다.

## DB 제공 의존성과 D-88

PG18의 `uuid_extract_version(uuid)`는 `pg_catalog`의 LANGUAGE internal 함수이며 새 UUID를 생성하지 않는다.
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
3. `verification/catalog.sql`로 별도 카탈로그를 얻고, 모든 테이블의 초기 행 수를 확인한다. 카탈로그의 비밀번호·비밀값은 수집하지 않는다.
4. `dependencies.sql`은 읽기 전용 트랜잭션으로 실행한다. 위 JSON Lines 목록과 소스의 CREATE/REPLACE/DROP 선언 및 호출 문맥을 대조한다.
5. 후보는 bootstrap이 만드는 CREATE SCHEMA와 psql 메타 명령을 빼고, 설치용 설정만 SET LOCAL로 한정한다.
   pg_dump의 함수/행 타입/테이블 순서를 보존하며, 파일 번호는 시험 후보의 실제 SQLx 버전이다.
6. B의 원본 대 원본·후보 비교와 기존 앱 역할 테스트를 완료하기 전에는 파일 수나 호출 검색 결과만으로 동일성 PASS를 선언하지 않는다.
