# W3 — PR #59 (D-94 일정) 남은 시험과 nit

- 시작: `df2261597ccc577abbf2b96c8682b5bb7b2f3c72` (origin/fix/audit-web-calendar-home)
- 커밋: `21d92f4f73541b4735f618f48987994c1357dc7e` — push `df22615..21d92f4` (fast-forward, 조상 확인 후)

## 변경

- `conflict.ts`: `trimLikeServer` 추가(유니코드 White_Space만 앞뒤로 지움 = 서버 Rust `str::trim`, `validate_title`·
  `validate_text`). `sameUnit`의 제목·설명 비교를 JS `trim()`에서 바꿈. JS `trim()`은 U+FEFF를 지우고 U+0085를 두어
  서버와 반대였다. 정규식 `^\p{White_Space}+|\p{White_Space}+$` 대신 반복문(긴 공백 입력에서 끝 앵커 되짚기 피함).
- `EventDialog.svelte` `unitText`: 공백만인 설명을 "(비어 있음)"으로 보임(`trimLikeServer(...) === ''`).
- 서버 계약·D-93 입력 규칙은 손대지 않음.

## 새 시험

`tests/component/event-conflict-merge.test.ts`
- trims the way the server does (U+FEFF ≠ 빈 값, U+0085·U+3000은 지움)

`tests/component/event-save-mine.test.ts`
- (a) overwrites onto the newer version a rule save returned after the question, without asking again
- asks again when the newer version a rule save returned also moved the time
- asks again when I change the clashing time after the question
- (b) loads the newer version a rule save returned while the notice was shown
- merges onto the event a rule conflict returned while the notice was shown
- shows a description of only spaces as empty in the question
- reopening drops the old notice and question

## 실패 확인

| 로그 | 소스 | 결과 |
|---|---|---|
| 01-new-tests-on-df22615.log | df22615 그대로 | trim 시험 2개만 FAIL, 나머지 24 PASS |
| 02-mutation-latest | `latest`가 늘 알림 판 | 더 새 판 시험 6개 FAIL((a)·(b)·규칙 409 포함) |
| 03-mutation-reask-on-any-new-version | 판이 바뀌면 늘 다시 물음 | (a) FAIL |
| 04-mutation-drop-server-arm | 확인에서 서버 값 비교 뺌 | "also moved the time" FAIL |
| 05-mutation-drop-mine-arm | 확인에서 내 값 비교 뺌 | "I change the clashing time" FAIL |
| 06-mutation-reopen-keeps-conflict | 다시 열 때 conflict 안 지움 | reopening FAIL |

(각 변이 로그의 "only spaces" 실패는 nit 수정 전이라 함께 난 것.)

## 덮임 확인(이미 있던 시험)

같은 단위 충돌(취소·덮어쓰기), 다시 409(더 새 판 표시·입력 보존·초점), 실패(입력·알림 보존, 같은 내용 재전송),
Escape(물음만 거둠), 검사에 걸린 제출, 늦은 결과(`event-dialog-late.test.ts`, 다시 저장도 같은 `save` 경로).

## 검사 (21d92f4)

| 명령 | 결과 |
|---|---|
| pnpm install --frozen-lockfile | PASS |
| pnpm run format:check (15) · prettier 변경 파일 (10) | PASS |
| pnpm run i18n (11) | PASS |
| pnpm exec eslint . (12) | PASS |
| svelte-check --fail-on-warnings (13) | PASS 0 오류 0 경고 |
| vitest run (14) | PASS 85파일 436개 |
| cargo xtask check / cargo xtask test | NOT RUN (웹만 바뀜, 위 웹 단계는 모두 실행) |
| 실제 브라우저에서 충돌 대화상자 확인 | NOT RUN (인계 3.2·CodeRabbit 요약이 남긴 항목, `cargo xtask dev`는 coordinator 전용) |

## #72 합치기 (임시 로컬 브랜치 `w3-merge-check-72`, push 안 함)

`git merge origin/fix/audit-delta-time-inputs`(0213eb8) → 충돌 없음(자동 병합 173e352). i18n·eslint·svelte-check·
vitest(85파일 453개)·format:check 모두 PASS (21~25). 합친 EventForm의 `submit`은 첫 await 전에 `onsubmit`을 부른다
(다시 저장의 `submitMode` 전제 유지). 임시 브랜치 `w3-merge-check-72`(병합 `173e352`)는 이 worktree에 로컬로만 남아
있다(push 안 함, 지우지 않음).

## CodeRabbit · CI

- `df22615`: 요약 댓글 "No actionable comments", Merge Risk Minimal, 리뷰 객체·인라인 새 지적 없음(`coderabbit-df22615-summary.md`).
- `21d92f4`: 상태 success "Review completed", 요약의 coveredCommitId = 21d92f4, "No actionable comments", Merge Risk
  Minimal, push 뒤 리뷰 객체·인라인 댓글 0건(`coderabbit-21d92f4-summary.md`).
- GitHub 검사(21d92f4): check·integration·oidc·supabase·licenses·rhwp 모두 success.
