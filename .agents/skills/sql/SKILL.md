---
name: sql
description: fvoci 기준 SQL(crates/db/migrations/의 영역 기준 파일 열 개)을 읽거나 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지
---

# 기준 SQL 작업

## 먼저 읽을 것

- 결정 기록: 바꾸려는 영역의 `docs/decisions/<주제>.md` 하나(맨 위 "현재 규칙 요약"). 접근·조직 경계는 `access.md`(D-111~D-115), id·멱등·기준 마이그레이션은 `database.md`(D-88·D-100), 나머지 번호는 `docs/decisions.md` 색인에서 찾는다.
- `crates/db/migrations/`의 영역 기준 파일 열 개(`202610280001_foundation.sql`~`202610280010_access_grants.sql`, 의존 순서): 첫 파일의 머리말 주석과 바꾸려는 함수의 `CREATE FUNCTION` 본문(`grep -n '^CREATE FUNCTION fvoci.<이름>(' crates/db/migrations/*.sql`). 01 기반(사용자·세션·검색 큐), 02 조직·공간·항목과 접근 계산 전부, 03 문서, 04 파일, 05 업무·일정, 06 기록·학습, 07 시간, 08 댓글·알림·활동·감사, 09 예약 실행·휴지통, 10 RLS·정책·GRANT·REVOKE와 기준 마커다. 접근 등급은 `app_access(kind, id)`(none|read|write|owner) 하나이고, 새 행 id는 앱이 주는 `p_ids`에서 `app_id(p_ids, n)`으로 꺼내며 가변 개수는 짝이 되는 `*_ids` 함수가 센다.
- 기록·학습 호출부는 `crates/domains/records/src/`(`fvoci-records`), 해당 HTTP 시험은 `crates/server/tests/it/records/`다. worker 전체 실행 시험은 별도 `tests/records.rs`다.
- 호출부: `crates/server/src/features/*.rs`·`crates/domains/*/src/*.rs`(`rg -F 'app_<이름>('`)와 `crates/server/tests/`. 시험은 주제별로 `tests/it/<주제>.rs`(실행 파일 하나 `it/main.rs`)에 있고, 공유 DB 전체를 보는 작업을 부르는 시험만 `tests/` 바로 아래 각자의 실행 파일이다(`it/main.rs` 머리 주석). 공통 헬퍼는 `tests/common/mod.rs`, 앱 역할 직접 호출 헬퍼는 `tests/sql_support/mod.rs`, 기준 번호·체크섬 시험은 `tests/it/migration_baseline.rs`.

## 바꿀 때 지킬 것

- 정식 `v1.0.0` 원격 태그 전의 오버홀 동안은 새 마이그레이션 파일을 더하지 않고 기준 파일을 고친다. 그 뒤에는 새 번호로 더한다(D-107, D-131). 기준 번호를 바꾸면 함께 맞춘다: `crates/db/src/admin.rs`의 `BASELINE_VERSION`(첫 파일 번호)·`BASELINE_MARKER`(마지막 파일이 남긴다)·`BASELINE_APPLYING_MARKER`(첫 파일이 남긴다), 첫 파일과 마지막 파일의 `COMMENT ON TABLE fvoci._sqlx_migrations` 문자열.
- 영역 파일은 SQLx가 파일마다 트랜잭션 하나로 적용한다. 파일마다 머리 `SET LOCAL` 설정과 끝의 `SET LOCAL search_path = fvoci;`를 둔다. 만드는 순서를 묶는 것은 표 CHECK가 부르는 함수(앞 파일), 표 행 타입을 서명에 쓰는 함수(그 표 뒤), 트리거 함수(그 표의 파일이나 앞 파일), 외래 키(앞 파일이나 같은 파일의 표)뿐이다(`check_function_bodies = false`).
- 함수 시그니처(인자·반환·id 개수)를 바꾸면 그 함수 파일의 `ALTER FUNCTION ... OWNER`, 10번 파일의 `REVOKE`/`GRANT` 줄, Rust 호출부, 시험을 함께 고친다. API가 바뀌면 `cargo xtask openapi`와 `pnpm run generate:api`.
- 권한 검사는 서버가 아니라 SQL 함수와 RLS에 둔다. 거부 경로 시험은 앱 역할(`state.db.begin_as`)로 돌리고 기대값을 `app_access`에서 유도하지 않는다.

## 돌릴 검사

- `cargo xtask check`(fmt·clippy·단위 시험·OpenAPI 대조·웹 검사), `cargo xtask test`(격리 통합 시험). `cargo xtask test`는 이 체크아웃의 격리 Compose(tmpfs DB)에 운영자 절차와 같은 `db bootstrap` → `db migrate`로 기준 SQL을 설치한 뒤 시험을 돌린다(`xtask/src/compose.rs` `prepare_database`). PostgreSQL 18은 CI `oidc` job이 본다. 로그인·Supabase 경로를 건드렸으면 `cargo xtask test --oidc`, `cargo xtask test --supabase --session-pooler --oidc`.
- 보고에는 명령·대상 SHA·PASS/FAIL/NOT RUN과 SQL 파일 sha256·`fvoci._sqlx_migrations` 체크섬을 적는다(D-42). 검사를 끄거나 건너뛰어 통과시키지 않는다.
