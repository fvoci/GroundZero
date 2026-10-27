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
- 계약: `contracts/openapi.json`(어느 쓰기가 `x-fvoci-expected-user`를 받는지 포함)과 그 파서 `apps/web/src/lib/api/client.ts`, 오류 문구 `apps/web/src/lib/api/errors.ts`.
- 코드 위치(`apps/web/src/lib/`):
  - 조직 문맥 `org.svelte.ts`, 경로 `router.svelte.ts`, 브라우저 저장소 `storage.ts`
  - 폼 초안 `features/common/form-draft.ts`·`form-draft.svelte.ts`, 이탈 확인 `features/common/leave-guard.svelte.ts`·`DiscardChangesConfirm.svelte`, 같은 줄 확인 `features/common/inline-confirm.ts`
  - 완료·오류 알림과 계정 전환 안전 상태 `features/feedback/feedback.svelte.ts`
  - 공용 입력 `ui/`, 색 토큰 `apps/web/src/styles/app.css`, 문구 `apps/web/messages/<영역>/{ko,en}.json`
  - 상태 정의 편집 `features/tasks/StatusEditor.svelte`
- 시험 준비: `apps/web/tests/setup.ts`, `apps/web/tests/component/fixtures/`, 스토리 `apps/web/stories/fixtures/screens.ts`, e2e `apps/web/e2e/fixtures.ts`·증거 게시 `apps/web/e2e/publish.ts`.

## 돌릴 검사

- `apps/web`에서 `pnpm run check`, `pnpm test`, `pnpm run build-storybook`. 문구를 바꿨으면 `pnpm run i18n`(위 두 검사도 먼저 실행한다).
- 루트에서 `pnpm run lint`, `pnpm run format:check`, `cargo xtask check`.
- API를 바꿨으면 `cargo xtask openapi`와 `pnpm run generate:api`, 의존성을 바꿨으면 `cargo xtask licenses`.
- 실제 브라우저는 `cargo xtask browser`.
- 보고는 명령·대상 SHA·PASS/FAIL/NOT RUN·남은 제한(`AGENTS.md`, D-42).
