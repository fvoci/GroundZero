# W2 — PR #82 (D-94 업무) 남은 지적과 독립 재검증

- 브랜치: `fix/audit-task-conflict-merge`, 시작 `263e471` → push `1640dbc8db797fe0dad262d5e56f7f5d153fa58d` (fast-forward, push 전 `git merge-base --is-ancestor` 확인)
- 커밋: `1640dbc` fix(web): keep the task overwrite question through late replies and compare fields as the server stores them (D-94 review)
- 서버 계약: 바꾸지 않음(웹만). 새 의존성·검사 규칙 완화 없음.
- PR 댓글(CodeRabbit outside-diff 2건 + nitpick 답): https://github.com/fvoci/GroundZero/pull/82#issuecomment-6331273235
  (TASK 본문이 PR 댓글로 답하라고 명시해 공통 규칙의 "다른 채널에 쓰지 않음"보다 우선으로 보았다.)

## 처리 내용

| 항목 | 처리 | 근거·시험 |
|---|---|---|
| 1. CodeRabbit Minor: 같은 이름 담당자 | 고침. 이름이 겹칠 때만 id 끝 네 글자(`#a1a1`, D-17 배지의 `shortId`). 폼 체크박스 이름표도 같은 글. 모르는 사람도 `#xxxx`로 구분. 이메일 등은 쓰지 않음 | `labels.ts` `assigneeLabels`; 시험 "tells apart people with the same name"(컴포넌트·단위) |
| 2. CodeRabbit Minor / 2차 [낮음]: 늦은 응답이 확인을 닫음 | 고침. `refresh()`가 확인이 떠 있었으면 `askAgain()`: 아는 가장 새 판으로 다시 묻고, 받은 판이 충돌 판보다 새면 충돌 알림도 그 판으로. 겹친 칸이 없어지면 확인을 닫고 초점을 "내 입력으로 다시 저장"으로(보내지 않음) | "a late response while the overwrite question is open" 3개 |
| 3a. 앞뒤 공백만 친 칸 | 고침. 비교를 저장될 값(서버와 같은 유니코드 White_Space trim, `trimLikeServer`)으로. 친 중인 글은 저장될 값이 서버와 같으면 폼에 그대로 | "does not treat surrounding spaces as an edit", "takes the server value when I only added surrounding spaces", "trims the same characters as the server"(U+FEFF/U+3000) |
| 3b. 태그 중복 제거 | 고침. `currentInput`이 `validate_tags`처럼 trim 후 처음 것만 둠 | "reads tags the way the server stores them, without repeats", "shows and saves my tags without repeats" |
| 3c. 여전히 똑같아 보이는 쌍 | 같은 이름 담당자는 1로 고침. 나머지(줄 끝 공백, 보이지 않는 글자 U+FEFF 등, 실제 글 "(비어 있음)"과 빈 값, 쉼표가 든 태그)는 값을 바꾸어 보이는 대신 확인 줄에 "두 값이 같아 보이지만 저장되는 값은 다릅니다" 안내(`looksSame`) | "says so when the two values look the same", "marks pairs that look the same" |
| 3d. JS/Rust trim 차이 | 고침(`\p{White_Space}`로 Rust `str::trim`과 같게). U+0085는 서버가 제어 문자로 거절하므로 실제로는 U+FEFF만 의미 있음 | 위 trim 시험 |
| nit: 폼 저장의 다시 409 초점 | 고침. `refocus = pending !== undefined`(누른 폼 "저장"이 남아 있으므로 그대로) | "leaves focus on the form save button when saving the form conflicts again" |
| nit(기존): "서버 내용 불러오기" 뒤 초점이 body | 고침. 제목 칸으로 | "still replaces the form …"에 초점 단언 추가 |
| nit(기존): 309자리 예상 시간이 null로 지워짐 | 고침. i32 상한 밖은 잘못된 입력 | "rejects numbers the server cannot store" |
| nit(기존): 미리보기 취소 시 초점 `task-save` | 기각(그대로). 있는 유효한 단추이고 범위 밖 기존 동작 | — |
| [낮음, 접근성] Safari 값 상자 키보드 스크롤 | 고치지 않음(보고). 비대화형 `dd`에 tabindex가 필요해 svelte a11y lint에 걸림. 규칙 완화는 사용자 확인 대상 | — |
| 5. 시험 빈 곳 | 보강: 덮어쓰기 → 다시 409(같은 칸 또 바뀜) → 새 값으로 다시 묻고 판 4로 보냄; 덮어쓰기 실패 → 입력 보존, 승인한 판 3으로 다시 시도. 기존: 내 칸만 적용, 같은 칸 충돌, 재저장 중 409(겹침 없음), 담당자 순서 무관 | "asks again with the newer value when overwriting conflicts again", "keeps my input when overwriting fails and retries on the version I approved" |

## 수정 전 실패 기록

- `new-tests-fail-before-on-263e471.log`: 새 시험을 `263e471` 소스에서 돌린 결과. 10개 실패. 그중 "keeps my input and lets me ask again when overwriting fails"는 내 기대가 틀린 것이라(승인한 덮어쓰기가 실패하면 기준이 이미 판 3이라 다시 묻지 않는 것이 맞음) "retries on the version I approved"로 바꾸었고, 이 시험은 기존 동작을 기록하는 것이라 `263e471`에서도 통과한다(고친 것으로 세지 않음). 나머지 9개는 진짜 실패였다.
- `late-refresh-fail-before.log`: 늦은 응답 시험 3개가 `263e471`에서 실패(확인이 사라짐, 초점 '').
- `load-server-focus-fail-before.log`: 서버 내용 불러오기 초점 단언이 수정 전에 실패.
- 단위 시험 "marks pairs that look the same"과 `assigneeLabels` 시험은 위 실행 뒤에 넣었다. 수정 전 실패 근거는 같은 로그의 컴포넌트 시험("says so …", "tells apart people …")이다.
- "asks again with the newer value when overwriting conflicts again"은 기존 동작의 빈 곳을 메우는 시험이라 수정 전에도 통과했다.

## `263e471`(Esc) 독립 반박 재검증

서브에이전트에 코드만 주고 격리 worktree(`263e471`)에서 탐침 34개를 돌렸다(31 통과, 의도된 실패 3).

- 반례(고침): 확인이 뜨기 전에 보낸 체크리스트·상위 업무(반복도 같은 경로) 요청이 확인 중에 더 새 판으로 돌아오면 확인이 말없이 닫히고, 초점이 상자 밖이라 다음 Esc가 업무 대화상자를 닫아 입력을 잃는다. 원인은 그 경로들이 `busy`를 세우지 않아 확인이 열릴 수 있고, `refresh` → `applyMerge`가 `clashAsk`를 지우는 것이다. 위 2번으로 고쳤다. 같은 탐침을 고친 소스에 복사해 돌리니 반례 3개가 통과했다(`verifier-probes-on-fix.log`, 34개 중 33개 통과). 남은 1개 실패는 "확인이 사라진다"는 옛 동작을 단언하는 탐침이라 실패가 맞다. `verifier-probes-on-263e471.log`에는 수정 전 3개 실패가 있다. 탐침 사본은 지웠다.
- 통과: 상자 안 어디서든 Esc(제목·상자·두 단추·값 상자·안내문). 취소, 덮어쓰기 → 다시 409, 덮어쓰기 → 네트워크 실패. 덮어쓰기 두 번 누름(요청 1번). 제목에서 Enter.
- 기존 동작(확인이 없어도 같음): Tab/Shift+Tab으로 "서버 내용 불러오기"·닫기(X)에 간 뒤의 Esc, 대화상자 내용·body 초점의 Esc, 충돌 문장 클릭 뒤 Esc, 바깥(오버레이) 클릭, 대화상자 닫았다 다시 열기(`{#key openTaskId}`로 다시 만듦), 덮어쓰기 요청 중 Esc. 모두 업무 대화상자를 닫아 저장하지 않은 입력을 잃는다. bits-ui의 Esc 층은 문서 전체에서 듣는다. 확인 고유의 문제가 아니라 대화상자의 미저장 입력 보호가 없는 문제라 범위 밖으로 둔다(필요하면 별도 결정).
- 설계 메모: `ClashConfirm`의 `busy` 분기는 닿지 않는다. 덮어쓰기가 `busy`를 세우기 전에 확인을 닫기 때문이다. 해가 없어 그대로 둔다.
- jsdom 한계: 클릭에 따른 초점 이동과 Tab 순서는 jsdom이 하지 않아 `.focus()`로 흉내 냈다. 실제 브라우저 확인이 필요하다.

## 검사 결과 (`1640dbc`)

| 명령 | 결과 | 로그 |
|---|---|---|
| `pnpm install --frozen-lockfile` | PASS | — |
| `pnpm run format:check` | PASS | `final-format-check.log` (amend 뒤 다시 실행: exit 0) |
| `pnpm run i18n` | PASS | `final-i18n.log` |
| `pnpm exec eslint .` | PASS | `final-eslint-1640dbc.log` |
| `pnpm exec svelte-check --tsconfig ./tsconfig.json --fail-on-warnings` | PASS (0 errors, 0 warnings) | `final-svelte-check.log` (amend 뒤 다시 실행: 0/0) |
| `pnpm exec vitest run` | PASS 77 files / 430 tests | `final-vitest-1640dbc.log` |
| 실제 브라우저 확인 | NOT RUN (coordinator가 최종 통합본에서) | — |
| `cargo xtask check` / `cargo xtask test` | NOT RUN (Rust·서버 변경 없음, 인수 기준은 웹 검사) | — |

로그 위치: worktree의 `target/audit-evidence/w2-pr82/` (git 무시 대상).

## 남은 것

- 실제 브라우저 확인(NOT RUN). 특히 같은 이름 표시, 늦은 응답 뒤 초점, 값 상자 클릭 뒤 Esc.
- Safari 값 상자 키보드 스크롤(lint 규칙 완화가 필요해 보고만 함).
- 업무 대화상자의 미저장 입력 보호(Esc·바깥 클릭·Tab으로 나간 뒤 Esc). 확인이 없어도 같은 기존 동작이다.
