# W11 — 업무·일정 대화상자 미저장 닫기 확인(A), ClashConfirm 값 상자 키보드(B), #82 formatNumber(C)

worktree: `w11-dialogs`. 로그는 모두 이 디렉터리(로컬 경로는 `<worktree>`·`<home>`으로 바꿈, 비밀값 없음).

## 커밋·push (모두 fast-forward, push 전 `git fetch` + `git merge-base --is-ancestor`)

#59 `fix/audit-web-calendar-home` — 시작 `21d92f4`, 최종 **`180a0ae`**
- `9e226e0` fix(events): ask before closing the event dialog drops unsaved input (공유 파일 포함)
- `88621a5` fix(events): lock the event fields while a request is on its way (review) — CodeRabbit 9e226e0 지적
- `7179119` fix(events): lock the event fields only while the event itself saves — 좁힘(다음 리뷰에서 겹침 지적을 받아 되돌림)
- `19161fc` fix(events): keep event and rule saves from overlapping (review) — CodeRabbit 7179119 지적
- `180a0ae` fix(events): do not send a conflict save while another request is on its way (review) — CodeRabbit 19161fc 지적

#82 `fix/audit-task-conflict-merge` — 시작 `1640dbc`, 최종 **`4d3ae8a`**
- `36e43d6` fix(tasks): ask before closing the task dialog drops unsaved input; keyboard-reachable long clash values (A·B·C, 공유 파일)
- `8cc2a52` fix(tasks): lock the task fields while a save is on its way (#59 지적과 같은 문제)
- `6c92d0f` fix(tasks): lock the task fields only while the form itself saves — 좁힘
- `cf44454` fix(tasks): lock the start date too while the form saves (review) — CodeRabbit 8cc2a52 지적
- `4d3ae8a` fix(tasks): neither close nor ask while the task form saves (review) — CodeRabbit cf44454 지적

## 공유 파일 (두 브랜치에 바이트까지 같음)

`50-shared-file-hashes.txt`: `180a0ae`(#59)와 `4d3ae8a`(#82)의 blob 해시, 17개 파일 모두 같음.
`DiscardChangesConfirm.svelte` = `b544b20173a76f127cd6668dc68089d539818b3b`. #82에는 #59 브랜치에서 `git checkout`으로 그대로 가져옴.

- `apps/web/src/lib/features/common/DiscardChangesConfirm.svelte` — 확인 UI. "계속 편집"(=Esc)은 확인만 닫고 `returnFocus`
  (없으면 Bits UI 기본: 열 때 초점이 있던 곳)로 초점을 돌린다. "변경 버리고 닫기"만 버린다(대화상자는 부르는 쪽이 닫음).
- `apps/web/src/lib/ui/alert-dialog/*` (13개) — **해석 선택**: 지침의 "shadcn-svelte/bits-ui 기존 구성요소"를 shadcn-svelte의
  기존 구성요소 `alert-dialog`를 저장소 규칙(D-66, ui/README)대로 CLI로 더하는 것으로 읽음:
  `pnpm dlx shadcn-svelte@1.7.0 add alert-dialog --yes --no-deps-install`. 새 npm 의존성 없음(bits-ui 2.19.5 기존).
  `alert-dialog-action/-cancel.svelte` 두 파일만 손댐: 생성된 `buttonVariants(...)` 호출을 ESLint가 `no-unsafe-call`로 거부 →
  `dialog-content`의 닫기 단추처럼 `child`로 `Button`을 그림(규칙 완화 없음). 생성기가 덮으려던 `button/`은 원본 유지(포맷만 달랐음).
- `apps/web/messages/common/{ko,en}.json` — `common_discard_title/description/keep/confirm`(ko/en).
- `apps/web/src/lib/ui/README.md` — 추가 명령·수정 기록 한 줄.

## A — 미저장 입력이 있는 대화상자 닫기

- 일정(#59): `EventForm`이 `onunsaved`로 알림(제목·설명은 `trimLikeServer`, 시간대는 보낼 때처럼 trim, 종일이면 날짜만·아니면
  시각·시간대만). `EventDialog`는 `bind:open` 함수 바인딩으로 닫기를 `close()`가 정하고, 고친 칸이 있으면 확인을 연다. 요청 중
  (`busy`)에는 예전처럼 닫지도 묻지도 않음(W24). `eventId`가 바뀌면 물음을 거두고, 버리기는 물은 차례(`shown`)가 같을 때만.
  `EventForm`은 Props 한 줄·import 한 줄·`submit` 뒤 블록만 바꿈(#72와 겹치지 않게).
- 업무(#82): `form.ts`에 `edited(before, mine)`(mergeForm의 `same` 판정을 모듈 함수로 올려 같이 씀). `TaskDetail`의 `unsaved`는
  `currentInput()` 대 `baseline`(예상 시간은 입력 글자의 값, 잘못된 글자는 고친 것), `onunsaved`로 알림. `baseline`은
  `$state.raw`(판정이 따라가게). 대화상자를 가진 `TaskBoard`가 `bind:open` 함수 바인딩 + `requestClose()`. `openTaskId`가 바뀌면
  물음을 거두고, 버리기는 물은 업무가 지금 업무일 때만.
- 저장 중: 두 대화상자 모두 폼을 저장하는 동안은 닫지도 묻지도 않는다(Esc·바깥 클릭 무시, 닫기 단추 숨김). 업무 대화상자는 원래
  저장 중에도 닫을 수 있었는데, 그때 "버리고 닫기"를 골라도 이미 보낸 저장은 서버에 남아 거짓이 되므로 일정 대화상자(W24)와 같게
  했다(CodeRabbit cf44454, `TaskDetail`이 `onsaving`으로 알림). 업무의 관계·상위 업무 요청 중에는 예전처럼 닫을 수 있다.
- 확인이 떠 있는 동안 바깥 대화상자의 Esc·바깥 클릭은 `ignore`(확인 층만 반응). D-94 덮어쓰기 확인의 Esc는 예전처럼 그 물음만
  거두고, 닫기 단추·바깥 클릭은 미저장 확인을 거쳐 "계속 편집"이면 물음·충돌 알림·입력 유지.
- 저장 중 칸 잠금(CodeRabbit #59 지적, 같은 문제를 #82에도): 저장되면 양식을 저장된 판으로 다시 그려(일정 `epoch`, 업무 `fill`)
  그사이 친 글이 사라졌다. 일정은 요청이 하나라도 가는 동안(`busy`) 일정 양식 감싸개와 반복 규칙 편집기를 각각 `<fieldset disabled>`로
  잠그고 충돌 알림의 "내 입력으로 다시 저장"·"덮어쓰고 저장"도 잠근다(`submitForm`도 돌아감): 일정 저장과 반복 규칙 저장이 겹치면
  늦게 온 쪽이 옛 일정을 다시 놓았다(CodeRabbit 7179119·19161fc). EventForm은 손대지 않음. 업무는 폼 저장(`submit`/`decide`)의
  `saving` 동안 폼 칸 `disabled={locked}`(시작일 포함). 관계·상위 업무 요청은 `refresh`가 고친 칸을 지키므로 잠그지 않음.

## B — ClashConfirm 값 상자(Safari 키보드)

넘쳐 스크롤되는 값 상자만 `role="region"` + `tabindex="0"` + `aria-labelledby`(칸 이름 + "서버의 지금 값"/"내 값")를 받는다.
`$effect`가 `rows`가 바뀔 때와 `ResizeObserver`로 크기가 바뀔 때 `scrollHeight > clientHeight`(또는 가로)를 잰다. 짧은 상자는
역할·탭 정지 없음. `dd`에는 역할을 줄 수 없어(ARIA in HTML) 안쪽 `div`가 스크롤 상자다(기존 시험 선택자 하나를 그쪽으로 옮김).

- 근거: 이름 있는 스크롤 영역에 초점을 주는 것이 표준 처리(WCAG 2.1.1, axe `scrollable-region-focusable`). 대안 읽기 전용
  `textarea`는 짧은 값에도 늘 탭 정지가 줄마다 둘씩 생기고 고칠 수 있는 칸처럼 읽혀 택하지 않음.
- lint: 예외 없이 돌리면 `a11y_no_noninteractive_tabindex` 경고로 svelte-check 실패(`32-pr82-B-without-ignore-svelte-check.log`;
  규칙은 조건부 tabindex도 막음). 그 요소 한 곳에만 `<!-- svelte-ignore a11y_no_noninteractive_tabindex -->` 한 줄, 이유·대안은 바로
  위 주석. 전역 설정 변경 없음.
- **실제 Safari: NOT RUN** (이 환경에 없음). 대체 확인은 jsdom 시험(크기를 흉내 냄)뿐이며, coordinator의 Chromium 확인은 Chrome이
  스크롤 영역에 기본으로 초점을 주므로 Safari 동작을 대신 증명하지 못한다.

## C — #82 formatNumber

`labels.ts` 예상 시간 표시를 `String(input.estimate_minutes)`로(폼 칸 `String(estimate_minutes)`과 같음), `formatNumber` import 제거.
#82가 `$lib/display`에서 쓰는 것은 이제 `formatDay`뿐이고, #62(`fix/audit-d92-cleanup`)가 `display.ts`에서 지우는 export는
`formatNumber` 하나뿐임을 확인.

## 새 시험·수정 전 실패

#59 `tests/component/event-dialog-discard.test.ts` (17개): 바로 닫힘(Esc·닫기 단추·바깥 클릭) · 서버가 지우는 공백은 고친 것
아님 · Esc → 확인, 확인의 Esc는 입력·초점 복원 · 닫기 단추 → 계속 편집 → 초점 대화상자 안 · 바깥 클릭 → 확인, 확인 바깥 클릭은
아무것도 안 바꿈 · 버리기를 골라야만 닫힘 · 설명 칸·칸 이름·대화상자 바탕 뒤 Esc · 저장 뒤엔 다시 바로 닫힘 · 저장 중엔 묻지도
닫지도 않음 · 저장 중 칸 잠금 · 일정 저장과 반복 규칙 저장이 서로 잠금 · 반복 규칙 저장 중 "내 입력으로 다시 저장" 안 보냄 ·
충돌 알림 유지 · 덮어쓰기 물음 유지 · 다른 일정이 열리면 물음을 거두고 그 일정을 닫지 않음.

#82 `tests/component/task-dialog-discard.test.ts` (14개, 같은 흐름 + 숫자가 아닌 예상 시간, 저장 중 칸 잠금(시작일 포함), 폼 저장
중엔 닫지도 묻지도 않음), `task-conflict-save-mine.test.ts` "lets the keyboard reach a value box that scrolls, and leaves short ones
alone"(B, 기존 시험 하나의 선택자를 안쪽 스크롤 상자로 옮김), `task-form-merge.test.ts` "shows the estimate the way the form field
shows it"(C).

| 로그 | 소스 | 결과 |
|---|---|---|
| 01/02-pr59-new-tests-before-fix*.log | 21d92f4 원본 EventDialog·EventForm | 14개 중 8개 FAIL(물음 없음), 6개 PASS(바로 닫힘 등 기존 동작) |
| 03-pr59-mutation-no-trim.log | 제목 trim 뺌 | "does not count spaces" FAIL |
| 04-pr59-mutation-reopen-keeps-question.log | 다시 열 때 물음 안 거둠 | 다른 일정 열림 시험 FAIL |
| 05-pr59-cr-lock-before-fix.log | 9e226e0 | "locks the fields" FAIL |
| 06-pr59-cr-serialize-before-fix.log | 7179119 | 일정 저장 중 반복 규칙 편집기가 잠기지 않음 FAIL |
| 07-pr59-cr-save-mine-busy-before-fix.log | 19161fc | 반복 규칙 저장 중 "내 입력으로 다시 저장"이 잠기지 않음 FAIL |
| 31-pr82-new-tests-before-fix.log | 1640dbc 원본 | 11개 FAIL: 확인 없음 9, region 없음 1(B), '1,500'≠'1500' 1(C) |
| 45-pr82-mutation-reopen-keeps-question.log | 다시 열 때 물음 안 거둠 | 다른 업무 열림 시험 FAIL |
| 46-pr82-mutation-always-tab-stop.log | 늘 region·tabindex 0 | B 시험 FAIL |
| 47-pr82-mutation-raw-compare.log | 저장 모양 대신 원값 비교 | "does not count what the server would store" FAIL |
| 48-pr82-lock-before-fix.log | 36e43d6 | "locks the fields" FAIL |
| 49-pr82-cr-start-date-before-fix.log | 6c92d0f | `task-start` 잠기지 않음 FAIL |
| 49b-pr82-cr-no-close-while-saving-before-fix.log | cf44454 | 저장 중 Esc에 확인이 뜸 FAIL |

jsdom 한계: 클릭에 따른 초점 이동을 하지 않아 `.focus()`로 흉내, 바깥 클릭은 가림막 pointerdown(좌표 500,500)으로 Bits UI의 판정을
탐. 실제 브라우저 확인은 coordinator 몫(NOT RUN).

## 검사 (`cd apps/web && pnpm run i18n && pnpm exec eslint . && pnpm exec svelte-check --tsconfig ./tsconfig.json --fail-on-warnings && pnpm exec vitest run`, format은 루트 `pnpm run format:check`)

| 대상 | format | i18n | eslint | svelte-check | vitest | 로그 |
|---|---|---|---|---|---|---|
| #59 `9e226e0` | PASS | PASS | PASS | PASS 0/0 | PASS 86파일 450 | 10~14 |
| #59 최종 `180a0ae` | PASS | PASS | PASS | PASS 0/0 | PASS 86파일 453 | 15~19 |
| #82 최종 `4d3ae8a` | PASS | PASS | PASS | PASS 0/0 | PASS 78파일 446 (첫 실행에서 부하로 5초 시간 초과 2개 — gantt-clipped-end, task-dialog-history — 따로 돌리면 통과, 다시 돌린 전체 PASS) | 40~44 |
| #59 `180a0ae` + #72 `0213eb8` (`0986316`, 로컬, 충돌 없음) | PASS | PASS | PASS | PASS | PASS 86파일 470 | 61-* |
| #59 `180a0ae` + #82 `4d3ae8a` (`ddac7b9`, 로컬, 충돌 없음) | PASS | PASS | PASS | PASS | PASS 88파일 514 | 66-* |
| (앞선 머리) #59 `9e226e0`·`88621a5` + #72 | PASS | PASS | PASS | PASS | PASS 467·468 | 21~29 |

`pnpm install --frozen-lockfile` PASS. `cargo xtask check`/`cargo xtask test`: NOT RUN (Rust·서버·계약 변경 없음, 인수 기준은 웹 검사).
합친 로컬 브랜치(`w11-merge59-72*`, `w11-merge59-82*`)는 push하지 않았고 지우지 않음.

## CodeRabbit (리뷰 본문·Outside diff·인라인·요약 댓글을 머리마다 확인)

| 머리 | 결과 | 처리 |
|---|---|---|
| #59 `9e226e0` | Outside diff 🟡 Minor: 저장 중 입력을 잠그라(저장되면 양식을 다시 그려 그사이 입력이 사라짐) | 유효 → `88621a5`. 제안한 EventForm `fieldset`은 #72와 충돌하므로 EventDialog 감싸개에서 |
| #59 `88621a5` | "No actionable comments" | — |
| #59 `7179119` | Outside diff 🟡 Minor: 일정 저장과 반복 규칙 저장을 직렬화하라(늦은 응답이 옛 일정을 놓음) | 유효 → `19161fc`(`busy` 동안 두 입력 모두 잠금) |
| #59 `19161fc` | 인라인 🟡 Minor(EventDialog.svelte:471): 충돌 저장 단추도 `busy` 동안 막으라 | 유효 → `180a0ae` |
| #59 `180a0ae` (최종) | "No actionable comments" (요약 19161fc..180a0ae), 상태 SUCCESS | — |
| #82 `36e43d6` | "No actionable comments" | — |
| #82 `8cc2a52` | Outside diff 🟠 Major: 시작일 칸도 잠그라(`!canEdit \|\| f.milestone`이 빠짐) | 유효 → `cf44454` |
| #82 `cf44454` | 인라인 🟡 Minor(TaskDetail.svelte:377): 저장 중에는 닫기·버리기를 막으라 | 유효 → `4d3ae8a`(`onsaving`) |
| #82 `4d3ae8a` (최종) | "No actionable comments" (요약 cf44454..4d3ae8a), 상태 SUCCESS | — |

자동 리뷰가 멈춰서(auto-pause) 두 PR에 `@coderabbitai resume`·`@coderabbitai review` 댓글을 남겼다(지침의 "멈춤이면 resume").
기각한 지적 없음. 지적에 대한 PR 답글은 달지 않았다(수정 SHA는 이 표에). CodeRabbit의 시간당 리뷰 한도가 0까지 쓰였다.

## 범위 밖으로 남긴 것

- 반복 규칙 편집기(RecurrenceEditor)의 저장하지 않은 규칙, 업무 상세의 체크리스트 새 항목·선행/상위 업무 고르기·지연일·댓글 초안·
  하위 업무 만들기 칸은 "고친 입력"에 넣지 않음(본 폼의 기준 값 비교만). RecurrenceEditor는 #59와 #82에서 내용이 달라 손대면 공유가 깨짐.
- 대화상자 안에서 다른 업무로 옮기기(하위·선행 업무 링크)·지우기 뒤 이동·브라우저 뒤로 가기는 확인하지 않음(결정 범위: Esc·바깥 클릭·
  닫기 단추). 다른 업무·일정이 열리면 앞의 물음만 거둔다.
- 달력의 업무 만들기 대화상자(CalendarTaskDialog)는 편집 대화상자가 아니라 넣지 않음.
- 실제 브라우저(Chromium) 확인, 실제 Safari 확인: NOT RUN.
