# W6: #60 계정이 바뀐 뒤 문서 저장이 다른 계정으로 나가는 경로 (웹만)

- 브랜치: `fix/audit-web-shell-members-files` `d913314` → **`81c9d99`** → **`0345026`** (둘 다 fast-forward push, push 전 원격이 조상임을 확인).
- 근거: W4 `target/audit-evidence/w4/ledger-update.md` 4.1.
- 서버 계약 변경 없음. 실시간 문서(`LiveDocumentEditor`) 경로는 대상 밖.

## 변경

- `apps/web/src/lib/features/documents/DocumentEditor.svelte`
  - `save()`가 내용 검사(제목·붙여 넣기 차단) 뒤, 요청 id·보낼 내용을 정하기 **전에** `signedInAs()`로 확인한다(확인 중 바뀐 편집이 옛 요청 id로 나가지 않게).
  - `signedInAs()`: `api.me()`로 지금 계정을 묻는다. 결과 `same`(보냄) / `other`(보내지 않음, `status='dirty'`, `switchAccount()`) / `signed-out`(보내지 않음, 로그아웃 안내, `reportSessionExpired()` — 기존 401 저장 경로와 같은 상태) / `unknown`(`me()` 실패 또는 이 창의 계정을 모름: 보내지 않음, `retrying` 안내, `RETRY_DELAY_MS` 재시도 예약) / `gone`(묻는 사이 편집기가 사라짐: 아무것도 하지 않음 — 사라진 뒤 재시도 타이머를 다시 걸지 않게).
  - 직접 누른 저장에도 같은 확인을 적용했다. 이유: 그 화면도 앞 계정의 것이라 다른 계정 쿠키로 나가면 같은 문제이고, 확인이 실패하면 기존 연결 오류 경로와 같은 "재시도 대기"가 되어 사용자가 잃는 것이 없다. 비용은 저장마다 `GET /api/me` 한 번(느린 연결에서는 직접 누른 저장도 요청이 나가기 전 한 왕복 동안 "저장 중"으로 보인다).
- `apps/web/src/lib/features/feedback/feedback.svelte.ts`: `onAccountSwitch(reload)`(FeedbackHost가 다시 읽기 방법을 등록)와 `switchAccount()`(`markAccountSwitch()` 뒤 등록된 다시 읽기, 없으면 `window.location.reload()`).
- `apps/web/src/lib/features/feedback/FeedbackHost.svelte`: `$effect(() => onAccountSwitch(reload))` 한 줄(창 복귀 확인 경로는 그대로).
- A의 초안: 편집할 때마다 `edited()`가 `fvoci:draft:<A>:<문서>`에 남기고, 저장하지 않았으므로 지우지 않는다(시험 1이 확인).

## 시험 (`apps/web/tests/component/document-save-account.test.ts`, 가짜 타이머 setTimeout/clearTimeout)

| 시험 | 수정 전 `d913314` | 수정 후 `81c9d99` |
|---|---|---|
| (1) `does not send a waiting retry once another account signed in, and reloads for that account` — 재시도 대기 중 `/api/me`=u2 → `saveDocument` 다시 안 불림, FeedbackHost의 reload 1회, `fvoci:draft:u1:doc-1*` 남음, u2 키 없음 | **FAIL** (`expected "saveDocument" to be called 1 times, but got 2 times`) | PASS |
| (2) `keeps the retry waiting without sending when it cannot check who is signed in` — `/api/me` 연결 실패 → 저장 안 보냄, "재시도 대기", reload 없음; 다시 닿으면 기다리던 재시도가 보냄 | **FAIL** (같은 메시지) | PASS |
| (3) `sends the retry as before when the same account is still signed in` — 같은 id → 저장 보냄(같은 내용·기준 판), 초안 지움 | PASS | PASS |

수정 전 로그: `target/audit-evidence/w6/before-fix-d913314.log`(최종 시험 파일을 `d913314`의 세 소스 파일로 돌림).

## 명령과 결과

| 명령 | 결과 | 로그 |
|---|---|---|
| `pnpm install --frozen-lockfile` | PASS | — |
| `cd apps/web && pnpm run i18n` | PASS | `web-checks.log` |
| `pnpm exec eslint .` | PASS | `web-checks.log` |
| `pnpm exec svelte-check --tsconfig ./tsconfig.json --fail-on-warnings` | PASS | `web-checks.log` |
| `pnpm exec vitest run` | PASS (87 파일, 420 시험, `0345026`에서) | `web-checks.log` |
| `pnpm exec prettier --check`(바꾼 4개 파일) | PASS | — |
| `cargo xtask check` / `cargo xtask test` | NOT RUN (웹만 바뀜, 서버·DB 변경 없음) | — |

## 3. 보고만: DocumentEditor 밖에서 타이머·재시도로 서버를 바꾸는 `api.*` 호출

| 위치 | 호출 | 재시도 방식 | 다른 계정으로 쓰일 수 있나 |
|---|---|---|---|
| `apps/web/src/lib/features/files/upload.ts:251` | `api.putUploadPart` | `withRetry`(`:140-151`): NetworkError·5xx에 1·2·3초 뒤 최대 3번 | 아니오로 보임: 업로드 행은 만든 사람 것이고 `app_record_part`가 `user_id = app_user_id()`로 찾는다(`crates/server/migrations/0027_files.sql`, 함수 본문 9줄째). 다른 계정이면 404 |
| `apps/web/src/lib/features/files/upload.ts:262` | `api.completeUpload` | `finish()`(`:259-275`): 409이고 다른 요청이 마치는 중이면 3초 뒤 다시(최대 20번) | 아니오로 보임: `app_begin_complete`도 `user_id = app_user_id()`로 찾는다(`0028_files_review.sql:162` 함수 본문 10줄째). 다른 계정이면 404 |

쓰기가 아니라 제외한 타이머: `TaskBoard.svelte:789-797`(느린 이동 표시·`reconcile`은 `api.tasks` 읽기), `time/timer.svelte.ts:40`(경과 초 표시), `:121-128`(탭을 다시 볼 때 `api.currentTimer` 읽기), `feedback.svelte.ts:22`(`/healthz`), 검색·대상 고르기 지연 입력(읽기), `LiveDocumentEditor`(실시간, 대상 밖; 다시 연결은 단추).

## 댓글

- #60: https://github.com/fvoci/GroundZero/pull/60#issuecomment-6331297217 (요약 우려 https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473 에 수정 SHA로 답)
- #88: https://github.com/fvoci/GroundZero/issues/88#issuecomment-6331297568 (확인과 전송 사이 남는 경합 — 서버의 기대 사용자 머리 검사, 사용자 결정 후보)

## 남는 것

- `/api/me` 확인과 저장 전송 사이의 짧은 경합(서버 계약 변경이 있어야 완전히 닫힘, 사용자 결정 후보, #88에 기록).
- 다른 화면의 사용자 조작(예: 타이머 시작·정지)은 창 복귀 확인 전까지 앞 계정 화면에서 새 계정으로 나갈 수 있다(사용자가 누른 것, 이번 범위 밖).

## CodeRabbit 후속 (push 뒤)

- `81c9d99` 리뷰(https://github.com/fvoci/GroundZero/pull/60#pullrequestreview-5608659609): Outside diff 1건 🟠 Major — "초안을 보관하지 못했으면 계정 전환 시 경고를 유지"(`DocumentEditor.svelte:412`). 유효(저장소 쓰기 실패 때 다시 읽기가 경고 없이 입력을 버림 = 데이터 유실) → **FIXED `0345026`**: 다시 읽기 직전(`beforeunload`)에 지금 입력을 앞 계정의 초안으로 저장하고, 성공했을 때만 경고를 생략. 남아도 저장은 계정 확인에서 막힌다.
  - 시험: `account-switch-unload.test.ts` `still warns when the edit could not be kept as a draft for the first account`(`Storage.prototype.setItem`이 던짐). 수정 전(`81c9d99`) FAIL `expected false to be true` — 로그 `before-fix2-81c9d99.log`. 수정 후 PASS.
  - 답글: https://github.com/fvoci/GroundZero/pull/60#issuecomment-6331495897
- `0345026` 리뷰: `success Review completed`(03:04:28Z), 새 리뷰 객체·인라인 없음, 풀리지 않은 스레드 0. 요약: Merge Risk ⚪ Minimal · Security 🔵 Low up to `03450`, Retained concerns: none. 남은 `_proposed_`(계정 전환 상태를 전환 범위로 한정하고 다시 읽기가 취소됐을 때의 회복 상태 정의)는 이슈 #88의 "새로 읽기가 막히거나 실패할 때의 안전 상태" 항목과 같다 → FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/88).
- 그 뒤 CodeRabbit이 자동으로 **Reviews paused**(마지막 검토 커밋 `0345026` = 지금 머리, 검토 안 된 커밋 없음). 다음 push가 있으면 `@coderabbitai resume`이 필요하다.

## 커밋

- `81c9d99` fix(web): check the signed-in account before a document save goes out (review)
- `0345026` fix(web): keep the unsaved-edit warning on an account switch when the draft cannot be kept (review)

## 판단 기록

- 계정이 다르다고 확인된 뒤 사용자가 다시 읽기를 취소하면(초안을 남기지 못해 경고가 뜬 경우) 편집기는 `dirty`로 남고, 다음 편집의 자동 저장이 다시 `other`를 받아 다시 읽기를 다시 묻는다. 저장은 나가지 않는다. `switchAccount()`에 "이미 전환 중이면 무시"를 넣지 않은 이유: 넣으면 취소한 뒤 앞 계정의 화면이 조용히 저장되지 않는 상태로 남는다. 다시 묻는 편이 안전하다. 취소 뒤의 회복 상태 정의는 #88 항목.
