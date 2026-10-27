---
name: sql
description: fvoci 기준 SQL(crates/server/migrations/202610250001_initial_state.sql)을 읽거나 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지
---

# 기준 SQL 작업

## 먼저 읽을 것

- 결정 기록: 바꾸려는 영역의 `docs/decisions/<주제>.md` 하나(맨 위 "현재 규칙 요약"). 접근·조직 경계는 `access.md`(D-111~D-115), id·멱등·기준 마이그레이션은 `database.md`(D-88·D-100), 나머지 번호는 `docs/decisions.md` 색인에서 찾는다.
- `crates/server/migrations/202610250001_initial_state.sql`: 머리말 주석과 바꾸려는 함수의 `CREATE FUNCTION` 본문. 접근 등급은 `app_access(kind, id)`(none|read|write|owner) 하나이고, 새 행 id는 앱이 주는 `p_ids`에서 `app_id(p_ids, n)`으로 꺼내며 가변 개수는 짝이 되는 `*_ids` 함수가 센다.
- 호출부: `crates/server/src/features/*.rs`(`rg -F 'app_<이름>('`)와 `crates/server/tests/`(같은 주제의 시험, 공통 헬퍼 `common/mod.rs`, 앱 역할 직접 호출 헬퍼 `sql_support/mod.rs`).

## 바꿀 때 지킬 것

- 정식 `v1.0.0` 원격 태그 전의 오버홀 동안은 새 마이그레이션 파일을 더하지 않고 기준 파일을 고친다. 그 뒤에는 새 번호로 더한다(D-107, D-131). 기준 번호를 바꾸면 `crates/server/src/db/admin.rs`의 `BASELINE_VERSION`·마커도 맞춘다.
- 함수 시그니처(인자·반환·id 개수)를 바꾸면 같은 파일의 `ALTER FUNCTION ... OWNER`와 `REVOKE`/`GRANT` 줄, Rust 호출부, 시험을 함께 고친다. API가 바뀌면 `cargo xtask openapi`와 `pnpm run generate:api`.
- 권한 검사는 서버가 아니라 SQL 함수와 RLS에 둔다. 거부 경로 시험은 앱 역할(`state.db.begin_as`)로 돌리고 기대값을 `app_access`에서 유도하지 않는다.

## 돌릴 검사

- 설치 확인: 격리 Postgres(`docker compose -p <고유 이름> -f deploy/compose.test.yaml up -d postgres`)에 `cargo run -p fvoci-server -- db bootstrap`과 `db migrate`를 적용한다. 끝나면 그 프로젝트만 `down --volumes`로 정리한다.
- `cargo xtask check`(fmt·clippy·단위 시험·OpenAPI 대조·웹 검사), `cargo xtask test`(격리 통합 시험). 로그인·Supabase 경로를 건드렸으면 `cargo xtask test --oidc`, `cargo xtask test --supabase`.
- 보고에는 명령·대상 SHA·PASS/FAIL/NOT RUN과 SQL 파일 sha256·`fvoci._sqlx_migrations` 체크섬을 적는다(D-42). 검사를 끄거나 건너뛰어 통과시키지 않는다.
