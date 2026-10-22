# W16: 최종 통합 SHA 고정과 R8 — `audit/final`

- 작업: task_2c03044b9bfe / dispatch ctx_18cd3e255878. 2026-10-22. worktree `w13-int6`(W13 `audit/int6`는 그대로 둠, 재사용 기준).
- 기반: main `a5cda4e169e2`. 로컬 브랜치 `audit/final`(push하지 않음). 원격·PR 브랜치는 고치지 않았다.
- 고정 머리: `pinned-heads.txt`(번호·브랜치·SHA·base). 06:3x Z에 `git fetch` 뒤 27개 모두 `origin/<브랜치>`와 같음을 확인했다.
- **최종 SHA `07be34d2486b2fe60507ccc3156fb7e33b5fde44`**(tree `ff5db09ae482`). 첫 빌드 `5ddece5`(로컬 태그 `w16-build1`)는 coordinator 후속
  지시(06:45Z, #64 문서 문구)로 #64 단계부터 다시 지었다(2절). R8은 모두 `07be34d`에서 돌렸다.

## 1. 병합 단계 (`final-steps.tsv`가 기준)

`final-steps.tsv` 열: order, pr, branch, pinned_head, step_commit, step_tree, prev_step_tree, kind, conflicted_files, changed_files.
`make-steps-tsv.sh`가 첫 부모 사슬에서 기계적으로 만든다: `git merge-tree --write-tree <앞 단계> <고정 머리>`에 충돌이 없고 그 tree가 단계
tree와 같으면 `clean`, 아니면 `update`(conflicted = merge-tree의 충돌 파일, changed = merge-tree 결과와 단계 tree가 다른 파일).
결과: clean 13(#89·#51·#52·#53·#54·#58·#57·#65·#67·#75·#81·#72·#73), update 14.

| 순서 | PR  | 고정 머리      | 단계 커밋      | 단계 tree      | 종류   | 충돌 / 수정 (근거)                                                                                                                |
| ---- | --- | -------------- | -------------- | -------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------- |
| 1    | #89 | `01f17fbc6c36` | `72a83aab9f3f` | `68310424b5dc` | clean  | —                                                                                                                                 |
| 2    | #51 | `eb881939efc8` | `5f5b1aadb169` | `977dde1599aa` | clean  | —                                                                                                                                 |
| 3    | #52 | `591f93917e5f` | `42cb98640258` | `9c7aa8b5adb0` | clean  | —                                                                                                                                 |
| 4    | #53 | `3fc48a5a7fd0` | `81841f1eea77` | `23c60cd8d9bb` | clean  | —                                                                                                                                 |
| 5    | #54 | `4c15e3a072c4` | `b4e92f4e2fed` | `f6a96a8ba157` | clean  | —                                                                                                                                 |
| 6    | #58 | `c9a2bceb6359` | `86091ca437ee` | `d426a9cee069` | clean  | —                                                                                                                                 |
| 7    | #57 | `40f54cb9ffaf` | `cb3ce2f4cfa5` | `6872e91b0994` | clean  | —                                                                                                                                 |
| 8    | #59 | `180a0ae8b4ea` | `0b1b79d3e998` | `b1bb2f931873` | update | 충돌 `recurrence.rs`(W13 `41fae73` 재사용), `development.md`(W13 결과 + #89 `01f17fb` 문단, 3-way)                                |
| 9    | #62 | `26615d1493b5` | `9e6601656d02` | `e0923a69362d` | update | 수정 #58 `item-screen-form.test.ts` `predecessor_ids` 삭제(W13 `bdb35e3`)                                                         |
| 10   | #65 | `b9f5c2231a46` | `e8d79b355fd4` | `df377ab96206` | clean  | —                                                                                                                                 |
| 11   | #66 | `5142fb0dab5d` | `8fa4b082aa94` | `eabae9f85473` | update | 충돌 `recurrence.rs`(W13 `03fbde7` 재사용)                                                                                        |
| 12   | #67 | `e3d0df79c398` | `f51b5b67ff11` | `153890ebf82c` | clean  | —                                                                                                                                 |
| 13   | #74 | `01f028fa3550` | `9b64840584f1` | `962e449c99cb` | update | 충돌 `search.rs`(W13 `1783e04` 재사용)                                                                                            |
| 14   | #75 | `235e95d59c0f` | `510e07060a21` | `f32fdbeb57e6` | clean  | —                                                                                                                                 |
| 15   | #81 | `bf5569669060` | `c1af8db1a61a` | `c76c3565baae` | clean  | —                                                                                                                                 |
| 16   | #55 | `a34a5300ca93` | `f95dae88a367` | `ca6903179237` | update | 충돌 `time_tracking.rs`(W13 `e17565c` 재사용)                                                                                     |
| 17   | #56 | `1d7f9fc8b159` | `79cd37f3643d` | `db978f2f8023` | update | 수정 `task-csv-import.test.ts` `predecessor_ids` 삭제(W13 `5e188f9`)                                                              |
| 18   | #60 | `a811f9250020` | `e38a32e2ee6c` | `ecf82d7f19c8` | update | **새 충돌** 3개(1.1) + 수정 #57 초안 시험 둘의 `api.me` 흉내(W13 `ec7ffe5`)                                                       |
| 19   | #61 | `97447d837872` | `70457b8c653d` | `35d609620991` | update | 수정 `home-week-dates.test.ts` `up_next_counts`(W13 `548e5ab`, 이번엔 수정 전 실패를 다시 보지 않음)                              |
| 20   | #63 | `324d6e1d6905` | `86ac6bc262fc` | `7b3101b236c2` | update | 충돌 `records.rs`·`tests/records.rs`(W13 `b52b089` 재사용)                                                                        |
| 21   | #64 | `2acebe989c52` | `6bf0a9dee9cd` | `4b053fe1e0b3` | update | **새 충돌** 2개(1.2) + 수정 #57 `revision-compare-race.test.ts` `edited_by_names`(W13 `2427544`) + `development.md` W1b 문구(2절) |
| 22   | #68 | `cd2c1bddaf76` | `238e0aa0f740` | `1fc857a5d47f` | update | 충돌 시험 다섯(W13 `66e131f` 재사용), `development.md`(W13 결과 + 우리 쪽 새 변경, 3-way)                                         |
| 23   | #72 | `0213eb8be929` | `d88f963cf3b0` | `4b6312976411` | clean  | —                                                                                                                                 |
| 24   | #73 | `067ef3630b9e` | `0a86c6062924` | `09ea4bdfd756` | clean  | —                                                                                                                                 |
| 25   | #80 | `48c00871864d` | `bee0cbbd141c` | `ff769b2cd20f` | update | 충돌 `DisplaySettingsDialog.svelte`(W13 `b71fce0` 재사용)                                                                         |
| 26   | #82 | `4d3ae8ac6329` | `d83ca6d826aa` | `e72a95c80728` | update | **새 충돌** messages/common(1.3) + 수정 `predecessor_ids` 삭제 둘(W13 `fca9be6` + 새 `task-dialog-discard.test.ts`)               |
| 27   | #69 | `43554479c830` | `07be34d2486b` | `ff5db09ae482` | update | **새 충돌** `development.md` 95절·96절(1.4)                                                                                       |

재사용 규칙은 W13과 같다: "재사용" = 충돌 파일의 우리·상대 blob이 W13 같은 단계 병합의 부모와 바이트 단위로 같을 때만 그 결과를 가져옴
(`reuse-ref-resolution.sh`). "3-way" = 한쪽만 달라졌을 때 W13 결과에 새 변경만 `git merge-file`로 얹음(`reuse-ref-rebased.sh`, 깨끗할 때만).
#68 `development.md` 검산: W13 결과와의 차이(바뀐 줄) = 우리 쪽이 W13 우리 쪽과 다른 줄(diff 비교 같음).

각 단계는 `step.sh <pr>`(= `git merge --no-ff --no-commit <고정 SHA>` → `resolutions/<pr>/apply.sh` → 한 merge 커밋 `Merge #N (<브랜치>) in
merge order`), 전체는 `build.sh [시작 PR]`. 해결은 모두 `resolutions/<pr>/`(apply.sh 머리 주석이 근거, 손 해결은 `files/`+`blobs.txt`: 양쪽 blob이
기록과 같을 때만 씀, 고정 값 수정은 `*.diff`). **재현 확인**: 빈 브랜치에서 `build.sh`로 27단계를 다시 지어 첫 빌드의 27개 단계 tree가 모두 같았다
(`logs/replay.log`, 불일치 0). 최종 스크립트로 한 번 더 a5cda4e부터 지어 최종 27개 단계 tree가 모두 같았다(`logs/replay2.log`). 다시 지은 #64~#69(`logs/build2-from-64.log`)와 첫 빌드의 차이는 `development.md`의 W1b 문구 한 곳뿐(+3/−1).

### 1.1 #60 새 충돌 (#60 `a811f92`가 W13 뒤 움직임)

- `messages/common/{en,ko}.json`: 끝에 #59가 `common_discard_*` 넷을, #60이 `common_error_too_large`를 더함 → 둘 다 둠.
- `DocumentEditor.svelte` `edited()`: #57은 `storeDraft(...)`를 `keepDraft()`(고르기를 기다리는 지난 초안 `reservedDraft`를 덮지 않음)로 바꿨고,
  #60은 같은 자리의 예약 저장을 `if (!accountChanged)`로 막았다 → `keepDraft(); if (!accountChanged) schedule(AUTOSAVE_DELAY_MS);`.
- 고정 값: #60의 `save()`가 보내기 전에 `api.me()`를 묻는다. #57 초안 시험 둘이 흉내 내지 않아 3건 실패(흉내 빼고 돌려 확인, 로그는 남기지 않음·세션에서 관찰) → W13 `ec7ffe5`와 같은 수정.

### 1.2 #64 새 충돌 (#60 `a811f92`·#64 `2acebe9` 모두 W13 뒤 움직임)

- `LiveDocumentEditor.svelte` `keepDraft`(과제의 `live.ts`가 아니라 이 파일): #60은 `latest === undefined` 조건을 빼고 본문을 `latest ?? start.body`로
  (첫 동기화 전 제목만 고친 경우도 남김, 4412 계정 전환 초안 보존), #64는 반환값 `boolean`만 바꿈(4403/4409 미확인 편집 `kept`/`lost` 안내).
  → #60 본문을 쓴다(둘 다 `boolean`, #64가 쓰는 반환값 의미 같음). 두 쪽의 새 함수(#60 `hasPending`, #64 `unsaved`·`keepUnsaved`)를 모두 둠.
- **자동 병합된 곳의 의미 충돌**: `connection-close` 처리에서 #64가 모든 닫기 코드에 먼저 `keepUnsaved()`를 부르고 못 남기면 바로 멈추게 바꿔,
  #60의 4412 경로(남기지 못하면 다시 읽지 않고 `documents_live_account_changed_lost`)에 닿지 않았다. 그 결과 4412 + 저장 공간 가득 참에서
  "초안으로 남았고 다시 읽습니다" 문구가 보였다(#60 `live-account-changed.test.ts` "does not reload and says the edits were not kept" 실패,
  `logs/step64-live-tests-before-fix.log`). 해결: `account`(4412)는 `keepUnsaved()` 앞에서 #60의 처리(연결 끊기, `stopped`, `hasPending`/`keepDraft` →
  `switchAccount` 또는 `accountUnsavedLost`)로 끝내고, 나머지 닫기 코드는 #64 그대로. 확인: #60 `live-account-changed`·`live-title-before-sync`·
  `account-switch-unload`, #64 `live-unsaved-close` 12/12 통과.
- `crates/server/src/http/mod.rs` 시험 모듈: #60의 `the_save_conflict_and_the_expected_user_header_are_in_the_contract`와 #64의
  `the_move_conflict_is_a_preview_or_error_body`·`the_restore_conflict_is_a_stale_base_or_error_body`를 모두 둠.
- 생성 파일: `contracts/openapi.json`·`schema.ts`는 글자 충돌 없이 합쳐졌고, 이 단계에서 `cargo xtask openapi` + `pnpm run generate:api` 결과가 같았다
  (`logs/step64-openapi.log`). 이 단계에서 `cargo clippy --workspace --all-targets -D warnings`, svelte-check 0/0.
- 고정 값: `edited_by_names`는 여전히 필수(빼면 `revision-compare-race` 3건 실패, 로그 없음·세션에서 관찰) → W13 `2427544`.

### 1.3 #82 새 충돌

- `messages/common/{en,ko}.json`: #82가 더한 `common_discard_*` 넷은 #59와 같은 blob(`cb9b13d`) → 우리 쪽(#59 + #60 `common_error_too_large`).
  `alert-dialog/*` 13개 파일도 #59·#82가 같은 내용으로 더해 충돌 없음.
- 고정 값: #62가 `TaskDetailView.predecessor_ids`를 없앴다. #82의 `task-conflict-save-mine.test.ts:42`(W13과 같음)와 #82의 새
  `task-dialog-discard.test.ts:37`에서 지움(수정 전 svelte-check 2건, `logs/step82-svelte-check-before-fix.log`).
- W13이 흉내 낸 W11 `String(estimate_minutes)`(`17df953`)는 #82 `4d3ae8a`에 들어 있다. 흉내 커밋 없음.

### 1.4 #69 새 충돌

- `docs/development.md`: #60(96절 기대 사용자)과 #69(95절 코드 전반 감사)가 같은 자리("참고 자료" 앞)에 덧붙임 → 절 번호 순서로 95(#69) 뒤 96(#60).
  다시 지을 때는 기록 파일, 우리 쪽만 바뀌었으면 첫 빌드 결과(`5ddece5`)에 새 변경만 3-way.

## 2. coordinator 후속 지시 반영 (06:45Z)

#64 `2acebe9`의 `development.md` W1b 줄 "(…, coordinator 판단 B, 사용자 추가 지시)"가 재확인 방식을 사용자 결정처럼 읽히게 했다. #64 단계에
`resolutions/64/pr64-w1b-wording.diff`를 더했다: 경합 시험·(3) 상한 입증·확인받지 못한 편집의 보존 확인은 사용자 추가 지시, 대기 중 재확인 방식
(선택지 B)은 사용자 결정이 아니라 coordinator 판단(기존 원칙 "권한 철회는 재확인 간격 안에 반영"을 이동 대기 중에도 지키기 위함). #63 단계
`86ac6bc`에서 `build.sh 64`로 #64~#69를 다시 지었다. #69가 `4355447`로 맞다는 확인도 같은 메시지에 있었다.

## 3. 마이그레이션·조상·차이

- `migrations-0079-0094.txt`: 0080~0094가 하나씩, 앞 네 자리 중복 없음. 각 파일을 처음 들인 첫 부모 단계 = 0080 #51 … 0093 #81, 0094 #64(번호 순서).
  0020 빈 번호는 main `a5cda4e`에도 있는 것(그대로).
- `ancestry.txt`: 고정 머리 27개 모두 `git merge-base --is-ancestor <머리> 07be34d` 참.
- `diff-stat-a5cda4e-final.txt`: `git diff a5cda4e 07be34d --stat` = 265 files changed, +27583/−1976. 첫 부모 사슬 `first-parent-log.txt`.

## 4. R8 (`07be34d`, 같은 체크아웃에서 차례로, 동시 실행 없음)

환경: WSL2 Linux, 32코어. "결과 줄" = `test result:` 줄 수(`Running` 46 + `Doc-tests` 1). 로그의 첫 줄 = SHA·명령·시작, 끝 줄 = rc·끝(UTC).
홈·작업 경로는 `<home>`·`<worktree>`로 바꿈. 비밀값 없음.

| R8  | 명령                                                  | 시작 → 끝 (UTC)                        | 결과                                                                                                                                                                                               | 로그                                                                                                    |
| --- | ----------------------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| (a) | `cargo xtask check`(licenses·cargo deny 포함)         | 2026-10-22T06:52:15Z → 06:55:18Z, rc=0 | **PASS** 전 단계: fmt·clippy·unit(79+2+12)·cargo deny(advisories·bans·sources)·licenses(cargo deny, pnpm)·openapi 최신·format·lint·svelte-check·i18n·vitest 129파일 718개·rhwp·third-party notices | `logs/r8a-check-07be34d.log`                                                                            |
| (b) | `cargo xtask test --oidc`                             | 06:55:26Z → 06:57:27Z, rc=0            | **PASS** 결과 줄 47(바이너리 46 + doc 1), 359 통과·0 실패·0 무시                                                                                                                                   | `logs/r8b-test-oidc-07be34d.log`(측정 `-pg-activity.txt`·`-db.log`)                                     |
| (c) | `cargo xtask test --supabase --oidc`                  | 06:57:47Z → 07:00:26Z, rc=0            | **PASS** 직접 연결 단계 결과 줄 47, 370 통과·0 실패                                                                                                                                                | `logs/r8c-test-supabase-oidc-07be34d.log`(`-supavisor.log`·`-db.log`·`-pg-activity.txt`)                |
| (d) | `cargo xtask test --supabase --session-pooler --oidc` | 07:00:37Z → 07:05:15Z, rc=0            | **PASS** 직접 연결 단계 47줄·370 통과·0 실패, session pooler 단계 47줄·370 통과·0 실패                                                                                                             | `logs/r8d-test-supabase-session-pooler-oidc-07be34d.log`(`-supavisor.log`·`-db.log`·`-pg-activity.txt`) |

W13과 달리 (d)의 session 단계가 처음부터 끝까지 통과했다(W13 4회 중 3회 collab `XX000`). #89 `01f17fb`(Supabase 두 단계 `RUST_TEST_THREADS=4`)가
들어간 결과로 본다(아래 측정). 한 번 실행이며 반복 실행은 하지 않았다.

### 4.1 연결 수 측정 (W14 `monitor.sh` 방식: 0.25초마다 `pg_stat_activity` client backend를 역할별로 셈, `peaks.sh`로 최댓값)

| 실행                                                        | DB                                            | 한도                                                   | 최고 total           | 최고 app(fvoci_app)  | 최고 owner(migrator) | 한도 오류                                                      |
| ----------------------------------------------------------- | --------------------------------------------- | ------------------------------------------------------ | -------------------- | -------------------- | -------------------- | -------------------------------------------------------------- |
| (b) `--oidc`                                                | 시험 Compose `postgres`(`monitor-test-pg.sh`) | `max_connections` 100, superuser 몫 3 → 비superuser 97 | **85**(06:56:16.715) | **64**(06:56:13.965) | 22                   | `remaining connection slots`·`too many clients` 0건(`-db.log`) |
| (c) `--supabase --oidc`                                     | Supabase db(`monitor-supabase.sh`)            | 100 / 3                                                | 25                   | 12                   | 5                    | 0건, Supavisor `EMAXCONN` 0건                                  |
| (d) 직접 단계                                               | Supabase db                                   | 100 / 3                                                | 25                   | 13                   | 5                    | 0건                                                            |
| (d) session 단계(Supavisor 첫 `mode=session` 07:03:12 이후) | Supabase db                                   | 100 / 3, Supavisor `pool_size` 64                      | 27                   | 17                   | 5                    | 0건, `EMAXCONN` 0건                                            |

- (b) 기본 `--oidc` 스택은 libtest 기본 스레드(32코어 → 32)라 W14 경고대로 한도에 가깝다: 06:56:13~16 사이 표본 5개가 total ≥ 80,
  최고 85/97(앱 64 + owner 22, 측정 연결 1개는 `other`에 포함). 이번엔 넘지 않았다. 여유 12. 시험 수가 더 늘면 이 스택도 넘을 수 있다(#89의
  4스레드 제한은 Supabase 단계에만 적용).
- (b) db 로그의 `FATAL: terminating connection due to administrator command` 5건은 시험이 일부러 끊는 연결이다(시험 실패 없음).

## 5. merge-plan.sh (실행하지 않음)

`merge-plan.sh [시작 순서]`: 단계마다 fetch → main tree == 앞 단계 tree, PR이 열려 있고 머리 == 고정 SHA, base == main(아니면 #62만 예상, main으로
바꿈) → `update`면 `git commit-tree <단계 tree> -p <고정 머리> -p <origin/main>`을 PR 브랜치에 fast-forward push, CI 대기 → `gh pr merge --merge
--match-head-commit` → main tree == 단계 tree 확인(다르면 멈춤). 보호 규칙이 PR을 main 최신으로 요구하면 clean 단계도 같은 갱신 커밋이 필요(주석).
`bash -n`·shellcheck 통과.

## 6. NOT RUN·남은 것

- 실제 브라우저 확인 NOT RUN. R8 각 명령은 한 번씩만 돌렸다(반복으로 간헐 실패율을 재지 않음).
- 단계별 전체 검사는 하지 않았다(최종 SHA에서만 R8). 단계에서는 바꾼 곳의 vitest·svelte-check(#60·#61·#64·#82)와 #64 단계 clippy만.
- 비치명(바꾸지 않음): #68 `api-task-conflict.test.ts:25` 타입 없는 고정 값의 `predecessor_ids`(W5·W13과 같음, 검사 통과).
- #69 최종 머리가 바뀌면 `git checkout -B audit/final audit/final~1 && build.sh 69`로 그 단계만 다시 짓고 R8을 그 SHA에서 다시 돌려야 한다(과거 PASS 재사용 안 함).

## 7. 산출물

`pinned-heads.txt`, `final-steps.tsv`, `merge-plan.sh`, `step.sh`·`build.sh`·`take-resolved.sh`·`reuse-ref-*.sh`·`make-steps-tsv.sh`, `resolutions/<pr>/`,
`resolutions/conflicted-<pr>.txt`, `merge-<pr>.out`, `migrations-0079-0094.txt`, `ancestry.txt`, `diff-stat-a5cda4e-final.txt`, `first-parent-log.txt`,
`run-r8.sh`·`monitor-test-pg.sh`(W14 `monitor.sh` 방식을 기본 스택 postgres에)·`monitor-supabase.sh`(W14 그대로)·`peaks.sh`(W14), `logs/`.
