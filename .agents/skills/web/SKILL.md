---
name: web
description: fvoci 웹(apps/web)을 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지 — 결정 번호와 코드 위치, 검사 명령
---

# 웹 작업

규칙 본문은 결정 기록에 있다. 이 문서는 읽을 곳과 돌릴 검사만 가리킨다.

## 먼저 읽을 것

- 결정 기록: 바꾸려는 영역의 `docs/decisions/<주제>.md` 하나(맨 위 "현재 규칙 요약"). 나머지 번호는 `docs/decisions.md` 색인에서 찾는다.
  - 화면 구성·경로·보기·저장소 키·피드백·Storybook·다크 모드·브랜드 토큰·화면 다듬기: `ui.md`(#d-66 · #d-82 · #d-108 · #d-109 · #d-110 · #d-121 · #d-122)
  - 조직 문맥·접근 등급·보관: `access.md`(#d-111 · #d-113)
  - 입력 보존·초안·닫기 확인·기대 사용자: `documents.md`(#d-27 · #d-96 · #d-97)
  - 업무·상태 정의·충돌 재저장·세부 통일(이탈 취소·시간대 대체 등): `tasks.md`(#d-94 · #d-116 · #d-125)
  - 시간 기록: `time.md`(#d-117). 수락 경로: `product.md`(#d-128)
- 계약: `contracts/openapi.json`(어느 쓰기가 `x-fvoci-expected-user`를 받는지 포함), 공통 요청·오류·기대 사용자 머리 `apps/web/src/lib/shared/api/`, 도메인별 파서 `apps/web/src/lib/api/client.ts`·`features/tasks/api.ts`·`features/activity/api.ts`·`features/{records,courses,study}/{api,model}.ts`, 강좌·스터디 화면 조합 `features/courses/course-data.ts`·`features/study/study-data.ts`, 오류 문구 `apps/web/src/lib/api/errors.ts`.
- 파일 자원 계약·호출·파서: `apps/web/src/lib/features/files/api.ts`·`model.ts`. `AttachmentsPanel.svelte`는 별도 버전 모음을 읽으며 짧은 이름은 `createFormDraft({ preserve: false })`로 화면에만 둔다. 재개 정보는 `upload.ts`가 맡는다.
- 코드 위치(`apps/web/src/lib/`):
  - 조직 문맥 `org.svelte.ts`, 경로 `router.svelte.ts`, 브라우저 저장소 `storage.ts`
  - 폼·업무 CSV 초안의 공통 저장 `features/common/draft-storage.ts`, 폼 정책 `form-draft.ts`·`form-draft.svelte.ts`, 업무 조직 키 래퍼 `features/tasks/drafts.ts`. 이탈 확인 `features/common/leave-guard.svelte.ts`·`DiscardChangesConfirm.svelte`, 같은 줄 확인 `features/common/inline-confirm.ts`
  - 완료·오류 알림과 계정 전환 안전 상태 `features/feedback/feedback.svelte.ts`
  - 공용 입력 `ui/`, 색 토큰 `apps/web/src/styles/app.css`, 문구 `apps/web/messages/<영역>/{ko,en}.json`
  - 상태 정의 편집 `features/tasks/StatusEditor.svelte`
  - 댓글·알림·활동·감사·고정 읽기·명령은 `features/activity/api.ts`(생성 자원 타입). 댓글 작성·답글·수정의 영구 초안은 기존 공통 폼 저장을 유지한다(D-27·#154).
  - 업무 읽기·명령 `features/tasks/api.ts`(생성 자원 타입), 화면 조합 `board-data.ts`·`detail-data.ts`. 보드 조각 `BoardCard`·`BoardLane`·`BoardQuickAdd`·`BoardFilters`·`board-keyboard.ts`, 상세 조각 `TaskFields`·`TaskDependencies`·`TaskRecurrence`·`TaskSidePanels`와 공통 `ChecklistForm`.
  - 홈·내 업무 집계 읽기와 생성 타입 연결은 `features/home/api.ts`다. 홈의 내 업무 요약은 홈 응답의 `my_tasks`를 주입하며, 독립 내 업무 화면만 `/me/tasks`를 읽는다. 달력·알림·활동은 각 기능 API를 계속 쓴다.
  - 일정 읽기·명령 `features/events/api.ts`, 생성 자원 타입·파서 `model.ts`. 저장한 보기 읽기·명령 `saved-views-api.ts`, 타입·파서 `saved-views-model.ts`. 반복 끝 검증 오류는 `EventDialog`·`RecurrenceEditor`가 `reason`·`field`로 표시하며 `tests/component/event-until-error.test.ts`가 한·영 입력 보존과 칸 오류 연결을 확인한다.
  - 칸반 이동 동기화·되돌리기 `features/tasks/move-sync.ts`. 재번호 뒤에도 원래 이웃이 붙어 있으면 정확히 그 사이로 돌아간다(#50). 판 변경·다시 읽기 실패 시험은 `tests/component/task-move-sync.test.ts`·`task-move-undo.test.ts`.
- 시험 준비: `apps/web/tests/setup.ts`, `apps/web/tests/component/fixtures/`, 스토리 `apps/web/stories/fixtures/screens.ts`, e2e `apps/web/e2e/fixtures.ts`·증거 게시 `apps/web/e2e/publish.ts`.

## 돌릴 검사

- `apps/web`에서 `pnpm run check`, `pnpm test`, `pnpm run build-storybook`. 문구를 바꿨으면 `pnpm run i18n`(위 두 검사도 먼저 실행한다).
- 루트에서 `pnpm run lint`, `pnpm run format:check`, `cargo xtask check`.
- API를 바꿨으면 `cargo xtask openapi`와 `pnpm run generate:api`, 의존성을 바꿨으면 `cargo xtask licenses`.
- 실제 브라우저는 `cargo xtask browser`.
- 보고는 명령·대상 SHA·PASS/FAIL/NOT RUN·남은 제한(`AGENTS.md`, D-42).
