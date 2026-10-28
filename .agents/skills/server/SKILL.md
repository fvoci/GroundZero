---
name: server
description: fvoci 서버(crates/server와 공통 crate core·db·http)를 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지 — 접근 등급 모듈, 조직 문맥, 정의자 함수 호출 규칙
---

# 서버 작업

## 먼저 읽을 것

- 결정 기록: 바꾸려는 영역의 `docs/decisions/<주제>.md`("현재 규칙 요약"). 권한·조직은 `access.md`(D-111~D-115), 알림·활동·내 업무는 `notify-audit.md`(D-118~D-120), 한도·보존·로그는 `operations.md`, 인증·초대는 `auth.md`, 시간은 `time.md`.
- `crates/core/src/access.rs`(등급 타입)·`crates/db/src/access.rs`(등급 질의 도우미, 서버 옛 경로는 재수출): 접근 등급은 SQL `app_access` 하나가 계산하고 서버는 `Grade`로 읽기만 한다(D-113). 읽기 경로의 404/403 구분은 `require_visible`·`require_write`·`require_owner`, 변경은 정의자 함수가 검사하고 `AppError::from_db_function`이 코드를 바꾼다(P0002→404, 42501→403, 22023→400, 55000→409). 역할·보관·공유로 등급을 다시 계산하지 않는다.
- `crates/domains/organizations`(`fvoci-organizations`): 계정·조직·공간·멤버 관리 API. 개인 화면의 현재 조직 문맥 `OrgScope` 정의는 `fvoci_http::organization`에 있다(질의 `organization_id`; 없으면 400, 멤버가 아니면 404). 로그인·로그아웃·OIDC와 실시간 공간 설정은 조립 crate의 `features/accounts.rs`·`features/workspaces.rs`에 남는다. 홈 집계는 `features/home.rs`, 댓글·알림·활동·감사·고정은 `crates/domains/activity`(`fvoci-activity`), 업무 도메인(업무·상태 정의·선행 관계·반복 업무·일정·일정 반복·저장한 보기)은 `crates/domains/work`(`fvoci-work`)에 있다(권한·불변식은 SQL).
- 파일 도메인: `crates/domains/files`(`fvoci-files`), 부분 상태·주입 경계 `state.rs`, 경로 `router.rs`, 자원 `model.rs`·`read.rs`, 업로드 `uploads.rs`·`completion.rs`, 명령 `commands.rs`, 내용 `content.rs`, 정리 본문 `cleanup.rs`. 저장소 구현·연결 구성은 `crates/storage`(`fvoci-storage`)다. 일정과 백업 잠금은 서버 조립부에 남는다.
- 오류 계약: 칸 검증 거절은 `AppError::invalid`(`crates/core/src/error.rs`)로 400 `bad_request`에 `reason`·`field`·`limit`을 싣는다(#76·#79). 판 충돌 409는 `fvoci_http::conflict::StaleWrite`(지금 자원과 바뀐 칸, #180 서버 부분)를 쓴다. 공통 검증은 `crates/core/src/validate.rs`.
- 달력 집계는 `crates/aggregates/calendar`(`fvoci-calendar`)의 `CalendarState`·`CalendarApi`·라우터이며, SQL `app_calendar` 하나로 읽는다. 시험은 `tests/it/calendar/`에 있다(D-80). 반복 규칙·검증 정본은 `fvoci_core::recurrence`이고 업무 도메인은 기존 `work::recurrence` 경로로 재노출한다.
- 개인 시간은 `crates/domains/time`(`fvoci-time`)에 있고 시험은 `it/time/`이다. 기록 자원과 대상 표시 모음(`/time-entry-targets`)은 따로 읽어 웹의 `features/time/data.ts`에서 조합한다. 업무·항목 예상/실제 비교는 시간 함수와 업무 표를 SQL 한 문장으로 읽으며 도메인 crate끼리 의존하지 않는다.
- 부르는 SQL 함수의 시그니처·id 개수는 `crates/db/migrations/`의 영역 기준 파일에서 직접 확인한다(`grep -n '^CREATE FUNCTION fvoci.<이름>(' crates/db/migrations/*.sql`). 새 행 id는 서버가 만든다(`new_ids(n)`, 가변은 `needed_ids`).
- 기대 사용자(D-96): `ExpectedUser`를 받는 처리기는 머리가 필수다. 업무·소통활동·조직 도메인 라우터의 `ExpectedUserIfSent`는 머리가 있으면 모든 명령에서 비교하고 400/409(`account_changed`)를 낸다. 머리는 비교에만 쓰며, 소통활동·조직 명령의 머리는 계속 선택이다(조직 설정·공간 한도 PUT의 기존 필수 머리는 유지)(D-96 개정). 파일 도메인도 같은 처리를 걸며 업로드 시작만 계속 필수이고 다른 파일 명령은 머리를 보내면 비교한다. 붙은 경로·빠진 경로와 빠진 경로의 위험은 [expected-user.md](expected-user.md).
- 템플릿으로 새 문서(`features/documents.rs` `initial_body`): 위키·항목 문서는 SQL `app_template_body`가 본문을 준다. 공간 문서 템플릿은 같은 조직 공간에서만 쓰고(다른 조직이면 409) 원본에 복제 감사를 남긴다(D-114·D-115). 개인 노트는 문서 RLS로 읽을 수 있는 템플릿만 쓴다.
- 마감 알림(#86): 보관이 커밋된 뒤에는 마감 알림이 없다. 기준 SQL `app_notify`의 `due` 분기가 `app_lock_item`으로 잠근 뒤 `app_item_archived`를 다시 본다. 시험 `tests/it/activity/comment_archive_race.rs`의 `due_notifications_wait_for_archive_and_do_not_notify_after_commit`.

## 바꿀 때 지킬 것

- 핸들러는 입력 검증과 응답 변환만 한다. 권한·불변식·감사는 SQL 정의자 함수. 오류 코드는 D-81 언어 중립(기존 `AppError` 변종).
- API를 바꾸면 `#[utoipa::path(..., operation_id = "<고유>")]`와 도메인의 OpenAPI·라우터(`WorkApi`·`ActivityApi`) 및 `http/mod.rs` 조립을 함께 고치고 `cargo xtask openapi`·`pnpm run generate:api`로 계약·클라이언트를 재생성한다.
- 시험은 실제 앱 역할(`state.db.begin_as`)과 실제 HTTP로 거부 경로를 확인한다. 기대값을 `app_access`에서 만들지 않는다. 잠금 대기는 `pg_blocking_pids`로 확인한다(고정 지연 금지). 새 시험은 `crates/server/tests/it/<주제>.rs`에 두고 `it/main.rs`에 모듈로 더한다. 도메인 crate로 옮긴 영역의 시험은 `it/<도메인>/`(예: `it/work/`) 폴더의 `mod.rs`에 더한다. 공유 DB 전체 작업·프로세스 전역 상태를 쓰는 시험만 `tests/` 바로 아래 각자의 실행 파일로 둔다(`it/main.rs` 머리 주석).
- Rust 코드를 쓸 때는 `.agents/skills/rust-best-practices/`, 검토할 때는 `.agents/skills/rust-review/`(BeautifulRust, CodeRabbit도 rust-review를 씀)를 참고한다. 저장소 규칙이 우선하고, 검사 규칙 변경은 제안하지 않는다.

## 돌릴 검사

- `cargo xtask check`(fmt·clippy·단위·OpenAPI 대조·웹 검사), `cargo xtask test`(격리 통합), 로그인·Supabase 경로를 건드렸으면 `cargo xtask test --oidc`, `cargo xtask test --supabase --session-pooler --oidc`. 결과는 명령·SHA·PASS/FAIL/NOT RUN으로 적는다(D-42).
