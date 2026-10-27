---
name: server
description: fvoci 서버(crates/server)를 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지 — 접근 등급 모듈, 조직 문맥, 정의자 함수 호출 규칙
---

# 서버 작업

## 먼저 읽을 것

- 결정 기록: 바꾸려는 영역의 `docs/decisions/<주제>.md`("현재 규칙 요약"). 권한·조직은 `access.md`(D-111~D-115), 알림·활동·내 업무는 `notify-audit.md`(D-118~D-120), 한도·보존·로그는 `operations.md`, 인증·초대는 `auth.md`, 시간은 `time.md`.
- `crates/server/src/access.rs`: 접근 등급은 SQL `app_access` 하나가 계산하고 서버는 `Grade`로 읽기만 한다(D-113). 읽기 경로의 404/403 구분은 `require_visible`·`require_write`·`require_owner`, 변경은 정의자 함수가 검사하고 `AppError::from_db_function`이 코드를 바꾼다(P0002→404, 42501→403, 22023→400, 55000→409). 역할·보관·공유로 등급을 다시 계산하지 않는다.
- `crates/server/src/features/organizations.rs`: 개인 화면(홈·내 업무·알림·활동·공유받은 문서·검색)의 현재 조직 문맥 `OrgScope`(질의 `organization_id`; 없으면 400, 멤버가 아니면 404)와 조직·서버 관리자 API(멤버·초대·설정·한도·워크스페이스 메타데이터·관리자로 참여). 개인 화면은 `features/home.rs`·`notifications.rs`·`activity.rs`, 항목별 상태 정의는 `features/task_statuses.rs`(권한·불변식은 SQL).
- 부르는 SQL 함수의 시그니처·id 개수는 `crates/server/migrations/202610250001_initial_state.sql`에서 직접 확인한다(`grep '^CREATE FUNCTION fvoci.<이름>('`). 새 행 id는 서버가 만든다(`new_ids(n)`, 가변은 `needed_ids`).
- 기대 사용자(D-96): 초안을 든 쓰기(문서 저장·노트 만들기·업로드 시작·조직 설정·워크스페이스 한도)는 `ExpectedUser`로 받아 쓰기 전에 400/409를 낸다. 개인 설정·문서 설정은 `CurrentUser`만 쓴다. 머리는 비교에만 쓴다.

## 바꿀 때 지킬 것

- 핸들러는 입력 검증과 응답 변환만 한다. 권한·불변식·감사는 SQL 정의자 함수. 오류 코드는 D-81 언어 중립(기존 `AppError` 변종).
- API를 바꾸면 `#[utoipa::path(..., operation_id = "<고유>")]`와 `http/mod.rs`의 paths·schemas·라우터를 함께 고치고 `cargo xtask openapi`·`pnpm run generate:api`로 계약·클라이언트를 재생성한다.
- 시험은 실제 앱 역할(`state.db.begin_as`)과 실제 HTTP로 거부 경로를 확인한다. 기대값을 `app_access`에서 만들지 않는다. 잠금 대기는 `pg_blocking_pids`로 확인한다(고정 지연 금지).
- Rust 관용·오류 처리·패닉 경로는 `.agents/skills/rust-review/`(BeautifulRust rust-review, CodeRabbit도 같은 지침을 씀)를 참고한다. 저장소 규칙이 우선하고, 검사 규칙 변경은 제안하지 않는다.

## 돌릴 검사

- `cargo xtask check`(fmt·clippy·단위·OpenAPI 대조·웹 검사), `cargo xtask test`(격리 통합), 로그인·Supabase 경로를 건드렸으면 `cargo xtask test --oidc`, `cargo xtask test --supabase --session-pooler --oidc`. 결과는 명령·SHA·PASS/FAIL/NOT RUN으로 적는다(D-42).
