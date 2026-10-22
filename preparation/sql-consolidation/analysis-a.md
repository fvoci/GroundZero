# SQL 통합 준비 A: 최종 상태 후보와 제거 근거

상태: 실행 가능한 시험 후보 작성 및 기준 SHA의 새 설치 검증 완료. [B의 실행 결과](verification/results.md)를 함께 본다.
작업 기준은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`이며 최종 기준선 확정이 아니다.
2026-10-22 확인 시 #91은 병합되었고 #69는 Draft다. 후속 변경의 포함·종결 여부는 조정자가 확인한다.
실제 `crates/server/migrations/`, 설치 명령, 공유 결정·개발·감사 문서는 변경하지 않는다.

## 후보의 경계

- 기존 `db bootstrap`이 역할과 `fvoci` 스키마를 준비한 **빈 DB**에 적용한다. 기존 DB의 업그레이드 SQL이 아니다.
- 0001–0094 중 실제 존재하는 93개 이력 파일을 적용한 카탈로그에서 마지막 정의를 추출한다. 이력 연결이나 번호만 바꾸는 작업이 아니다.
- 기본 테이블 정의에 최종 열·기본값·CHECK를 직접 포함하고, 함수마다 최종 서명·본문을 한 번만 만든다. PK·UNIQUE·FK·인덱스·트리거·RLS·ACL은 최종 상태를 적용한다.
- SQLx 원장은 통합 후보 자체를 실제 SQLx migrator로 적용하여 만든다. 과거 93개 성공 행을 가장하지 않는다.
- 내부 호출·트리거·정책 의존성이 있는 함수는 Rust에서 직접 부르지 않아도 유지한다. 이름이나 외부 호출 수만으로 제거하지 않는다.
- P-24/25/26/28/29/30/31/35는 미승인 정책 제안이다. 현재 의미를 그대로 보존하며 통합을 근거로 구현하지 않는다.

## 설치와 지원 환경

`crates/server/src/db/admin.rs`의 `bootstrap`, `prepare_database`, `borrow_migrator`, `release_migrator`는 변경하지 않는다.
이들은 비밀번호·관리자 ADMIN OPTION·일시 SET 멤버십 복구까지 다루므로 고정 역할 DDL로 대체할 수 없다.
앱은 `fvoci_app`, 소유자는 `fvoci_migrator`, 요청 사용자는 트랜잭션의 `app.user_id`다. 역할 기본 search_path는 `fvoci`다.
현재 DB 이름에 CONNECT를 주며, 관리자에게 원래 있던 멤버십과 옵션을 보존한다.

D-88의 PostgreSQL 18 / 셀프호스트 Supabase PostgreSQL 17 직접 연결·session pooler는 지원 환경이다.
D-92의 과거 개발 버전 호환 제거와 구분한다. `auth.uid()`, Supabase Auth·Storage·Realtime·Data API 또는 UUID 생성 fallback을 도입하지 않는다.
`uuid_extract_version(uuid)`의 실제 스키마·소유자·ACL·확장 의존성은 각 환경 카탈로그로 확인한다.
이는 버전을 검사하는 의존성이지 UUID를 만드는 대체 함수가 아니다. 제공자 소유 시스템 객체를 앱 마이그레이션으로 덮어쓰지 않는다.

설치 문서의 `db bootstrap` → `db migrate`, xtask의 같은 명령 호출, 백업/복구의 원장 확인이 유지되어야 한다.
준비 후보는 운영 설치 경로에 연결하지 않는다. 실제 전환 시에는 최종 기준선, 새 SQLx 버전 정책, 기존 개발 DB 처리와 문서 갱신이 별도 구현 범위다.

## 공통 SQL과 제한된 설치 권한

2026-10-22 추가 확인: 현재 후보를 PostgreSQL과 Supabase용으로 나눌 근거는 없다.
후보의 앱 함수 298개는 SQL 93개와 PL/pgSQL 205개이고, 모두 제품 스키마 `fvoci`에 속한다.
새 확장·역할·DB·이벤트 트리거 생성, 시스템 객체 변경, LANGUAGE internal/C 함수 생성, LEAKPROOF 지정이 없다.
Supabase 역할·`auth`/`storage`/`extensions` 스키마·서비스 API를 참조하지 않는다.
동일한 후보 SQL을 사용하고, DB 주소·TLS·직접/session 연결과 초기 관리자 권한을 환경 설정으로 둔다.

| 단계                  | 현재 코드·후보에 필요한 권한과 전제                                                                                                                                                                                           |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 설치 관리자 연결      | 대상 DB의 CONNECT·CREATE. bootstrap이 두 제품 역할에 CONNECT를 부여하므로 CONNECT 위임 권한도 준비한다. DB 소유권은 이를 충족하는 한 방법이며 필수 조건은 아니다.                                                             |
| 역할 준비             | 새 역할을 만드는 CREATEROLE. 이미 존재하는 제품 역할에는 ADMIN OPTION도 있어야 한다. 새로 만든 역할의 ADMIN OPTION으로 migrator에 대한 SET을 잠시 부여하고 원래 옵션으로 복구한다.                                            |
| `fvoci` 스키마 준비   | 관리자에게 DB CREATE와 migrator로 SET할 권한이 필요하다. 스키마 소유자인 migrator 자체에는 DB CREATE가 필요하지 않다. 기존 스키마의 소유자는 migrator여야 한다.                                                               |
| 후보 적용             | `fvoci_migrator`로 접속하며 `fvoci` 스키마를 소유한다. SQL/PL/pgSQL 언어와 인자·반환 타입의 USAGE, 사용하는 PostgreSQL 기본 함수의 EXECUTE가 필요하다. 통상 엔진 기본 권한으로 제공되는 항목도 실제 환경에서 유지되어야 한다. |
| 소유자·ACL·RLS·트리거 | 후보에서 생성한 객체는 migrator 소유다. 같은 소유자로 명시한 ALTER OWNER와 자기 객체에 대한 GRANT/REVOKE, RLS·정책·트리거 설정은 그 소유권으로 실행한다. 앱에는 최종 카탈로그에 있는 권한만 부여한다.                         |
| 앱 실행               | `fvoci_app`의 LOGIN·CONNECT, 후보의 스키마/객체 권한과 트랜잭션별 `app.user_id`를 사용한다. 앱은 테이블 소유자가 아니며 BYPASSRLS가 없다.                                                                                     |

위 요구는 [현재 bootstrap](../../crates/server/src/db/admin.rs),
[PostgreSQL 역할 속성](https://www.postgresql.org/docs/17/role-attributes.html),
[함수 생성 권한](https://www.postgresql.org/docs/17/sql-createfunction.html)을 대조한 것이다.
PostgreSQL 17.0의 [스키마 생성 구현](https://github.com/postgres/postgres/blob/REL_17_0/src/backend/commands/schemacmds.c#L84)은
DB CREATE를 호출 관리자에게 검사한다. [동일 소유자의 ALTER SCHEMA](https://github.com/postgres/postgres/blob/REL_17_0/src/backend/commands/schemacmds.c#L344)는
소유권 변경 없이 성공하므로 후보 첫 문장을 위해 migrator에 DB CREATE를 추가할 필요가 없다.

277개 SECURITY DEFINER 함수는 스키마/테이블 소유자인 migrator의 객체 권한으로 실행한다.
SUPERUSER나 BYPASSRLS 속성을 부여해서 실행하는 구조가 아니다. 현재 FORCE RLS가 0개인 상태와
각 함수의 `search_path=fvoci, pg_temp`, PUBLIC EXECUTE 회수는 그대로 보존한다.
기본 ACL을 새로 변경하거나 앱 역할에 스키마 CREATE를 주지 않는다.

### UUID 검사 함수의 출처

`pg_catalog.uuid_extract_version(uuid)`는 **PostgreSQL 17.0부터 있는 엔진 기본 함수**다.
[17 릴리스 노트](https://www.postgresql.org/docs/17/release-17.html)와 최초 17.0 태그의
[카탈로그 선언](https://github.com/postgres/postgres/blob/REL_17_0/src/include/catalog/pg_proc.dat#L8847),
[함수 구현](https://github.com/postgres/postgres/blob/REL_17_0/src/backend/utils/adt/uuid.c#L445)에서 확인했다.
따라서 Supabase PG17에 이 함수가 있는 것을 별도 backport나 Supabase 확장 의존성으로 해석하지 않는다.
이 함수는 UUID의 버전을 읽으며, 앱이 전달한 UUIDv7도 검사할 수 있다. DB에서 UUIDv7을 생성하는 기능의 버전 조건과 구분한다.

로컬 PG18.3과 셀프호스트 Supabase PG17.6 카탈로그에서 모두 반환형 smallint,
LANGUAGE internal, IMMUTABLE/PARALLEL SAFE/STRICT/LEAKPROOF인 기본 함수를 확인했다.
소유자는 각각 초기 클러스터 역할 `fvoci`와 `supabase_admin`이지만 앱이 그 역할의 권한을 빌리지는 않는다.
PUBLIC EXECUTE가 있고 SECURITY DEFINER가 아니다. 후보는 이 함수의 생성·교체·소유자 변경·ACL 변경을 하지 않는다.
제품 ID는 계속 앱이 공급하며 UUID 생성 fallback을 추가하지 않는다.

### 실제 검증 범위와 남는 조건

B의 기존 PostgreSQL 18.3 / 셀프호스트 Supabase 17.6 비교는 동일 후보를 일반 migrator로 적용해
객체·덤프 동일성을 확인했다. 별도로 제한 설치관리자에 LOGIN·CREATEROLE, 대상 DB CREATE·CONNECT 위임만 주고,
DB 소유권·전역 역할 멤버십·SUPERUSER·CREATEDB·BYPASSRLS·복제 설정 권한 없이 bootstrap부터 검증한다.
`createrole_self_grant=''`로 시작하여 bootstrap의 임시 SET 부여·복구도 실행되도록 한다.
이 검증의 실제 PASS/FAIL과 동작 시험 범위는 [B 실행 결과](verification/results.md)에 기록한다.

Cloud의 기존 원격 실행 근거는 TLS/session 연결과 SELECT 1까지다.
실제 프로젝트의 엔진 버전·역할 속성·ADMIN OPTION·DB 권한·기본 함수 ACL은 그 실행으로 확인되지 않았다.
Cloud PG17 이상과 위 권한·접속 조건이 충족되면 동일 SQL을 사용하는 설계이며,
로컬 제한 권한 시험을 Cloud 프로젝트의 설치·동작 실측으로 표시하지 않는다. 이번 작업에서 Cloud DB를 변경하지 않는다.

복구는 별도 제약이 있다. [현재 복구 코드](../../crates/server/src/backup.rs)는
`session_replication_role=replica` 설정 권한을 요구하며, 후보 SQL이나 일반 앱 실행에는 그 권한이 필요하지 않다.
제한 관리자에서 이 권한이 없으면 현재 복구 경로까지 공통 동작한다고 선언할 수 없다.
Supabase의 별도 허용 동작에 의존하지 않는 복구 설계는 이 준비 후보만으로 해결된 사항이 아니다.

## 이미 대체된 정의

정확한 CREATE/REPLACE/DROP 위치와 인자 전체는 `inventory/function-history.jsonl`에 있다. 일부 같은 이름은 살아 있는 다른 서명이 있으므로 이름 전체를 제거해서는 안 된다.

| 이력             | 최종 후보에서 재생하지 않을 것                                                   | 유지하는 의미                                                                      |
| ---------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| 0002             | 첫 OIDC 상태 테이블을 만들고 지우는 단계                                         | 쿠키 비밀값의 해시로 찾고 별도 state를 비교하는 최종 테이블·함수                   |
| 0003–0069의 교체 | 앞선 함수 본문, 교체 전 반환형·서명·정책·트리거                                  | 마지막 카탈로그 정의. 아래 함수 지도에서 최종 위치 추적                            |
| 0006             | `app_next_wiki_number(uuid)`와 문서만 있던 번호 대장                             | 공통 `app_allocate_number`와 `numbered_targets`, 번호 비재사용                     |
| 0034             | `comments.workspace_id`와 저장된 알림 excerpt                                    | 대상에서 워크스페이스·권한을 판단하고 현재 댓글로 excerpt 계산                     |
| 0070–0075        | 0075가 DROP한 ID 목록 없는 옛 서명 102개, ID `DROP DEFAULT` 반복                 | 새 행 UUIDv7은 앱이 명시 제공. 최종 27개 ID CHECK, 부족 `FV001`, 잘못된 값 `FV002` |
| 0071             | 문서 제목 변경 감사 트리거와 중간 정의                                           | 명시 `app_audit_document_rename`, 리비전별 한 번의 감사 인덱스                     |
| 0076             | 인자 3개 `app_move_task`                                                         | 조건 인자를 가진 최종 함수와 이동 판 트리거. 현재 무조건 첫 이동도 지원            |
| 0082             | `app_add_task_dependency(uuid,uuid,uuid[])`, base 없는 `app_set_task_dependency` | 최신 관계 base 비교·승인 목록 검사·잠금과 현재 생성 동작                           |
| 0086             | `app_set_event_recurrence(uuid,jsonb,uuid[])`                                    | 규칙 base를 받는 최종 서명과 충돌 처리                                             |
| 0087             | `app_set_time_zone(text)`                                                        | 언어·시간대·주 시작·시각 표시를 함께 저장하는 `app_set_display_settings`           |
| 0094             | editors 없는 `app_system_collab_store`, 예전 반환형 `app_collab_load`            | edited_by/editors 열과 실제 편집자 기록. 이전 저장 대기 상태는 빈 목록 의미 유지   |

## 데이터 보정과 새 설치 불변식

다음 SQL은 과거 행을 다음 구조로 맞추는 단계다. 빈 DB의 동일한 최종 상태를 만드는 데 불필요하더라도, 그 보정의 **의미**까지 제거해서는 안 된다.
기존 DB에 후보를 직접 적용하거나 기존 원장을 지우는 것은 허용하지 않는다.

| 과거 보정                                            | 빈 DB에서의 처리와 유지 근거                                                                                                  |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 0003: `workspaces.owner_id = created_by`             | 최종 owner_id NOT NULL/FK와 현재 생성 함수의 소유자 지정. 처음부터 행이 없어 UPDATE 불필요                                    |
| 0006: 기존 위키를 numbered_targets로 INSERT          | 현재 공통 번호 할당 함수가 새 행마다 카운터와 대장을 함께 기록                                                                |
| 0010: 기존 문서 위치를 자리별로 다시 매김            | 최종 position 기본값·삽입 트리거·자리 잠금·형제 순서 함수 유지                                                                |
| 0016: task_series.effective_from을 anchor로 채움     | 최종 NOT NULL 열과 현재 규칙 생성·변경 함수가 유효 시작일 제공                                                                |
| 0022: 예전 방식으로 지운 일정을 휴지통 묶음으로 옮김 | 빈 DB에 옛 삭제 행이 없음. 현재 휴지통 함수는 앱 ID를 받아 묶음 생성. 옛 DO 블록은 ID 없는 INSERT이므로 새 후보에 남기지 않음 |
| 0034: 알림의 옛 excerpt 삭제                         | 현재 댓글 내용을 읽는 최종 알림 조회와 생성 함수 유지                                                                         |
| 0034: 허용되지 않는 강좌/회차 URL을 빈 문자열로 바꿈 | 최종 `app_web_url_ok` CHECK와 생성·수정 검증 유지                                                                             |
| 0045: 이미 탈퇴한 사용자 item_members 삭제           | 최종 멤버 제거 함수의 관련 행 정리·잠금 유지. 초기 행 없음                                                                    |
| 0045: 기존 문서 리비전의 파일 참조 INSERT            | 최종 리비전 INSERT 트리거 및 참조 고정·버전 잠금 유지                                                                         |
| 0075: UUID id 기본값 DROP, UUIDv7 CHECK 추가         | 테이블 정의부터 UUID 기본값 없음·CHECK 있음. DB 생성이나 다른 버전 UUID 대체 없음                                             |
| 0077: 기존 관계 kind/lag를 FS/0로                    | 최종 기본값 FS/0와 허용 값 CHECK 유지. 관계는 복합 키로 새 UUID 불필요                                                        |
| 0079: 겹친 칸반 열 위치를 다시 매김                  | 최종 초기화 복제의 `app_renumber_lane` 호출, 순서·move_version 트리거 유지                                                    |
| 0091: 기존 항목 검색 대기열 갱신                     | 최종 items_search가 updated_at 변경도 감지. 빈 DB의 초기 검색 대기열은 비어 있음                                              |
| 0094: 추가된 editors 열의 기본 빈 배열               | NULL/빈 배열/실제 편집자 목록의 구분을 최종 열 기본값과 함수에 보존                                                           |

0003, 0004, 0006의 DO 블록 중 함수 ACL을 설정하는 블록은 데이터 보정이 아니다. 카탈로그의 **실효 ACL**로 펼쳐 적용한다.
마이그레이션 직후 전체 테이블 행 수도 비교하여 필수 초기 행을 놓치지 않았는지 확인한다. 개발 예제는 운영 필수 초기 데이터가 아니다.

## 호출 경로와 검증 경계

`inventory/source-occurrences.jsonl`은 서버·worker·테스트·xtask·개발 seed·문서의 함수 이름 등장 위치와 원문을 보존한다.
문서·주석과 실제 호출을 같은 의미로 세지 않는다. SQL 함수 내부 호출 및 정책·트리거 경로는 카탈로그에서 별도로 대조한다.
UUID 목록 개수와 호출 순서는 `docs/sql-id-contract.md`의 `new_ids` / `needed_ids` 계약을 유지한다.
worker는 상한 조회부터 트랜잭션 전체를 최대 세 번 재시도하며, HTTP의 FV001은 409다.
`xtask/src/dev.rs`는 각 seed 파일에 UUIDv7 1000개를 공급하고 `pg_temp.take`가 소비한다. 임시 함수는 영구 앱 스키마에 옮기지 않는다.

검증 담당 B가 원본 대 원본(A/A), 원본 대 후보(A/B), 실제 SQLx 적용, 기존 앱 역할 통합 테스트 및 지원 환경별 확인을 수행한다.
이 문서는 준비 범위의 검토 기록이며, 테스트가 끝나기 전 동일성 PASS나 최종 전환 완료를 선언하지 않는다.

## A의 확인 결과

- PASS: 현재 원본 93개 이력으로부터 최종 객체를 추출한 실행 SQL 작성. 후보는 `candidate/0001_final_state.sql` 한 개이며
  실제 마이그레이션 디렉터리에 연결하지 않았다. 원본 pg_dump의 함수/행 타입 의존 순서를 보존했다.
- PASS: 함수 이력 753건(생성 431, 교체 189, 삭제 133), 최종 함수 298개의 마지막 선언 위치 연결. 현재 이름별 오버로드 중복 0건.
- PASS: 앱 테이블 55개·열 493개, 함수 298개, 인덱스 160개, 사용자 트리거 24개, 정책 51개를 카탈로그로 열거했다.
  UUID 열 기본값 0건, SECURITY DEFINER 277개 모두 고정 search_path, PUBLIC EXECUTE 0건이다.
- PASS: 원본의 초기 데이터 0행을 B가 확인했다. 별도 초기 seed SQL을 후보에 추가할 필요가 없다.
- PASS: 구조적 의존성 3079건을 읽기 전용으로 수집했다. 함수·정책·트리거의 본문 이름 참조와 외부 코드/문서 위치를 별도 보관했다.
- FAIL → 수정: 첫 B 설치는 `_sqlx_migrations`를 찾지 못했다. 고정 의존성 SQLx 0.8.6의 `execute_migration`은 SQL 본문 뒤
  커밋 전에 스키마 없는 원장 INSERT를 실행한다. 후보 끝에 `SET LOCAL search_path = fvoci`를 추가하여 원장 쓰기 경로를 복원했다.
  세션 전역 RESET을 사용하지 않으며 커밋·롤백 후에는 원래 세션 설정이 보존된다. 수정본의 실제 재검증 결과는 B 기록을 따른다.
- NOT RUN(A): 원본/후보 DB 동일성, 앱 역할 동작 테스트, Supabase 직접·session 연결. 이 작업은 B가 전담하며 그 결과를 최종 보고서에 합친다.
- NOT RUN: 최종 기준선 확정, 기존 설치 DB 전환, 배포. 이번 준비 작업의 실행 범위가 아니다.
