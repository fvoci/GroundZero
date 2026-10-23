# SQL 통합 비교 검증

고정 SHA의 누적 SQL과 현재 `crates/server/migrations/`를 각각 빈 DB에 적용한다.
기본 비교 기준은 `8ffd8c3b940924eca8cf7ec739a85494243c0479`이며 `--baseline-sha`로 전체 SHA를 명시할 수 있다.
현재 제품 초기 SQL이 최종 객체·동작을 보존하는지 확인하며 기존 DB나 Cloud에는 연결하지 않는다.
[준비 당시 결과](results.md)는 해당 SHA에 한정된 역사 기록이다.

```sh
# 비교기 대조군: 독립된 빈 클러스터 둘에 고정 SHA의 누적 마이그레이션 적용
cargo xtask sql-compare --control --output target/sql-compare-aa

# PostgreSQL 17: 과거 누적 마이그레이션 A / 현재 제품 B + 기존 앱 역할 통합 테스트
cargo xtask sql-compare \
  --output target/sql-compare-postgres --behavior

# 같은 비교 + 시험용 Keycloak으로 OIDC 시험까지 포함한 전체 행동 시험(18·Supabase에도 붙일 수 있다)
cargo xtask sql-compare --postgres-major 17 \
  --output target/sql-compare-postgres-oidc --behavior --oidc

# 고정된 셀프호스트 Supabase PostgreSQL 17: 같은 비교 + 직접 연결 및 session pooler
cargo xtask sql-compare \
  --output target/sql-compare-supabase --supabase --behavior --oidc

# 표준 PostgreSQL의 제한 설치 관리자 + 명시된 앱 역할 시험만 실행
cargo xtask sql-compare \
  --output target/sql-compare-restricted \
  --restricted-admin --behavior --behavior-scope core
```

표준 PostgreSQL의 기본값은 최소 지원 메이저 17이다. `--postgres-major 18`을 명시하면 기존 18 이미지로 같은 비교를 실행한다. 두 값 외에는 받지 않으며 Supabase는 자체 고정 이미지를 사용한다. 선택한 메이저와 Compose 파일 해시는 `inputs.json`에 기록한다. 기본 통합 CI는 17, 기존 OIDC·S3 CI는 `compose.test.pg18.yaml`을 겹쳐 18을 검증한다. 개발·셀프호스트 볼륨이 있는 구성은 기존 18을 유지한다.

출력 디렉터리는 새 경로여야 한다. 부모 디렉터리는 필요하면 만들고 출력 경로 자체는 원자적으로 생성해 같은 `--output`을 쓰는 동시 실행 중 하나만 허용한다. 기존 출력은 덮어쓰지 않는다. 양쪽 모두 제품 `db bootstrap`을 실행한다. A는 고정 SHA의 SQL만 `previous-migrations/`로 꺼내 SQLx `Migrator`로 적용하고, B는 현재 제품 `db migrate`를 실행한다. B에 누적 이력을 복사하거나 체크섬 검사를 우회하지 않는다. `--control`이면 B에도 A와 같은 과거 SQL을 적용한다. 제품 SQL은 SQLx가 같은 트랜잭션에서 `_sqlx_migrations`를 기록할 수 있게 마지막에 `SET LOCAL search_path = fvoci`를 둔다.

Compose 파일·이미지·테스트 자격 증명은 기존 설정을 재사용한다. 프로젝트 이름은 체크아웃 해시와 실행 UUID를 포함하며 A/B/검색·S3가 서로 다르다. 호스트 포트는 동적 루프백 포트다. 종료 시 생성한 프로젝트만 `stop`한다. 시작 전에 정지 대상을 등록해 부분 시작 실패도 처리하며, 한 정지가 실패해도 나머지 정지와 `result.json` 기록을 시도한다. 정지 실패는 `cleanup: FAIL`과 `passed: false`로 남긴다. 컨테이너·이름 붙은 볼륨은 보존하고 `down`·`prune`을 호출하지 않는다. 기존 Compose DB는 tmpfs이므로 **컨테이너를 정지하면 DB의 tmpfs 데이터 자체는 사라진다**. 그 전에 카탈로그·덤프·로그를 출력 경로에 저장한다. 개인 개발 DB 또는 Cloud DB 주소는 받지 않는다.

과거 실행에는 예외가 있다. 2026-10-22 첫 A/A 진단에서 수정 전 바이너리가 해당 실행 B 프로젝트에 `down --volumes --remove-orphans`를 호출해 전용 PostgreSQL 컨테이너와 네트워크를 삭제했다. 이 DB는 tmpfs였고 이름 붙은 볼륨은 없었다. 공유 컨테이너·볼륨은 대상이 아니었다. 이후 실행은 위의 `stop` 방식으로 수정했다. 정확한 명령과 삭제 로그는 [원본 진단 로그](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/diagnostics/initial-control-cleanup.txt), 구성 증거는 [저장소 구성](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/diagnostics/initial-control-postgres-storage.json), 실행별 처리는 [결과 보고서](results.md)에 남겼다.

## 객체 비교

행동 테스트가 fixture를 만들기 전에 양쪽 카탈로그·초기 행·덤프를 읽는다. `catalog.sql`이 다음을 이름 기반 키로 비교한다.

- 앱 함수 전체 정의, 언어, 소유자, 실제 ACL, SECURITY DEFINER, 설정(search_path 포함), volatility/strict/parallel/leakproof/cost/rows.
- 테이블·인덱스·시퀀스·타입 소유자와 ACL, RLS/FORCE RLS, 컬럼 타입·기본값·generated/identity·collation·column ACL, 제약·인덱스 식과 상태.
- RLS USING/WITH CHECK·대상 역할·명령·permissive/restrictive, 일반 트리거와 내부 FK 트리거의 enabled/deferrable/deferred 상태.
- 역할 속성(비밀번호 제외), 멤버십 grantor/admin/inherit/set, 기본 ACL, DB 소유자·ACL·tablespace 이름과 설정, 확장 버전·스키마·소유자.
- 관리 스키마를 포함한 namespace·relation 소유자/ACL/속성, function 정의/소유자/ACL과 event trigger. 관리 스키마의 relation 컬럼·제약·정책 등 전체 DDL을 비교한다는 뜻은 아니다. `uuid_extract_version` 제공자 함수도 포함한다.
- 모든 앱 테이블의 초기 행(빈 테이블 포함), 시퀀스 현재 값/is_called. 초기 행의 시각·ID를 임의로 지우지 않는다.

`pg_dump --schema-only --schema=fvoci --no-comments`를 같은 서버 이미지의 도구로 만들고 객체 블록별로 비교한다. 함수의 실행 내용과 권한 텍스트를 비교한다. SQLx 이력 테이블 **구조도 비교한다**. 그 테이블 행은 별도 `migration-ledger.json`에 기록한다. 제품 기준의 진짜 적용 버전/체크섬과 누적 이력은 의도적으로 다르며 동등하다고 주장하지 않는다. 이력의 실행 시각·소요 시간만 별도 기록에서 제외한다. 앱이 이력 조회 권한 없이 세대를 구별하는 원장 COMMENT도 별도 기록한다.

정규화 범위는 명시적이다. OID 참조를 이름으로 바꾸며 DB 이름은 `current` 키로 비교한다. 시스템 transaction horizon과 물리 tablespace OID는 비교하지 않고 후자는 이름으로 대신한다. 덤프의 무작위 `\restrict` 토큰·도구 버전 헤더·객체 순서를 제외한다. 객체 COMMENT는 동작 비교 대상이 아니다. 정지 전에 뽑은 원본 덤프도 남긴다.

이번 주석 정리로 함수 본문의 전체 줄 `--` 주석이 달라진다. `fvoci` 함수에 한해서 문자열·식별자·중첩 달러 인용·블록 주석 밖의 전체 줄 주석만 비교 시 제거한다.
실행 바이트·인라인 주석·공백·함수 속성·ACL은 그대로 비교한다. 원본 정의도 보존하고 `comment-only-differences.json`에 그 차이를 별도로 기록한다.
문자열 안의 `--`를 데이터로 유지하고 실행 내용·속성 변경을 검출하는 단위 시험을 둔다.
과거 A에 새 `db migrate`를 실행하면 재생성 필요 오류가 나야 하며, 거부 전후 카탈로그·초기 행·원장·세대 표시의 무변경도 확인한다.

한 가지 PostgreSQL 재파싱 차이는 `files_name_check`에서 관찰됐다.

```sql
-- 누적 DDL의 덤프
CHECK (((A AND B) AND C))
-- 그 덤프를 다시 적용한 뒤의 덤프
CHECK ((A AND B AND C))
```

여기서 A/B/C는 각각 파일명 길이 하한·상한·금지 문자 검사다. `pg_get_constraintdef(oid, true)`의 결과는 같다. 덤프의 정확한 TABLE 블록 안에서 정확한 이름으로 찾은 단일 행 CHECK만 그 카탈로그 표현으로 대체한다. 다른 괄호·함수 본문·ACL은 정규화하지 않는다. 제약의 의미를 바꾸면 카탈로그와 이 정규 덤프가 달라진다.

각 실행은 B에 정책 추가, 세션 테이블 DELETE 권한 부여, `app_user_id()` 본문 대체, 파일명 CHECK를 `true`로 완화하는 네 변조를 **하나씩** 트랜잭션 안에서 넣는다. 해당 객체 종류에서 차이가 검출돼야 하며, ROLLBACK 뒤 전체 스냅숏이 원래와 같아야 한다. 결과는 `self-test.json`에 남긴다.

## 행동 비교와 한계

`--behavior`는 기존 workspace Rust 시험을 실제 `fvoci_app` 접속으로 양쪽에서 실행한다. A의 `fresh_database` 공통 헬퍼도 같은 고정 SHA의 SQLx 디렉터리로 DB를 준비한다. 별도 API/권한 구현이나 축약한 모의 SQL 시험으로 대체하지 않는다. CRUD·테넌트 격리·개인/공유 문서·철회·경합/409·반복·시간 기록·검색·백업 등의 기존 검증을 포함한다.

행동 시험의 일반 DB·검색·S3 환경 변수도 격리된 서비스에 고정한다. 기본 파일 저장소는 local이며 S3 시험만 기존 헬퍼로 시험 SeaweedFS를 선택한다. 외부 환경에서 설정한 저장소·검색 URL이 시험에 새어 들어가지 않는다. Zotero 기본 URL은 닫힌 루프백 포트이고 해당 시험이 자체 HTTP fixture로 덮어쓴다.

Supabase에서는 각 DB의 직접 연결과 session pooler를 실행한다. 일반 앱 연결과 `db_context`는 실제 pooler를 사용한다. **새 DB를 만드는 백업·검색 시험은 기존 규칙대로 직접 연결을 사용한다**. 따라서 session 단계 통과를 모든 시험의 pooler 통과로 해석하지 않는다. OIDC Keycloak 시험은 `--oidc`를 붙인 실행에서만 돈다. `cargo xtask test --oidc`처럼 기존 `oidc` 프로필의 시험용 Keycloak을 이 실행의 검색·S3 프로젝트에 함께 띄우고 `FVOCI_TEST_OIDC_ISSUER`를 넣어 `--skip oidc_` 없이 전체 시험을 실행한다. A와 B는 같은 Keycloak을 차례로 쓴다(기존 Supabase direct→session 실행과 같은 방식). `--oidc`는 `--behavior`가 필요하고 core 범위·제한 설치 관리자와는 함께 쓸 수 없다. 붙이지 않은 실행의 OIDC는 NOT RUN이며, PostgreSQL 실행의 Supabase 전용 시험도 NOT RUN이다. S3는 기존 SeaweedFS 시험 서비스를 사용한다.

`inputs.json`은 실행 소스·과거 기준 SHA, 양쪽 SQL SHA-256, 비교기와 카탈로그 해시·Supabase 고정 revision을 기록한다. `result.json`은 설치·스냅숏·객체 비교·변조 시험과 각 행동 경로의 PASS/FAIL/NOT RUN을 분리한다. 두 파일의 `oidc`는 OIDC 시험 요청 여부이며, 행동 경로의 PASS 문구에도 OIDC 실행 여부를 적는다. `a/`, `b/`의 원본 덤프·카탈로그·이력·행동 로그가 근거다. 실패한 실행도 별도 출력 경로로 보존한다.

새 설치 동등성은 기존 백업의 새 버전 복구나 Supabase Cloud 실행을 증명하지 않는다. 누적 마지막 버전 98과 새 기준 202610230001은 다른 이력이다. 기존 개발·시험 DB는 승인 후 재생성하며 자동 이전·이력 교체 경로는 없다(D-107). 구버전 백업 복구와 실제 Cloud 검증은 별도 실행 결과가 없으면 NOT RUN이다. [결과 보고서](results.md)는 준비 당시 결과이고, 이번 통합 결과는 별도 PR과 고정 SHA 증거에 남긴다.

## 제한 설치 관리자 프로필

`--restricted-admin`은 표준 PostgreSQL의 전용 로컬 fixture에만 적용되며 `--supabase`와 함께 쓸 수 없다. 처음 한 번 fixture superuser가 설치 관리자 역할 둘을 만들고 대상 DB의 `CREATE`와 `CONNECT WITH GRANT OPTION`만 부여한다. DB 소유자는 바꾸지 않는다. 이후 제품 bootstrap·재실행은 제한 관리자, 과거/제품 SQLx 적용과 객체 조회는 `fvoci_migrator`, 앱 요청은 `fvoci_app`으로 수행한다. Supabase 관리 역할·스키마·서비스를 만들거나 흉내 내지 않는다.

프로필 `restricted-installer-v1`의 설치 관리자는 LOGIN·CREATEROLE만 가지며 SUPERUSER·CREATEDB·BYPASSRLS·REPLICATION은 없다. `createrole_self_grant=''`로 새 제품 역할의 자동 ADMIN OPTION만 받고 SET/INHERIT는 받지 않게 한다. 두 제품 역할의 특권 속성도 실제로 확인한다. `capabilities.json`에 DB 소유자·DB 권한·역할 속성, 반복 bootstrap 뒤 새 연결에서 확인한 정확한 멤버십, SQLx 재실행 전후 카탈로그·이력 동일성을 기록한다.

다른 설치 관리자가 기존 역할의 비밀번호 변경을 요청하면 ADMIN OPTION 부족으로 거절돼야 한다. 요청에는 기존과 다른 고정 시험 비밀번호를 쓰고, 거절 뒤 원래 자격 증명으로 새 연결이 열리는지 확인한다. SUPERUSER 승격·CREATE DATABASE·SET ROLE·역할/스키마 생성·RLS 해제 등의 실제 금지 명령은 기대 SQLSTATE `42501`과 관찰 결과를 각각 남긴다. 이 거부 성공은 일반 기능의 PASS와 구분한다.

`--behavior-scope core`는 기존 `db_context`, `first_flow`, `ids`, `sharing`, `members`, `recurrence`, `time_tracking`만 실행한다. 전체 시험의 대체물이 아니며 미선택 바이너리, 백업/복구·FreshDb 검색·OIDC·S3·Supabase 전용 경로는 NOT RUN이다. 이 프로필에는 `session_replication_role` SET 권한이 없어 현재 복구 요구를 충족하지 못한다. 이는 **해당 로컬 프로필의 한계**다. 이전 셀프호스트 Supabase 복구 시험은 통과했고, 실제 Cloud에서 이 설정을 변경하는 시험은 실행하지 않았다.
