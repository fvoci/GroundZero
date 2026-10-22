# 코드 전반 감사 인계 (audit-resume)

**상태: 원격 자료 준비 완료, 수신 측 인수 확인 대기.** 새 환경에서 이 문서의 0절(인수 확인)을 마치기 전까지는 인계가 끝난 것이
아니다.

- 작성: 2026-10-22, 기존 환경(macOS arm64 데스크톱의 Claude Code 세션)
- 원격 상태를 마지막으로 확인한 시각: 2026-10-22T01:24:17Z
- 이 문서가 있는 브랜치: `handoff/audit-resume` (main `a5cda4e` 위, 이 문서와 `docs/handoff/` 아래 파일만 더함). 병합하지 않는다.
- 기존 환경은 인계 체크포인트 뒤 이 저장소의 브랜치·PR·원격 DB를 더 고치지 않는다. 인계 중에 돌던 작업(D-95 구현)은
  멈췄고, 읽기 전용 검증·수집만 끝까지 돌렸다. 새 환경은 0절에서 이를 확인한 뒤 시작한다.

## 0. 새 환경이 가장 먼저 할 일 (인수 확인)

1. 저장소를 새로 clone하고 `git fetch origin '+refs/heads/*:refs/remotes/origin/*'`.
2. 1절 표의 브랜치마다 `git rev-parse origin/<브랜치>`가 표의 전체 SHA와 같은지 확인한다. 다르면 누가 언제 올렸는지 PR 기록으로
   확인하고, 기존 환경의 작업이 아니면 그대로 진행해도 되는지 사용자에게 묻는다.
3. `docs/handoff/evidence/`에서 `shasum -a 256 -c SHA256SUMS`(Linux는 `sha256sum -c SHA256SUMS`)를 돌려 모두 OK인지 본다.
   `MANIFEST.md`의 파일 목록과 실제 파일이 같은지 본다.
4. 9절의 "로컬에만 남은 것"이 새 환경에 필요한지 판단한다(필요한 것은 없게 만들었다. 원본 로그는 공개 저장소에 올리지 않았다).
5. 인수 결과(일치/불일치, 누락)를 사용자에게 알린다. 기존 환경이 더 고치지 않는지(이 문서 머리말) 사용자에게 확인받은 뒤 작업을 시작한다.

## 1. 원격 상태 (2026-10-22T01:24:17Z 확인)

- 저장소: `fvoci/GroundZero`(공개). main: `a5cda4e169e237c6a33075fdde21c4b20fec8a1a`(Merge PR #48). 감사 PR은 아직 하나도
  병합하지 않았다. main에 보호 규칙은 없다(`branches/main/protection` 404). 병합은 기존 방식의 merge commit.
- 모든 감사 PR은 Draft다. Ready 전환·병합·태그·배포·SQL 말아올리기는 이번 인계 범위 밖이며 새 환경이 최종 검증 뒤에 한다.

### 1.1 PR과 브랜치

"이번 인계에서 올림"은 2026-10-22 인계 체크포인트에 기존 환경이 push한 커밋이다. 표시가 없는 PR의 머리는 그 전부터 원격에
있었다(이전 보고의 "push하지 않았다"는 그때 진행 중이던 이번 인계분만을 가리킨다).

| PR  | 브랜치                              | base                       | 원격 HEAD 전체 SHA                         | 마이그레이션         | 의존·메모                                                                          |
| --- | ----------------------------------- | -------------------------- | ------------------------------------------ | -------------------- | ---------------------------------------------------------------------------------- |
| #51 | `fix/audit-trash-restore`           | main                       | `eb881939efc85fe21556a7906fca6f320c0093bf` | 0080                 |                                                                                    |
| #52 | `fix/audit-membership-locks`        | main                       | `591f93917e5f97eba436036810c3c6c4118a8004` | 0081                 |                                                                                    |
| #53 | `fix/audit-schedule-links`          | main                       | `3fc48a5a7fd04209869c1f6aa58f39777c9099b9` | 0082                 | #62의 base                                                                         |
| #54 | `fix/audit-time-events`             | main                       | `4c15e3a072c4e329ea60c89428402713c4b7ace8` | 0083                 |                                                                                    |
| #58 | `fix/audit-web-items`               | main                       | `c9a2bceb6359a643d106940c86aaac233085e2c2` | 0084                 | 서버 변경 있음(records.rs 판 검사·`validate`의 `kept` 인자)                        |
| #57 | `fix/audit-web-documents`           | main                       | `40f54cb9ffaf7fcdecfb00a2d2ce626bfc8943fa` | 0085                 |                                                                                    |
| #59 | `fix/audit-web-calendar-home`       | main                       | `df2261597ccc577abbf2b96c8682b5bb7b2f3c72` | 0086                 | 이번 인계에서 올림: `d30bd03`, `df22615`(D-94 일정)                                |
| #62 | `fix/audit-d92-cleanup`             | `fix/audit-schedule-links` | `26615d1493b5579b9d5df518cde8f3c992916ae1` | 0087                 | #53 위. #53 병합 뒤 base를 main으로                                                |
| #65 | `fix/audit-upload-claim-release`    | main                       | `540097718c4c3ab6913624175a5dac0014b38e3f` | 0088                 |                                                                                    |
| #66 | `fix/audit-recurrence-lane-order`   | main                       | `611e6644adbedc34f1825eb5f116f60853360dc3` | 0089                 |                                                                                    |
| #67 | `fix/audit-search-claim-recheck`    | main                       | `e3d0df79c398c3c5c070ae8e44f9a4e873f7b18f` | 0090                 |                                                                                    |
| #74 | `fix/audit-delta-search`            | main                       | `01f028fa3550b749af12a85efe5a63cef21e209f` | 0091                 |                                                                                    |
| #75 | `fix/audit-delta-time-pause`        | main                       | `235e95d59c0fad6e82754b6d3b9c24ed578add95` | 0092                 |                                                                                    |
| #81 | `fix/audit-stale-version-reload`    | main                       | `bf5569669060a501cf67fff53dad28309843ccd4` | 0093                 | Closes #77                                                                         |
| #55 | `fix/audit-time-summary`            | main                       | `a34a5300ca93e6f08a924869c34a795481f744c2` | —                    |                                                                                    |
| #56 | `fix/audit-web-csv-import`          | main                       | `1d7f9fc8b159686fa8e392992cc9791275f41c0e` | —                    |                                                                                    |
| #60 | `fix/audit-web-shell-members-files` | main                       | `d913314a94a9627221f5dd83a28e16c78f968a41` | —                    |                                                                                    |
| #61 | `fix/audit-home-up-next`            | main                       | `97447d83787259e9a25a3e7a8327fc57140e5522` | —                    | 서버 변경(home.rs)                                                                 |
| #63 | `fix/audit-idempotent-retries`      | main                       | `324d6e1d69057d06776130909dff5680af1911cb` | —                    | 서버 변경(records.rs)                                                              |
| #64 | `fix/audit-move-live-snapshot`      | main                       | `5edd8f4545e1aca1f9e78376f4477b87ba4f81bc` | (0094 예정, WIP에만) | 이번 인계에서 올림: `d495c65`(#71), `5edd8f4`(P-34). D-95 조건 2 미완 → WIP        |
| #68 | `fix/audit-test-gaps`               | main                       | `cd2c1bddaf769b395d3352cd4499d682c50c9f85` | —                    | 시험·문서                                                                          |
| #72 | `fix/audit-delta-time-inputs`       | main                       | `0213eb8be9298e57c4d9222d681be08ce6b6f653` | —                    | D-93, Closes #78                                                                   |
| #73 | `fix/audit-delta-gantt-search`      | main                       | `067ef3630b9e49579abf436a8508693807640edb` | —                    |                                                                                    |
| #80 | `fix/audit-display-settings-reset`  | main                       | `48c00871864dd8270c97f6e324fcace52d8cfe5e` | —                    |                                                                                    |
| #82 | `fix/audit-task-conflict-merge`     | main                       | `263e47163b29c68fa39e308d2af7c60a3e29944a` | —                    | 이번 인계에서 만든 새 Draft PR(D-94 업무). `6cee356`·`8ba72ad`·`70435e4`·`263e471` |
| #69 | `docs/audit-record`                 | main                       | `6ed1df82f28898c74501646f98fc9ba8455b37f8` | —                    | 감사 기록. 이번 인계에서 올림: `06df978`, `6ed1df8`(D-94·D-95). 마지막에 병합      |

PR이 아닌 원격 브랜치(이번 인계에서 올림, 병합하지 않는다):

| 브랜치                             | 전체 SHA                                   | 내용                                                                                                                                          |
| ---------------------------------- | ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `wip/audit-d95-stand-in-record`    | `ab0ffaf317e5b81f806b1884321a9edfd4339d5a` | D-95 구현 도중 멈춘 스냅숏(`5edd8f4` 위 한 커밋, 0094 포함). **미완성·미검증**                                                                |
| `handoff/ref-integration-1`        | `7d6775c59bca594f031ed207afa1b00e34cfbdc5` | 첫 통합(로컬 `audit/integration`). R0·R1의 `634019f`, `912c48a`, R2의 `7d6775c`를 포함. 통합 전용 고정 값 커밋 `912c48a`                      |
| `handoff/ref-integration-2`        | `36f52d7b596b7bdde4f88755aae44d56c464d770` | 둘째 통합. R3 `2286a9d`(통합 전용 고정 값 커밋), R4 `a0a45bb`, R6 `36f52d7`                                                                   |
| `handoff/ref-integration-3`        | `d7ea7fc7044c668831436d15d0b55e2306d50a58` | 셋째 통합(#64 없음). R7 `156b8d0`(통합 전용 고정 값 커밋), 실제 브라우저 확인 `d7ea7fc`                                                       |
| `handoff/ref-integration-4-prefix` | `7e7137159efeb14a753bfc7c8e5ab9d30edf19af` | 병합 순서대로 다시 짓기 시작한 통합의 앞부분(#51→#52→#53→#54→#58→#57, 모두 충돌 없음)                                                         |
| `handoff/ref-int4-dry`             | `cd2d9b62b478f0d7675cf20795f4a6b8fe68a957` | 위 순서를 #80까지 지금(인계 전) 머리로 미리 합쳐 본 것. #64는 옛 머리 `2e1d3a1`, #59는 옛 머리 `dbd1861`, 고정 값 수정 없음. 충돌 해결 참고용 |

후속 이슈(모두 OPEN, 2026-10-22T01:24:17Z): fvoci/GroundZero#50, #70, #71, #76, #77, #78, #79. #77은 #81, #78은 #72, #71은 #64가 병합되면
닫힌다(#64는 D-95를 마친 뒤 본문에 `Closes #71`을 넣는다).

## 2. 결정 (확정·해석·미확정)

원문은 `docs/decisions.md`(#69 브랜치 `6ed1df8`)에 있다. 여기서는 요지만 옮긴다.

### 2.1 사용자가 확정한 결정

- **D-93**(2026-10-22, #78·#72): 일정과 시간 기록을 고칠 때 손대지 않은 시작·끝은 저장된 시점과 초를 그대로 보존한다. 고친 시각이
  그 시간대에서 두 번 있는 시각이면 저장 전에 묻고, 선택지는 "첫 번째 01:30 (UTC−04:00)"·"두 번째 01:30 (UTC−05:00)"처럼 순서와
  UTC 차이를 함께 보인다. 끝이 시작보다 앞서면 서버가 400으로 거절한다. 근거: 사용자 선택 "보존 + 명시 선택"과 선택 문구 지시.
- **D-94**(2026-10-22, #82 업무·#59 일정): 사용자 원문 — "내가 수정한 칸만 적용하고, 나머지는 서버 최신 값을 유지하세요. 같은
  칸을 다른 사람도 수정했다면 양쪽 값을 보여주고 덮어쓸지 확인받으세요. 일정의 날짜·시각·시간대·종일 여부는 한 묶음으로
  처리하세요. 재저장은 최신 판 번호를 기준으로 서버에서 검사하고, 그사이 또 변경됐다면 다시 409로 처리하세요. 실패해도 내 입력은
  보존하세요." 문서 본문 충돌(D-53, 자동 3-way 병합 제외 X-11)은 그대로다.
- **D-95**(2026-10-22, #71·P-34, #64): 사용자 선택 "옮긴 사람이 대신 저장"(열린 방과 방이 닫힌 뒤 남은 저장 대기 상태 모두)과
  조건 — "대신 저장은 이동 권한을 검증한 서버 내부 처리로 한정하고, 편집자의 권한을 늘리지 않습니다. 실제 편집자와 이동·대리
  저장한 사람을 기록에서 구분합니다. 옮긴 사람이 내용을 직접 작성한 것처럼 표시하지 않습니다. 저장 대기 편집을 안전하게 남긴 뒤
  이동합니다. 저장에 실패하면 이동을 409로 거절하고 편집은 보존합니다. 저장과 이동 사이에 들어온 새 편집도 유실되지 않도록
  처리하고, 이동 실패·취소 경로까지 시험합니다." P-34는 이 결정으로 채택됐다.

### 2.2 구현자의 해석 (사용자 결정 밖 — 새 환경이 필요하면 사용자에게 다시 확인)

- D-93: 시간대를 바꾸면 두 끝 모두 새 시간대로 다시 읽으므로 "고친 시각"으로 본다. 새로 만들 때의 겹친 시각은 D-81대로 앞선 시각.
- D-94 업무(#82): 업무의 시작일·마감일·마일스톤을 한 묶음으로 본다(일정 묶음과 같은 이유). 담당자는 순서 없이 집합으로, 태그는
  순서까지 비교한다(서버 `validate_tags`가 순서를 지킴). 제목·설명은 서버처럼 앞뒤 공백을 지운 값으로 충돌을 판정한다.
- D-94 일정(#59): 묶음 = {all_day, start_date, end_date, starts_at, ends_at, timezone}. 시각은 문자열이 아니라 시점 값으로 비교한다.
  알림 뒤 반복 규칙 저장으로 더 새 판이 생기면 그 판(`latest`)에 합친다. 충돌 알림의 저장 단추가 아닌 폼의 보통 저장은 여전히 옛
  판 번호로 보내 다시 409가 된다(안전하지만 병합하지 않음).
- D-95: 리비전의 `author`는 그 리비전을 쓴 트랜잭션의 사람(대신 저장한 사람)으로 두고(삽입 정책 `author = app_user_id()` 유지),
  작성자가 지난 리비전 뒤의 편집자에 없으면 실제 편집자를 새 열 `edited_by`에 적는다(편집자를 모르는 옛 저장 대기 상태는 빈
  배열). 같은 규칙이 main에 이미 있는 "실시간을 끄는 관리자의 대신 저장"에도 적용된다. 방 없는 **복원**은 대신 저장하지 않는다(방
  안 복원과 같게: 저장할 수 있는 편집자가 없으면 409). 이 해석과 0094는 WIP에만 있다.

### 2.3 미확정 제안 (승인 전 구현 금지)

`docs/decisions.md` 2절의 P-24(검색 날짜 색인 시간대), P-25(작성자 필터 후보 300 한계), P-26(저장한 보기 동시성), P-27(진행 중
타이머 시작 변경 검증), P-28(보관된 항목의 자기 댓글 지우기), P-29(시간 기록 길이 상한), P-30(항목 제목·설명 판 검사), P-31(업무
한 번에 만들기), P-32(초안 옛 키 옮기기 제거), P-33(보관 항목 복제 소유 권한 예외), P-35(반복 회차 맞추기 트랜잭션 분할),
P-36(CI `integration` job 정리). SQL을 바꾸는 제안(P-24·P-25·P-26·P-28·P-29·P-30·P-31·P-35)은 SQL 말아올리기 전에 결정하는 편이
좋다(#69 95절).

## 3. 진행 중이던 작업의 상태 (인계 체크포인트)

### 3.1 D-94 업무 — PR #82 (`263e471`)

- 완료: 내가 고친 칸만 최신 판에 얹기, 같은 칸 충돌 확인(양쪽 값 전체 표시), 일정 묶음, 담당자 집합 비교, 확인 중 나머지 잠금
  (inert), Esc는 확인만 취소, 다시 409·실패에서 입력 보존, 초점 복귀. jsdom 시험 38개(두 파일), 전체 vitest 413개 통과, eslint·
  svelte-check(`--fail-on-warnings`)·prettier 통과. 고치기 전 실패: main `a5cda4e`의 TaskDetail로 5/6 실패, 1차(`8ba72ad`)로 18/38 실패.
- 1차(`8ba72ad`) 반박 검증: **실패**(일정 칸별 병합, 확인 값≠보낸 값, 담당자 순서, 긴 글 자름, 초점·잠금). 2차(`70435e4`)가 모두 고쳤다.
- 2차 반박 검증: **통과 아님**(인계 중 완료, 읽기 전용). D-94 병합 자체(일정 묶음·담당자 집합·태그 순서·예상 시간)에서는 반례가 없었고 검사·vitest 413개가 통과했다. 그러나 [중간, 병합 차단] 확인 상자의 버튼이 아닌 곳을 눌러 초점이 업무 대화상자(`Dialog.Content`)로 간 뒤 Esc를 누르면 업무 대화상자가 닫히고 저장하지 않은 입력이 사라진다(고칠 방향: 확인 뿌리에 `tabindex="-1"`, 또는 확인이 떠 있는 동안 `onEscapeKeydown`에서 `preventDefault`. 회귀 시험은 확인이 뜬 뒤 대화상자에 초점 → Esc → 입력 유지). [낮음] 진행 중이던 체크리스트·반복·상위 업무 요청이 돌아오면 확인이 말없이 닫힘, 앞뒤 공백만 바꾼 칸을 고친 칸으로 봄, 태그 중복 제거를 따르지 않음, 여전히 똑같아 보이는 값 쌍, 값 상자 키보드 스크롤(Safari). [nit] 초점 이동 조건·기존 초점 문제·309자리 예상 시간. 전체 목록은 #82 댓글 https://github.com/fvoci/GroundZero/pull/82#issuecomment-6329993004 .
- **Esc 입력 유실 수정(`263e471`, 인계 체크포인트에서 함)**: 확인 상자 뿌리에 `tabindex="-1"`(outline-none)을 달아 안내문·값 상자를 눌러도 초점이
  상자에 남고 Esc는 확인만 거둔다. 회귀 시험 "keeps focus in the question after a click on its text, so Escape only cancels it"는 `70435e4`에서
  실패(`evidence/logs/d94-tasks/263e471-esc-fail-before-on-70435e4.log`)하고 지금 통과. prettier·eslint·svelte-check(`--fail-on-warnings`)·vitest
  77파일·414개 통과(`263e471-vitest.log`). 반박 재검증과 실제 브라우저 확인은 하지 않았다. 대화상자 바깥(충돌 알림 글)을 눌러 초점이
  `Dialog.Content`로 간 뒤의 Esc는 지금처럼 대화상자를 닫는다(확인이 떠 있지 않을 때와 같은 기존 동작).
  설명은 https://github.com/fvoci/GroundZero/pull/82#issuecomment-6330375703 .
- 남은 것: 2차 검증의 [낮음]·nit 지적, `263e471`의 반박 재검증, 실제 브라우저 확인(NOT RUN), 최종 통합 시험. CodeRabbit은 `70435e4`에서
  "No actionable comments", 합병 위험 Minimal. `263e471`의 리뷰 상태는 7절 표를 본다.
- 알려진 범위 밖: 화면에서 같은 태그를 두 번 치면 클라이언트는 겹침을 빼지 않아 고친 칸·충돌로 볼 수 있음. 값 칸의 스크롤 영역은
  Safari에서 키보드 초점을 받지 않음(tabindex는 lint 경고, 규칙 완화는 사용자 확인 대상).

### 3.2 D-94 일정 — PR #59 (`df22615`)

- 완료: 충돌 알림의 "내 입력으로 다시 저장"은 지금 입력 양식을 제출(EventForm 검사 포함)해 내가 고친 단위(종류·제목·설명·일시
  묶음)만 최신 일정에 얹어 최신 판 번호로 보낸다. 같은 단위 충돌 확인, 취소·다시 409·실패에서 입력 보존, Esc는 확인만 취소, 초점
  복귀, 규칙 저장 뒤 더 새 판 사용, 제목·설명 앞뒤 공백 비교. vitest 85파일·428개 통과.
- 1차(`d30bd03`) 반박 검증: 통과(Esc가 대화상자를 닫아 입력을 잃는 문제 등은 2차에서 고침). 2차(`df22615`) 반박 검증: **통과**. 남은
  지적: 회귀 시험 빈칸 둘(알림 뒤 규칙 저장 → 덮어쓰기가 다시 묻지 않는 경로, "서버 내용 불러오기"가 더 새 판을 쓰는 경로; 임시
  탐침으로는 통과 확인), nit 둘(공백만인 내 설명이 확인에서 "(비어 있음)"으로 보이지 않음 — `description.trim() === ''`로 바꾸면 됨,
  U+FEFF를 JS trim은 지우고 Rust는 안 지움).
- #72(D-93, EventForm 재작성)와 합쳐 본 것: 2차 반박 검증이 임시 worktree에서 합쳐 일정 시험을 돌렸다(결과는 증거 묶음 밖, 에이전트
  보고만). 최종 통합에서 다시 확인해야 한다.
- 남은 것: 위 회귀 시험 둘 추가, 실제 브라우저 확인, 최종 통합 시험, CodeRabbit의 `df22615` 리뷰(진행 중).

### 3.3 #71·P-34·D-95 — PR #64 (`5edd8f4`) 와 WIP (`ab0ffaf`)

- 완료·검증(#64 브랜치):
  - `d495c65`(#71): 열린 방이 이동을 자기 명령 안에서 한다(체크포인트 → 이동 트랜잭션 → 커밋 전에 받은 편집만 → 리비전 → 4409).
    저장 확인(메시지 100)은 리비전을 남긴 뒤에만 보내고, 남기지 못하면 연결을 먼저 닫고 상태만 저장한다(브라우저 초안). 이동이
    일어나지 않으면 기다린 편집을 평소처럼 받는다. 방이 없으면 이동 중 새 방이 열리지 않는다(4503). 수정 전 코드에서 재현(확인받은
    편집이 옮긴 문서에 없음), `cargo xtask test --keep` 전체 통과, 이동 시험 반복 통과, 반박 검증에서 손실 경로 없음.
  - `5edd8f4`(P-34 + 검토 반영): 방 없이 남은 저장 대기 상태를 옮기기 전에 리비전으로 남기고 실패하면 409. 체크포인트 전 요청 끊김
    확인, 이동 뒤 Shutdown에 실제 결과로 답함, 시험의 고정 대기를 핑/퐁으로. 수정 전 실패 확인, `cargo xtask test` 286개 통과,
    `cargo xtask check --skip-licenses` 통과, 반박 검증 통과.
- **미완(병합 차단)**: D-95 조건 2(실제 편집자와 대리 저장자 구분)는 #64 브랜치에 없다. 지금 #64의 대신 저장은 옮긴 사람을 작성자로만
  적는다. 이동 취소 경로 시험, 방 없는 복원의 저장 대기 상태 보존(main에도 있는 빈틈)도 없다.
- WIP `wip/audit-d95-stand-in-record`(`ab0ffaf`, 부모 `5edd8f4`): 위 미완 부분을 구현하던 중 인계 때문에 멈춘 스냅숏. 0094
  `crates/server/migrations/0094_collab_stand_in_editors.sql`(`document_revisions.edited_by uuid[]`, `document_collab_states.editors`,
  `app_system_collab_store`에 편집자 인자, `app_collab_load`가 편집자 반환), room.rs·revisions.rs·documents.rs, OpenAPI·생성 클라이언트,
  RevisionHistory·ExportDialog 표시, 시험 일부. 확인한 것: `cargo check -p fvoci-server --tests` 통과, svelte-check 0 오류, 웹 시험 2파일
  6개 통과. **확인하지 않은 것**: DB 통합 시험, collab 시험, 취소 경로·복원 시험 작성 여부, 반박 검증. 검증 완료로 보지 말 것.
  **주의**: 멈추기 직전(09:36 KST) WIP 작업 트리의 collab 시험에서 27개 통과·1개 실패 기록이 있다. 실패한 것은 `5edd8f4`에서 통과하던
  P-34 시험 `a_move_without_a_room_saves_a_live_state_left_pending`이고, 409 "live edits could not be saved as a revision first"였다
  (`evidence/logs/d95-wip/test1.log`). **인계 체크포인트에서 스냅숏 `ab0ffaf`를 다시 돌린 결과(확정)**: `cargo xtask test --keep`(PostgreSQL,
  OIDC 없음)에서 collab 27개 통과·1개 실패로 같은 P-34 시험이 같은 409로 실패했고, cargo가 그 시험 바이너리에서 멈춰 뒤의 시험
  바이너리는 돌지 않았다(`evidence/logs/d95-wip/ab0ffaf-xtask-test-keep.log`). 곧 **WIP에는 확인된 회귀가 있다**. 원인은 아직
  조사하지 않았다(새 환경의 첫 D-95 작업).
- **CodeRabbit의 `5edd8f4` 새 지적(OPEN, 병합 전 처리)**: (1) 방 안에서 도는 `move_in_db`가 `app_lock_move`·`FOR UPDATE`·`app_lock_item`을
  `lock_timeout` 없이 기다려 방과 보내는 쪽이 끝없이 멈출 수 있고, 55P03이 409로 바뀌지 않는다
  (https://github.com/fvoci/GroundZero/pull/64#discussion_r4343460648). (2) 그 기다림 동안 방의 권한 재확인이 돌지 못해, 커밋 전에 받은 편집이
  예전 `can_write`로 들어가 세션이 끝났거나 권한을 잃은 작성자의 편집이 저장될 수 있다(요약의 남은 우려, 보안 Medium 추정,
  https://github.com/fvoci/GroundZero/pull/64#issuecomment-6320685284).
- 5edd8f4 반박 검증의 비차단 지적(WIP에서 다루던 중): 새 시험이 방이 실제로 닫혔는지 확인하지 않음, 검사 실패처럼 영구적인 원인도
  "try again" 409, Shutdown 답이 경합 창을 줄일 뿐 없애지 않음(확인받지 않은 편집만 해당), 출발지 끄기의 Shutdown에도 이동 뒤 결과로
  답함, 문서 nit(MoveRequest 주석, 이동 409 OpenAPI 설명).

### 3.4 그 밖의 결함 기록

- 실제 브라우저 확인(`d7ea7fc`)의 업무 409 "내 입력으로 다시 저장" **FAIL**(탭 A가 바꾼 제목을 탭 B가 옛 값으로 되돌림, main에도
  있음) → D-94·#82로 수정 중. 이 FAIL은 수정 뒤 최종 SHA에서 다시 확인해야 한다.

## 4. 마이그레이션 0080→0094와 병합 순서

| 번호 | 실제 파일                                               | PR                                                    |
| ---- | ------------------------------------------------------- | ----------------------------------------------------- |
| 0080 | `crates/server/migrations/0080_trash_restore_races.sql` | #51                                                   |
| 0081 | `0081_membership_locks.sql`                             | #52                                                   |
| 0082 | `0082_shift_plan_links.sql`                             | #53 (#62에도 보임: #53 위에 있어서)                   |
| 0083 | `0083_missing_rule_checks.sql`                          | #54                                                   |
| 0084 | `0084_item_visibility_on_change.sql`                    | #58                                                   |
| 0085 | `0085_document_settings_on_change.sql`                  | #57                                                   |
| 0086 | `0086_event_rule_base.sql`                              | #59                                                   |
| 0087 | `0087_drop_replaced_time_zone_setter.sql`               | #62                                                   |
| 0088 | `0088_upload_claim_release.sql`                         | #65                                                   |
| 0089 | `0089_recurrence_tick_lane_order.sql`                   | #66                                                   |
| 0090 | `0090_search_claim_recheck.sql`                         | #67                                                   |
| 0091 | `0091_item_search_on_change.sql`                        | #74                                                   |
| 0092 | `0092_running_timer_pause_range.sql`                    | #75                                                   |
| 0093 | `0093_stale_version_reload.sql`                         | #81                                                   |
| 0094 | `0094_collab_stand_in_editors.sql`                      | #64 (지금은 WIP 브랜치에만. D-95를 마쳐 #64에 넣는다) |

- 원래 지시의 "0080→0092"는 #81(0093)과 D-95(0094)가 더해져 **0080→0094**가 됐다. #64는 #81 뒤에 병합해야 0093 다음에 0094가 온다.
- 권장 병합 순서(첫 부모 기준): #51 → #52 → #53 → #54 → #58 → #57 → #59 → #62(#53 병합 뒤 base를 main으로) → #65 → #66 → #67 →
  #74 → #75 → #81 → #55 → #56 → #60 → #61 → #63 → #64 → #68 → #72 → #73 → #80 → #82 → #69(마지막). 이 순서로 미리 합쳐 본
  결과는 `handoff/ref-int4-dry`(옛 #59·#64 머리 기준). 충돌은 #59, #66, #74, #55, #63, #68, #80에서만 났고 아래 6절의 해결과 같다.
- 적용된 마이그레이션 파일은 고치지 않는다(체크섬). 0081·0086·0088·0089·0090·0092는 병합 전에 같은 파일을 리뷰로 고친 적이 있어 그
  브랜치를 미리 적용한 개발·시험 DB는 다시 만들어야 한다(공유·운영 DB에는 적용된 적 없음).

## 5. 통합 때 보존할 양쪽 변경과 충돌 해결

해결 예시는 `docs/handoff/evidence/patches/integration-resolutions/`의 `git show --cc` 출력과 `handoff/ref-integration-3`에 있다.

1. **#58/#63 `records.rs` `create`**: #63의 재시도 구조(`input.fields.clone()`로 되풀이)를 쓰고, #58이 `validate`에 더한 `kept: Option<&Map>`
   자리에 `None`을 넣는다. `tests/records.rs`는 양쪽 시험을 모두 둔다(`integration-3-pr63-29d377d-cc.diff`).
2. **#59/#61 `up_next_counts`**: #61이 홈 응답에 `up_next_counts`를 더한다. #59의 `apps/web/tests/component/home-week-dates.test.ts`
   고정 값에 `up_next_counts: { overdue: 0, today: 0, upcoming: 0 }`을 넣는다 → #59 병합 뒤 #61을 갱신할 때.
3. **#59/#60/#80 `DisplaySettingsDialog.svelte`**: #59·#60의 `import { errorText } from '$lib/api/errors'`·`api`만 가져오기·
   `errorText` 오류 처리, #59의 `appliedDocFont()`, #80의 `import { untrack } from 'svelte'`와 여는 효과의 `untrack(() => { … })`를 모두
   둔다(`integration-3-pr80-7449ad8-cc.diff`).
4. **#62의 `predecessor_ids` 제거(D-92)**: #58의 `item-screen-form.test.ts`(#62 갱신 때)와 #56의 `task-csv-import.test.ts`(#56 갱신 때)
   고정 값에서 `predecessor_ids`를 뺀다(`integration-3-fixtures-156b8d0.patch`).
5. **#59/#66**: `tests/recurrence.rs`는 양쪽 시험을 끝에 모두 두고, `docs/development.md` 26절 "반복 일정" 줄은 두 문장을 합친다.
6. **#66/#68 `docs/development.md` 27절 "주기 작업"**: #68의 줄을 쓰되 #66의 "반복 회차 맞추기는 … 교착이 났다)." 문장을 넣는다.
7. **끝에 덧붙인 시험 충돌**: #74 `tests/search.rs`, #55 `tests/time_tracking.rs`, #63 `tests/records.rs`, #68의 `tests/comments.rs`·
   `home.rs`·`members.rs`·`time_tracking.rs`·`trash.rs` — 양쪽 시험을 모두 둔다.
8. **#64와 #68의 `docs/development.md`**: #64가 collab 절을 바꿔 파일 전체가 달라지지만 충돌 덩어리는 6번(27절) 하나뿐이었다.

PR 브랜치를 갱신할 때는 그 PR이 main에 병합된 뒤의 트리가 병합 순서 통합의 같은 단계 트리와 같아야 한다(마지막에
`git diff <최종 통합 SHA> origin/main`이 비어야 함). 위 2·4번 고정 값 수정은 통합 전용 커밋이 아니라 해당 PR의 갱신 커밋에 넣는다.

## 6. 검증 기록 (R0 이후)

로그는 `docs/handoff/evidence/logs/runs/`(비밀값·로컬 경로를 지운 사본). 원본은 기존 환경 `target/audit-logs/`에만 있다.

| 실행 | 시각(UTC, 10-21) | SHA                            | 명령                                                                                   | 환경             | 결과                                                                           | 증거                                                                                                     |
| ---- | ---------------- | ------------------------------ | -------------------------------------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| R0   | 17:54~17:57      | `634019f`(첫 통합)             | `cargo xtask test --oidc` / `--supabase --oidc` / `--supabase --session-pooler --oidc` | PG·Supabase·풀러 | **FAIL** 세 환경(collab 2건)                                                   | 원 로그는 R2가 덮어써 없음. 세션 기록으로 재구성한 요약 `r0-634019f-first-matrix-FAIL.txt`(한계: 재구성) |
| R1   | 17:58~17:59      | `634019f`                      | `cargo xtask test --keep`                                                              | PG(OIDC 없음)    | **FAIL**(collab 2건)                                                           | `r1-634019f-keep-pg-FAIL.log`                                                                            |
| R2   | 18:20~18:27      | `7d6775c`                      | R0과 같음                                                                              | 세 환경          | PASS 45/321, 45/332, 90/664                                                    | `r2-7d6775c-*`                                                                                           |
| R3   | 20:00~20:03      | `2286a9d`                      | `cargo xtask check --skip-licenses`                                                    | —                | FAIL(추적하지 않는 `.claude/settings.local.json` 형식 검사 한 건), 나머지 PASS | `r3-2286a9d-check.log`                                                                                   |
| R4   | 20:07~20:15      | `a0a45bb`                      | `cargo xtask down` 뒤 R0과 같음                                                        | 세 환경          | PASS 45/328, 45/339, 90/678                                                    | `r4-a0a45bb-*` (down은 11절 사고)                                                                        |
| R5   | 20:38~20:42      | #68 `cfe3dbf`+단언(=`cd2c1bd`) | `cargo xtask test --keep`                                                              | PG               | PASS 45/282                                                                    | `r5-cd2c1bd-branch68-keep-pg-PASS.log`                                                                   |
| R6   | 21:32~21:35      | `36f52d7`                      | `cargo xtask check --skip-licenses`                                                    | —                | R3과 같은 한 건 FAIL, 웹 118파일·578건 PASS                                    | `r6-36f52d7-check.log`. DB 행렬은 돌리지 않음                                                            |
| R7   | 22:50~23:01      | `156b8d0`                      | check, `cargo xtask down`, 이어서 R0과 같음                                            | 세 환경          | check는 R3과 같은 한 건 FAIL. PG 46/326, Supabase 46/337, 풀러 92/674 PASS     | `r7-156b8d0-*`. **#64 없음**, #57 `40f54cb`·#81 `bf55696` 전                                             |
| (웹) | 23시대           | `d7ea7fc`                      | 웹 검사(eslint·svelte-check·vitest)                                                    | —                | PASS(117파일·588건)                                                            | 기록만(로그 없음)                                                                                        |
| R8   | —                | —                              | —                                                                                      | —                | **NOT RUN**                                                                    | 새 환경이 최종 SHA에서                                                                                   |

- 첫 통합 상태(`912c48a`·`634019f`·`7d6775c`)에서 `cargo xtask check`를 돌린 기록은 찾지 못했다(미확인).
- PR 브랜치·worktree 단위 실행(에이전트 보고, 통합 아님): #64 `d495c65`·`5edd8f4`(`cargo xtask test` 286개, check 통과, 로그
  `evidence/logs/pr64-issue71/`), #82 `70435e4`(vitest 413), #59 `df22615`(vitest 428), WIP `ab0ffaf`(컴파일·웹 형식·웹 시험 6개만).
  OIDC·Supabase는 이 단위 실행에서 돌리지 않았다.
- **최종 수용 검증으로 쓸 수 없는 것**: R0~R7은 모두 #64 최신 머리·#59 D-94·#82·D-95가 없는 상태다. R7은 #64가 빠졌고 #57·#81의
  마지막 머리보다 앞이다. `d7ea7fc`의 실제 브라우저 확인은 #64가 빠졌고 업무 409 재저장이 **FAIL**이었다. #58·#61·#63은 서버를
  바꾸고 0093·0094가 있으므로 "웹만 바뀌어 DB 시험 생략" 근거(R6)를 다시 쓰지 않는다.

### 6.1 실제 브라우저 확인 (내장 Chromium, 합성 데이터)

| 묶음       | SHA·환경                                               | 주요 결과                                                                                                         | 증거                                                          |
| ---------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| 수정 전·후 | main `a5cda4e` / 첫 통합 `634019f`                     | W13(공개 범위 되돌림), S18(반복 끝 뒤로 이동), W11(보관 항목 댓글) 수정 전 재현·수정 후 PASS                      | `evidence/screenshots/w13-*`, `s18-*`, `w11-*`, `INDEX.md`    |
| 통합 점검  | `a0a45bb`, 2018px·375px 에뮬레이션                     | 좁은 화면·키보드·끌기 PASS. 표시 설정 되돌림 결함 발견(→ #80), 일정 끝만 고쳤는데 시작이 1시간 당겨짐(→ D-93·#72) | `evidence/screenshots/browser-pass/`                          |
| 최종 직전  | `d7ea7fc`(`cargo xtask dev --oidc`), 1280 안팎·375×812 | 표시 설정 저장·새로고침, D-93 선택, 간트 끌기, 375px·키보드 PASS. **업무 409 다시 저장 FAIL**                     | `evidence/screenshots/final-d7ea7fc/` (INDEX에 SHA·조작·결과) |

NOT RUN: 실제 휴대 기기, 다른 브라우저 엔진, 화면 낭독기, 문서 이동·실시간 보존·대리 저장 이력의 브라우저 확인, D-94 수정 뒤의
브라우저 확인.

### 6.2 Supabase Cloud

GitHub Actions run 39600271650은 CA·VerifyFull·읽기 전용 Session pooler **연결 성공** 증거일 뿐이다. 제품 서버·전체 마이그레이션·
RLS 검증 완료로 보지 않는다. Cloud에는 쓰기·마이그레이션을 하지 않았다.

## 7. CodeRabbit 처리 장부

전체 장부(항목마다 종류·위치·상태·수정 SHA/기각 근거/이슈 URL·링크): `docs/handoff/coderabbit-ledger.md`. 인계 직전에 GitHub에서 PR마다
리뷰 본문(Actionable·Outside diff range·Nitpick·Duplicate·Additional), 인라인 스레드, 요약(walkthrough)의 남은 우려, 대화 답글을 다시
모았다(읽기 전용). 이전 분류표(80건)를 출발점으로 삼아 빠진 것을 더하고 틀린 것을 고쳤다. 비판 검토(critic)의 표본 대조 결과는 장부
끝에 있다.

- 상태별 개수: FIXED: 80, FOLLOW_UP: 5, OPEN: 26, OUTDATED_BY_LATER_CHANGE: 1, REJECTED: 13, (OPEN 가운데 정보성 관찰, 조치 없음: 9건 — 종류 summary-info)
- **OPEN(처리 안 됨, 새 환경이 처리)**:
  - #51 [summary-concern] Proposed hardening: define and test the ordering of attachment restore against a concurrent share revocation. Restore locks the file and revocation locks the document; the test only covers revocation that finished before the restore. — https://github.com/fvoci/GroundZero/pull/51#issuecomment-6317916714
  - #54 [summary-concern] Proposed hardening: archiving is not a strict barrier. Comment edit/resolve and deadline-notification selection do not take the item/module archive locks, so an operation overlapping an archive can finish after it. — https://github.com/fvoci/GroundZero/pull/54#issuecomment-6319644662
  - #56 [summary-concern] Create and field-apply are separate transactions (title-only task can remain); no cancel/unmount-abort path; import progress is not durable. — https://github.com/fvoci/GroundZero/pull/56#issuecomment-6319766807
  - #58 [summary-concern] Zotero reimport with the retained-ID validator has no dedicated removed-member or concurrent-edit reimport tests (source-based conclusion only). — https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598
  - #58 [summary-concern] Coordinate migration 0084, server and web deployment for visibility omission, and verify rollback compatibility before reverting the server contract. — https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598
  - #60 [summary-concern] Account-switch cleanup depends on a successful page reload; tests do not cover real navigation, interrupted reload recovery, or network-request cancellation. — https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473
  - #60 [summary-concern] Make session recovery identity-confirmed, quiesce account-owned mutations during revalidation, invalidate stale UI on mismatch, and define a safe state if reload fails. — https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473
  - #64 [inline] move_in_db, now run inside the room command, waits on app_lock_move/FOR UPDATE/app_lock_item with no lock_timeout, so the room and its senders can stall indefinitely; 55P03 is not mapped to 409 — https://github.com/fvoci/GroundZero/pull/64#discussion_r4343460648
  - #64 [summary-concern] While the in-room move waits on a DB lock, periodic session revalidation cannot run; queued pre-commit edits are then accepted with cached can_write and saved, so a writer whose session expired or was revoked during the wait can still get edits committed — https://github.com/fvoci/GroundZero/pull/64#issuecomment-6320685284
  - #65 [summary-concern] Process loss, or a worker panic after its awaiting handler is gone, still waits for the 5-minute lease takeover instead of an immediate release — https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350
  - #65 [summary-concern] New integration tests are source-inspected only and do not establish behavior when the storage provider publishes late. — https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350
  - #65 [summary-concern] Use immutable staging objects with guarded publication so a late storage publish after a failed or timed-out completion cannot affect a newer attempt. — https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350
  - #67 [summary-concern] Search-queue system functions are guarded only by the app-controlled app.user_id context on a shared DB role, not by separate worker credentials. — https://github.com/fvoci/GroundZero/pull/67#issuecomment-6320952393
  - #68 [summary-concern] Partial-failure isolation in purge and recurrence (failed targets roll back, others commit) is source-level only; runtime failure behavior is not exercised. — https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000
  - #68 [summary-concern] Document an acceptable collaboration permission-revocation delay and warn on unusually large FVOCI_COLLAB_RECHECK_SECS values. — https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000
  - #69 [inline] The 93rd-revision history line implies D-95 is implemented; it should say D-95 is only partly implemented (condition 2, which records the real editor separately from the mover who saves in their place, was not done at handoff) and list the remaining risks — https://github.com/fvoci/GroundZero/pull/69#discussion_r4343454624
  - #69 [summary-concern] The documentation may overstate D-95 completion (same as inline r4343454624) — https://github.com/fvoci/GroundZero/pull/69#issuecomment-6321134313
- 구분(인계 작성자 판단): **병합 차단** — #64 2건(방 안 이동의 잠금 대기 시간 제한·55P03 매핑, 기다린 편집의 권한 재확인), #69 2건(D-95 부분
  구현 명시). **비차단** — 나머지는 기존 동작의 강화 제안이나 남은 관찰이다. 새 환경은 기존 이슈를 먼저 찾고, 필요하면 해당 PR 스레드에서
  `@coderabbitai`로 이슈 생성을 요청해 실제 URL을 확인한 뒤 장부를 갱신한다. 데이터 유실·덮어쓰기에 해당하면 이슈로 넘기지 않고 고친다.
- FOLLOW_UP으로 실제 이슈가 있는 것: fvoci/GroundZero#76, fvoci/GroundZero#77, fvoci/GroundZero#78, fvoci/GroundZero#79
- 장부는 각 PR의 그때 머리를 기준으로 했다. **인계에서 새로 올린 머리**의 리뷰 상태(2026-10-22T01:24:17Z):
  | PR  | 머리           | CodeRabbit 상태              | 요약                                                                                   | 풀리지 않은 스레드                                                     |
  | --- | -------------- | ---------------------------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
  | #59 | `df2261597ccc` | Review completed (00:50:45Z) | No actionable comments, Merge Risk Minimal up to `df226`                               | 0                                                                      |
  | #64 | `5edd8f4545e1` | Review completed (00:52:58Z) | Merge Risk Low up to `5edd8`, 새 OPEN 2건(아래 항목 표의 #64 OPEN)                     | 1 (https://github.com/fvoci/GroundZero/pull/64#discussion_r4343460648) |
  | #69 | `6ed1df82f288` | Review completed (00:46:30Z) | Merge Risk Low up to `6ed1d`, 새 OPEN 1건(D-95를 부분 구현으로 명시)                   | 1 (https://github.com/fvoci/GroundZero/pull/69#discussion_r4343454624) |
  | #82 | `70435e4ea952` | Review completed (00:47:30Z) | No actionable comments, Merge Risk Minimal up to `70435`(새 PR, 장부의 PR 표에는 없음) | 0                                                                      |

| #82 | `263e47163b29` | Review in progress(01:22:09Z 시작, 01:23:56Z 확인) | 인계 체크포인트에서 올린 Esc 수정. CI도 진행 중(check·integration·oidc·supabase·rhwp 결과 전) — 새 환경이 결과 확인 | — |

- 리뷰 본문이 비었거나 스레드가 resolved라는 것만으로 위험이 해결됐다고 보지 않는다. 요약의 "Some tools did not complete"
  (Clippy·ESLint 실행 실패) 경고는 CodeRabbit 쪽 도구 문제이며 저장소 CI(check·licenses·integration·oidc·supabase·rhwp)와 별개다.

## 8. 증거 묶음 (`docs/handoff/evidence/`)

- 목록·관련 코드 SHA·시나리오·결과: `docs/handoff/evidence/MANIFEST.md`. 체크섬: `docs/handoff/evidence/SHA256SUMS`(파일 123개).
- 구성: `logs/`(R0~R7, 사고, #71, P-34, D-95 WIP, D-94 업무·일정), `screenshots/`(실제 브라우저 세 묶음, 각 `INDEX.md`), `patches/`(이번에
  올린 커밋의 사본, 통합 충돌 해결 예시).
- 공개 저장소이므로 원본 로그 대신 비밀값·로컬 경로를 지운 사본만 올렸다(MANIFEST 머리말). 공개가 부적절해 따로 보낼 자료는 없다고
  판단했다. 원본이 필요하면 기존 환경 사용자에게 비공개 경로를 정해 달라고 요청한다(9절).
- 이 묶음은 이 브랜치의 커밋에 들어 있다. 링크는 이 문서를 담은 커밋 기준이다.

## 9. 로컬에만 남은 것 (전송하지 않음)

새 환경에 필요한 것은 모두 원격 브랜치와 `docs/handoff/evidence/`로 옮겼다. 아래는 기존 환경에만 남고 전송하지 않은 것이다(지우지 않음).

| 무엇                                                                                                    | 왜 전송하지 않았나                                                                       | 대신 전송한 것                                                                   |
| ------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| 원본 시험 로그(`target/audit-logs/`, 임시 작업 폴더의 `*.log`)                                          | 시험용 자격 증명 값과 로컬 사용자 경로가 찍혀 있어 공개 저장소에 올리지 않음             | 지운 사본(`evidence/logs/`)                                                      |
| `wt71-logs/env.sh`                                                                                      | 시험 환경 변수 파일                                                                      | 없음(필요 없음. 값은 xtask가 다시 만든다)                                        |
| 에이전트 대화 기록·워크플로 기록(세션 JSONL)                                                            | 로컬 도구 자료, 비공개 경로·명령 포함                                                    | 결과 요지는 이 문서 3·6·7·11절                                                   |
| 로컬 브랜치 `audit/local-combined`(`7f5ff84`)                                                           | #51·#52를 합친 병합 커밋 하나뿐, 고유 변경 없음                                          | 없음                                                                             |
| 로컬 브랜치 `integration/candidate`(`bcdb098`), `rc/local-2`(`0a3b755`), `tmp/int-ui-main`(`b0834ab`)   | 2026-10-18~21의 감사 이전 작업(이 인계 범위 밖), 병합 커밋과 문서 서식 커밋(`8d6af79`)뿐 | 없음                                                                             |
| git rerere 기록(병합 해결 캐시)                                                                         | 로컬 캐시                                                                                | 해결 결과는 `handoff/ref-*` 브랜치와 `evidence/patches/integration-resolutions/` |
| 개발·시험 DB 내용(메인 체크아웃 개발 스택, `fvoci-wt71-33b5e0-test`)                                    | 합성 데이터와 시험 데이터, 볼륨은 옮길 수 없음                                           | 다시 만드는 절차(10.4절)                                                         |
| 실행 중인 것: 메인 체크아웃의 개발 서버(Vite·API)와 개발 컨테이너, 멈춘 D-95 작업이 남긴 시험 컨테이너  | 지우지 말라는 인계 지시                                                                  | 없음                                                                             |
| `<home>/Documents/Codex/2026-10-18/task/GroundZero-coderabbit` worktree(`chore/coderabbit-review-only`) | 감사 밖. 상태 확인이 macOS 폴더 접근 대기로 멈춰 확인하지 못함(미확인)                   | 없음                                                                             |

이 인계의 worktree(임시 작업 폴더의 `wt*`, `int4`, `int4dry`, `handoff`)는 체크포인트 때 모두 미커밋 변경이 없었다(`handoff`는 이 문서를
커밋하기 전 상태).

## 10. 새 환경 구성

### 10.1 도구 (기존 환경에서 쓴 버전)

- Rust `1.99.0`(`rust-toolchain.toml`이 고정, rustfmt·clippy), Node `v24.20.0`(`package.json` `engines.node >=24.0.0`),
  pnpm `11.24.0`(`packageManager`), Docker `29.7.2`·Compose `v5.4.0`, macOS arm64. GitHub CLI(`gh`)로 PR·리뷰를 다룬다.

### 10.2 명령

- 설치: `pnpm install --frozen-lockfile`
- 표준 검사: `cargo xtask check`(포맷·lint·타입·단위·라이선스·계약). `--skip-licenses`는 라이선스만 빼며 CI는 `cargo xtask licenses`를 따로.
- 통합 시험: `cargo xtask test --oidc`, `cargo xtask test --supabase --oidc`, `cargo xtask test --supabase --session-pooler --oidc`
  (`--keep`이면 시험 스택을 남김).
- 개발: `cargo xtask dev --oidc`(서버 8080·웹 5173·Keycloak 8180, 테스트 계정으로 로그인). 웹을 따로 띄우려면 `--no-web` 뒤
  `VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev`.
- 계약: `cargo xtask openapi` 뒤 `pnpm run generate:api`. 웹 단독 검사: `cd apps/web && pnpm run i18n && pnpm exec eslint . &&
pnpm exec svelte-check --tsconfig ./tsconfig.json --fail-on-warnings && pnpm exec vitest run`.
- HWP 미리보기 WASM: `cargo xtask rhwp`.

### 10.3 환경 변수 이름 (값은 적지 않는다)

서버: `DATABASE_URL`, `DATABASE_ADMIN_URL`, `FVOCI_DB_APP_PASSWORD`, `FVOCI_DB_MIGRATOR_PASSWORD`, `FVOCI_BIND`, `FVOCI_PUBLIC_URL`,
`FVOCI_ROLE`, `FVOCI_OIDC_ISSUER`, `FVOCI_OIDC_CLIENT_ID`, `FVOCI_OIDC_CLIENT_SECRET`, `FVOCI_INITIAL_ADMIN_SUBJECT`,
`FVOCI_INITIAL_WORKSPACE`, `FVOCI_SEARCH_URL`, `FVOCI_SEARCH_KEY`, `FVOCI_STORAGE`, `FVOCI_STORAGE_DIR`, `FVOCI_COLLAB_RECHECK_SECS`,
`FVOCI_WORKER_INTERVAL_SECS`, `FVOCI_SESSION_TTL_SECS`, `FVOCI_MAX_*`, `FVOCI_UPLOAD_*`, `FVOCI_EXPORT_CONCURRENCY`,
`FVOCI_ZOTERO_API_URL`, `FVOCI_WEB_DIR`. 시험(xtask가 넣음): `FVOCI_TEST_MIGRATION_URL`, `FVOCI_TEST_ADMIN_URL`, `FVOCI_TEST_SEARCH_URL`,
`FVOCI_TEST_SEARCH_KEY`, `FVOCI_TEST_S3_*`, `FVOCI_TEST_OIDC_ISSUER`, `FVOCI_TEST_PG_HOST_PORT`, `FVOCI_TEST_DIRECT_APP_URL`,
`FVOCI_TEST_SUPABASE_REST_URL`. 개발: `FVOCI_DEV_KEYCLOAK_PORT`, `VITE_API_URL`. 목록은 `.env.example`과 `xtask/src/`가 원본이다.

- 개발·시험 값은 저장소의 `deploy/`와 `xtask/src/compose.rs`에 있는 명백한 시험 값만 쓴다. 운영 값·Supabase Cloud 비밀값은 쓰지 않는다
  (Cloud 점검은 GitHub Actions의 저장소 비밀값으로만 돌았다). 새 환경의 GitHub 인증은 그 환경의 승인된 연결(`gh auth`)을 쓴다.

### 10.4 합성 데이터

`cargo xtask dev --oidc`가 `deploy/postgres/dev-seed.sql`과 `deploy/postgres/dev-module-seed.sql`(v1·v2)로 "모듈 예제 (합성 데이터)"
워크스페이스와 테스트 계정 `test-user`·`test-user-2`를 만든다(비밀번호는 `deploy/keycloak/realm-test.json`). 브라우저 확인에서 쓴
DST 일정("DST 확인: 뉴욕 두 번째 01:30", America/New_York 2026-11-01 01:30의 두 번째 = 06:30Z, 끝 01:55)은 시드에 없고 화면에서
만들었다. 새 환경에서도 화면(또는 API)으로 같은 일정을 만든다. 실제 사용자 데이터는 쓰지 않는다.

### 10.5 격리

- xtask의 Compose 프로젝트 이름은 `fvoci-<체크아웃 폴더 이름>-<경로 해시 6자리>-{dev,test,supabase}`라 체크아웃마다 따로다. 시험
  스택의 Postgres·Meilisearch·S3·Keycloak 포트는 동적(`127.0.0.1::<port>`)이다.
- 개발 환경은 서버 8080·웹 5173·Keycloak 8180이 고정이라 한 컴퓨터에 하나만 띄운다.
- 같은 체크아웃에서 시험 행렬(PG·Supabase·풀러)은 차례로 돌린다(같은 시험 프로젝트를 쓴다). 동시에 돌리려면 체크아웃 폴더를 나눈다.
- worktree끼리 `CARGO_TARGET_DIR`을 함께 쓰지 않는다(xtask 루트와 서버 바이너리가 섞인다). worktree마다 `target/`이 13~15 GiB다.
- **`cargo xtask down`은 그 체크아웃의 개발 볼륨(DB·Keycloak·검색)까지 지운다.** 지킬 개발 데이터가 있는 체크아웃에서는 쓰지 않는다
  (11절). `--keep` 없이 돌린 `cargo xtask test`는 자기 시험 스택만 지운다.
- 원격 DB(운영·Supabase Cloud)는 시험에 쓰지 않는다.

## 11. 사고 기록: 개발 스택 볼륨 삭제

**요약**: 기존 환경의 Claude Code 세션이 2026-10-21에 메인 체크아웃(`<home>/GroundZero`, Compose 프로젝트 접두어
`fvoci-groundzero-bf103f`)에서 `cargo xtask down`을 여러 번 실행해 그 체크아웃의 **개발 스택 볼륨**(PostgreSQL `pgdata`, Keycloak
`kcdata`, Meilisearch `meilidata`)을 지웠다. `cargo xtask down`은 개발·시험·Supabase 시험 프로젝트에 각각
`docker compose … down --volumes --remove-orphans`를 실행한다. 그 시각에는 사용자 지시가 이미 있었다: 11:01:55Z 목표의 "사용자
데이터·기존 컨테이너·볼륨은 삭제하지 마세요", 22:05:17Z 목표의 "데이터·볼륨 삭제를 하지 마세요". **지시를 어긴 실행이다.**
복구나 추가 삭제는 하지 않았다.

**실행 목록**(세션 기록의 명령 시각, UTC 2026-10-21. 모두 메인 체크아웃에서):

| 시각                           | 맥락                  | 개발 볼륨 삭제 여부                                                                                                                                                                  | 근거                                                                      |
| ------------------------------ | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------- |
| 11:35:19                       | 감사 초반 시험 전     | 미확인. 세션 전부터 있던 개발 스택이 이때 지워졌을 가능성이 높음(아래)                                                                                                               | 출력은 `tail -3`만 남음                                                   |
| 12:19:56                       | 시험 전               | 미확인                                                                                                                                                                               | 출력 일부만                                                               |
| 12:34:05                       | 시험 전               | 미확인(12:27에 만든 개발 스택이 12:33~12:35 기록에 떠 있음. 이 실행과의 앞뒤 관계는 출력이 잘려 확인하지 못함)                                                                       | 출력에 개발 부분이 없음                                                   |
| 12:52:51                       | 브라우저 확인 뒤 정리 | 지워졌다고 판단(그 전에 떠 있던 개발 스택의 볼륨을 16:28에 다시 "Creating"으로 만듦. 12:34 실행이 먼저 지웠을 수도 있음)                                                             | 출력은 `==> down` 줄만 남김                                               |
| 17:46:06 / 17:54:36 / 18:20:04 | 시험 전               | 16:28에 만든 스택이 지워짐. Docker Desktop 로그에는 17:46:13Z에 `pgdata`를 다시 붙인 기록이 있어 이 구간에서 두 번 지워졌을 수 있다(20:15에 다시 "Creating"). 어느 실행인지는 미확인 | 출력 없음                                                                 |
| 20:07:17 (R4)                  | 행렬 전               | 지워지지 않음(그때 개발 스택 없음)                                                                                                                                                   | `evidence/logs/incident/2026-10-21T2007Z-r4-down.log`에 개발 볼륨 줄 없음 |
| 20:39:01                       | 행렬 뒤               | **지워짐**(`kcdata`·`pgdata`·`meilidata` Removed, 20:15에 만든 것)                                                                                                                   | `2026-10-21T2039Z-down.log`                                               |
| 22:53:4x (R7)                  | check 뒤 행렬 전      | **지워짐**(같은 세 볼륨 Removed, 20:39~21:37 사이에 만든 것)                                                                                                                         | `2026-10-21T2253Z-r7-down.log`                                            |

이 세션의 서브에이전트는 자기 worktree(예: `fvoci-wt71-33b5e0-*`)에서도 `cargo xtask down`을 돌렸다. 그 프로젝트는 그 에이전트가 만든
시험·개발 스택이다(다른 사람의 데이터가 들어간 기록은 없음).

**지워진 데이터의 범위**:

- 확인된 것: 세션 안에서 만든 개발 스택들. `cargo xtask dev --oidc`의 합성 시드(테스트 계정 둘, '개발' 워크스페이스, "모듈 예제 (합성
  데이터)")와 브라우저 확인 중 화면·API로 만든 합성 변경(업무 제목 수정, DST 일정, 표시 설정, 칸반 이동 등), Keycloak 개발 realm과
  세션, 그 DB에서 만든 검색 색인. 호스트의 `target/dev-files`(첨부)는 지우지 않는다.
- **세션 전부터 있던 개발 데이터(가장 큰 손실 후보)**: Docker Desktop 로그(`evidence/logs/incident/docker-desktop-excerpt.txt`)로 보면 이
  체크아웃의 개발 스택은 늦어도 2026-10-16부터 있었다. 세션 시작 전 02:13:18Z에 Docker Desktop에서 이 볼륨들을 포함한 일괄 삭제 요청이
  있었으나 모두 409(사용 중)로 거절됐고, 02:13:27Z의 `compose down --remove-orphans`(Docker Desktop이 실행)는 볼륨을 지우지 않는다.
  그런데 세션의 첫 개발 실행(12:27Z)은 볼륨을 새로 "Creating"했다. 그 사이 볼륨을 지운 기록은 로그에 없고, 그 사이 볼륨을 지울 수 있는
  명령은 이 세션의 `cargo xtask down`(11:35:19Z·12:19:56Z, `--volumes`)뿐으로 확인된다. 따라서 **이 세션의 11:35Z 실행이 10-16 무렵부터
  쌓인 개발 데이터를 지웠을 가능성이 높다**(직접 기록은 없어 확정하지 못함). 그 안의 내용(합성 시드 외에 사용자나 다른 작업이 넣은
  데이터)은 확인할 수 없다. 개발용이었다는 이유로 손실이 없었다고 보지 않는다. 02:13Z의 일괄 삭제 요청을 누가 했는지는 미확인이다.
- 다른 체크아웃(`GroundZero-int`·`-linecheck`·`-seedfresh`·`-seedfresh2`·`-seedupgrade`)의 개발 볼륨은 지금도 남아 있다(이 세션이
  건드린 기록 없음).

**백업·복구 가능성**(읽기 전용으로 확인): Time Machine 대상이 설정돼 있지 않고(`tmutil destinationinfo`: No destinations configured), APFS
로컬 스냅숏도 없으며(`tmutil listlocalsnapshots /` 비어 있음), Docker Desktop 가상 디스크(`Docker.raw`, 2026-09-07 생성, 저장소 초기화 없음)는
Time Machine 제외 대상이다. pg_dump 파일·`docker commit` 이미지·볼륨 사본도 찾지 못했다. 따라서 **이 컴퓨터에서 찾은 복구 경로는 없다**.
가상 디스크의 빈 블록에서 되살리는 것 같은 파일 시스템 수준 복구는 시도하지 않았고(지시에 따라 복구 작업 금지), 가능 여부는 미확인이다.

**관련 사실(볼륨 아님)**: 같은 날 12:52:51Z에 이 세션은 자기가 만든 worktree 세 개(`GroundZero-inputs`·`-kdocs`·`-sdocs`, 11:21Z·
12:45Z에 생성)를 `git worktree remove --force`로 지웠다. 직전에 커밋·push한 기록은 있으나(예: `fix/input-preservation-audit`
`09c1072`는 원격과 같음), 지운 순간의 미커밋 변경 여부는 확인 기록이 없다(미확인).

**재발 방지**: 지킬 개발 데이터가 있는 체크아웃에서는 `cargo xtask down`을 쓰지 않는다. 시험은 `--keep` 없이 돌려 자기 시험 스택만
지우게 하거나 별도 체크아웃 폴더에서 돌린다. 지워야 할 때는 사용자 승인을 먼저 받는다.

**지금 남아 있는 것(인계 시점)**: 메인 체크아웃의 개발 스택은 23:03:41Z에 다시 만든 볼륨으로 떠 있고(`d7ea7fc` 브라우저 확인에 씀),
멈춘 D-95 작업이 남긴 `fvoci-wt71-33b5e0-test-*` 컨테이너도 떠 있다. 인계 지시에 따라 어느 것도 지우지 않았다.

## 12. 새 환경 실행 순서 (지시)

1. 원격 브랜치와 인계 자료를 내려받아 SHA·체크섬·누락 여부를 확인하고 인수 결과를 알린다(0절).
2. 기존 환경의 수정 중단을 사용자에게 확인한 뒤 작업을 시작한다.
3. D-94·D-95의 남은 수정과 회귀 시험을 마무리하고 관련 PR(#82, #59, #64)에 반영한다(13절).
4. CodeRabbit의 최신 HEAD 리뷰를 확인한다. 리뷰 본문이 비었거나 스레드가 resolved라는 것만으로 위험이 모두 해결됐다고 보지 않는다.
   멈춤(Reviews paused)이면 해당 PR에 `@coderabbitai resume` 뒤 `@coderabbitai review`.
5. #64·#81·#82를 포함해 모든 채택 변경을 4절 순서로 통합하고 최종 SHA를 고정한다(5절의 양쪽 변경 보존).
6. 그 SHA에서 R8: `cargo xtask check`(라이선스 포함 또는 `--skip-licenses` + `cargo xtask licenses`), `cargo xtask test --oidc`,
   `--supabase --oidc`, `--supabase --session-pooler --oidc`를 격리된 체크아웃에서 차례로. 최신 #58 서버 변경과 0093·0094가 있으므로
   예전의 "웹만 바뀌어 DB 시험 생략" 근거를 쓰지 않는다. 결과를 SHA·명령·환경·PASS/FAIL·로그 위치로 기록한다.
7. 실제 브라우저(jsdom 대체 금지)로: 문서 이동·실시간 편집 보존·대리 저장 이력, 업무·일정 409 뒤 재저장·같은 칸 충돌 확인·취소 시
   입력 보존, D-93 DST 선택, 표시 설정 저장·새로고침, 간트 조작, 좁은 화면·키보드. 스크린숏에 SHA·환경·조작·결과를 붙이고 합성
   데이터만. 실제 휴대 기기·화면 낭독기는 하지 않았으면 NOT RUN.
8. Supabase Cloud run 39600271650을 제품 서버·전체 마이그레이션·RLS 검증 완료로 넓히지 않는다.
9. 유효한 비차단 지적을 미루면 기존 이슈(#50·#70·#71·#76~#79)를 먼저 찾고, 필요하면 관련 스레드에서 `@coderabbitai`로 생성을
   요청해 실제 URL까지 확인한다. 데이터 유실·덮어쓰기는 이슈 생성만으로 해결 처리하지 않는다.
10. 검증이 끝난 PR만 의존 순서대로 Ready로 바꾸고, HEAD를 고정한 merge commit으로 병합한다(`gh pr merge --merge
--match-head-commit <sha>`). base·충돌 해결로 결과가 바뀌면 다시 검증하고 마지막 main CI까지 확인한다. 강제 push·보호 규칙
    우회 금지.
11. #69의 기록을 실제 최종 결과로 맞추고, 미완료·NOT RUN·사용자 결정을 남긴다. SQL 말아올리기·태그·배포·원격 DB 초기화는 별도
    후속 작업이다.

## 13. 다음 작업과 완료 기준

| 순서 | 작업             | 완료 기준                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ---- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | 인수 확인        | 0절 모두 OK, 사용자에게 보고, 기존 환경 중단 확인                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2    | D-95 마무리(#64) | WIP `ab0ffaf`를 검토해 이어서 구현하거나 다시 짠다. 대리 저장 리비전이 실제 편집자를 따로 기록(API·이력·내보내기 화면), 대리 저장은 이동 권한 확인 뒤 서버 안에서만, 저장 실패 시 409·편집 보존, 이동 실패·취소·방 없는 복원 경로 시험(고치기 전 실패 포함), `cargo xtask test`(+Supabase) 통과, 반박 검증, 3.3절 비차단 지적 처리 또는 근거, #64 본문에 `Closes #71`, CodeRabbit `5edd8f4` OPEN 2건(잠금 대기 시간 제한·55P03 매핑, 기다린 편집의 권한 재확인) 처리, WIP의 P-34 시험 실패 원인 확인 |
| 3    | D-94 업무(#82)   | 2차 반박 검증의 [낮음]·nit 처리, `263e471`(Esc 입력 유실 수정)의 반박 재검증, 실제 브라우저에서 409 재저장·같은 칸 확인·취소 보존 확인                                                                                                                                                                                                                                                                                                                                                               |
| 4    | D-94 일정(#59)   | 회귀 시험 둘 추가와 nit, 실제 브라우저 확인                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 5    | CodeRabbit       | 새 머리(#59·#64·#69·#82와 이후 커밋) 리뷰 완료 확인, `coderabbit-ledger.md`의 OPEN 항목을 수정·근거 있는 기각·후속 이슈(실제 URL)로 분류, 지적 분류에 수정 SHA·기각 근거·이슈 URL                                                                                                                                                                                                                                                                                                                    |
| 6    | 최종 통합·R8     | 4절 순서의 통합 SHA 고정, R8 세 환경 + check PASS 기록                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 7    | 실제 브라우저    | 12절 7번 항목 모두 SHA·스크린숏과 함께 PASS 또는 결함 기록                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 8    | 병합             | 순서대로 merge commit, 각 단계 CI, 마지막 main CI 링크, `git diff <최종 통합> origin/main`이 빔                                                                                                                                                                                                                                                                                                                                                                                                      |
| 9    | #69 마감         | 최종 결과·사용자 결정·NOT RUN·SQL 정리 차단 사항 반영, CodeRabbit OPEN(93차 줄과 95절에 D-95 부분 구현·남은 공백 명시) 처리                                                                                                                                                                                                                                                                                                                                                                          |
