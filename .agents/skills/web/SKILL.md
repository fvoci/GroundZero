---
name: web
description: fvoci 웹(apps/web)을 바꿀 때 읽을 코드와 결정, UI·조직 문맥 규칙, 검증 명령. 오버홀 3단계 초안이며 서버·SQL 변경은 각 영역 스킬을 따른다.
---

# fvoci 웹 (3단계 초안)

## 먼저 읽기

- `AGENTS.md`와 `docs/decisions/ui.md`의 현재 규칙 요약. 업무는 `tasks.md`, 홈·알림은 `notify-audit.md`, 접근은 `access.md`, 편집 보존은 `documents.md`, 시간은 `time.md`에서 해당 조항만 읽는다. 수락 경로 D-128은 `docs/decisions/product.md`에 있다.
- 조직 문맥은 `apps/web/src/lib/org.svelte.ts`. 개인 화면은 `org.currentId`가 없으면 요청하지 않고 조직 선택 안내를 보인다. 첫 조직을 자동 선택하지 않는다. API 시험 준비의 단일 조직 확인은 UI 선택 정책이 아니다.
- 경로는 `apps/web/src/lib/router.svelte.ts`: 업무 보기 `/w/:ws/i/:item/tasks/board|list|calendar|gantt`, 상세 `/tasks/board/t/:taskId`, 내 업무 `/tasks`, 알림 `/notifications/:id`. 보기 전환은 경로 함수와 `router.navigate`로 하고 popstate도 미저장 보호를 거친다.
- 저장소는 `apps/web/src/lib/storage.ts`. 새 키는 `org.key(name)`으로 사용자·조직 범위를 넣는다(D-121).
- 응답 계약은 `contracts/openapi.json`, 파서는 `apps/web/src/lib/api/client.ts`의 `str/int/bool/optStr/rec` 규칙. 없는 서버 계약을 임의 optional·any·가짜 응답으로 숨기지 않는다.
- 컴포넌트 시험은 `apps/web/tests/setup.ts`·`tests/component/fixtures/organization.ts`(기본 org-1, 미선택은 `org.reset()`). 앱 셸 시험은 `api.myOrganizations`·`api.unreadCount`도 spy 한다. 스토리는 `apps/web/stories/fixtures/screens.ts`의 `setUpScreen`을 쓴다.

## 구현·검증 기준

- 입력은 `$lib/ui`, 본문 여러 열은 컨테이너 쿼리. 같은 줄 확인은 `features/common/inline-confirm.ts`의 초점 이동·복귀를 따르고 상태 알림은 처음부터 그려 둔다(D-66·D-110).
- 일반 완료는 `notifyDone`, 입력 오류는 필드 옆, 저장 실패·충돌은 입력을 보존한 화면 내 메시지, 배너에는 닫기 단추(D-82).
- 색은 `app.css` 토큰만 사용하고 hex를 추가하지 않는다. 밝음·어두움은 기존 테마 체계로 확인한다(D-109·D-122).
- 권한은 서버 `access`·`can_*`를 따른다. 역할 이름으로 다시 계산하지 않는다(D-113).
- 문구는 해당 영역의 `apps/web/messages/<영역>/{ko,en}.json` 양쪽에 두고 `pnpm --filter @fvoci/web run i18n`을 실행한다.
- 바꾼 제품 화면은 Vitest와 실제 컴포넌트 Storybook 상태를 추가한다. 기존 전역 `theme: 'light'|'dark'`, `fontScale: '100'|'200'`(루트 글꼴 %), 뷰포트 375/500/1280을 쓰고 새 데코레이터를 만들지 않는다. 스토리마다 밝음·어두움·375px·200% 상태를 둔다(D-108). DOM 단언만으로 색·여백·정보 위계를 검증했다고 보고하지 않는다.

## 검사

- `apps/web`에서 `pnpm run check`, `pnpm test`, `pnpm run build-storybook`; 루트에서 `pnpm run lint`, `pnpm run format:check`. 웹 패키지에는 별도 lint 스크립트가 없다.
- 공통 게이트는 루트 `cargo xtask check`. 의존성 변경이면 `cargo xtask licenses`; API 변경을 맡았다면 `cargo xtask openapi`와 `pnpm run generate:api`를 함께 실행한다.
- 실제 브라우저는 루트 `cargo xtask browser`. 오버홀 3단계에서는 지정된 브라우저 작업자만 실행한다. 격리 Compose를 쓰고 `--keep`, `cargo xtask down`, prune, 다른 체크아웃 자원 조작을 하지 않는다. 좁은 Chromium viewport는 실기기·Safari·화면 낭독기 검증이 아니다.
- `apps/web/e2e/fixtures.ts`가 테스트 OIDC 로그인과 API 준비를 맡는다. 조직을 `/api/me/organizations`에서 받고 공간 생성에 `organization_id`를 보낸다. 두 번째 사용자는 조직 초대 수락 뒤 공간에 초대한다. 개인 화면 조직은 UI 전환기로 고른다.
- 증거 게시기 `e2e/publish.ts`의 `recordSensitive`·`id_token_hint` 탐지와 `SAFE_TO_UPLOAD`를 유지한다. 원시 trace·토큰은 게시하지 않는다. 안전한 증거 정제는 4단계에서 별도 설계한다.
- 보고에는 실행 명령·대상 SHA·PASS/FAIL/NOT RUN·남은 제한을 적는다. 미구현 화면 실패를 skip이나 mock으로 통과시키지 않는다.
