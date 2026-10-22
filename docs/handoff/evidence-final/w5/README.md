# W5: 병합 순서 통합 시험(dry-run) — `audit/int5-dry`

- 작업: task_4fe2de1a2168 / dispatch ctx_033d698c8a88. 2026-10-22.
- 기반: main `a5cda4e169e2`. 로컬 브랜치 `audit/int5-dry`(push하지 않음). 원격·PR 브랜치는 고치지 않았다.
- PR 머리: `gh pr view <n> --json headRefName,headRefOid`로 확인. 26개 모두 인계 문서 1.1절 표의 SHA와 같고 `origin/<브랜치>`와도 같다
  (#59 `df22615`, #64 `5edd8f4`, #82 `263e471` = 인계 체크포인트 머리. 다른 작업자가 갱신 중이므로 최종 통합에서 다시 한다).
- 병합: 각 PR 머리 SHA를 `git merge --no-ff -m "Merge #N (<브랜치>) in merge order"`로 첫 부모 순서대로.

## 1. 병합 표 (첫 부모 순)

충돌 해결 "재사용"은 그 파일의 우리 쪽·상대 쪽 blob이 `origin/handoff/ref-int4-dry`의 같은 단계 병합 부모와 **바이트 단위로 같을 때만**
그 병합 결과 파일을 가져온 것이다(스크립트로 blob SHA 비교). 같지 않은 파일은 손으로 풀었다.

| 순서 | PR  | 병합 SHA       | PR 머리        | 충돌 파일                                                                                     | 해결                                                                                                                         |
| ---- | --- | -------------- | -------------- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 1    | #51 | `5c3046f9b0b0` | `eb881939efc8` | —                                                                                             |                                                                                                                              |
| 2    | #52 | `bdda3023c3bd` | `591f93917e5f` | —                                                                                             |                                                                                                                              |
| 3    | #53 | `d9f2a6d9328d` | `3fc48a5a7fd0` | —                                                                                             |                                                                                                                              |
| 4    | #54 | `3b1cabb51f9e` | `4c15e3a072c4` | —                                                                                             |                                                                                                                              |
| 5    | #58 | `c4bdbae98b5d` | `c9a2bceb6359` | —                                                                                             |                                                                                                                              |
| 6    | #57 | `f502257cffbc` | `40f54cb9ffaf` | —                                                                                             | 트리가 ref-int4-dry `7e71371`과 같음(diff 0줄)                                                                               |
| 7    | #59 | `e7a5cfbf16a5` | `df2261597ccc` | `crates/server/tests/recurrence.rs`, `docs/development.md`                                    | 재사용(ref `3eccf00`). 인계 5절 5번: 시험 둘 다 끝에(#54 S18 → #59 W21), 26절 "반복 일정"은 #59 문장 뒤에 #54(0083) 문장     |
| 8    | #62 | `79a60c079473` | `26615d1493b5` | —                                                                                             |                                                                                                                              |
| 9    | #65 | `33ae165cefc9` | `540097718c4c` | —                                                                                             |                                                                                                                              |
| 10   | #66 | `5e0a679812be` | `611e6644adbe` | `crates/server/tests/recurrence.rs`                                                           | 재사용(ref `dfeec73`). 5번: 앞의 둘 + #66의 두 시험을 끝에 모두                                                              |
| 11   | #67 | `b5793a305e57` | `e3d0df79c398` | —                                                                                             |                                                                                                                              |
| 12   | #74 | `634e207ae975` | `01f028fa3550` | `crates/server/tests/search.rs`                                                               | 재사용(ref `8a0d334`). 7번: 끝에 덧붙인 시험 양쪽 유지                                                                       |
| 13   | #75 | `f47440ba5a50` | `235e95d59c0f` | —                                                                                             |                                                                                                                              |
| 14   | #81 | `82b0dff4d657` | `bf5569669060` | —                                                                                             |                                                                                                                              |
| 15   | #55 | `1698b45886ce` | `a34a5300ca93` | `crates/server/tests/time_tracking.rs`                                                        | 재사용(ref `8135259`). 7번: 양쪽 유지                                                                                        |
| 16   | #56 | `26639a80b0d0` | `1d7f9fc8b159` | —                                                                                             |                                                                                                                              |
| 17   | #60 | `d1126080b670` | `d913314a94a9` | —                                                                                             |                                                                                                                              |
| 18   | #61 | `4c0ced2d8dda` | `97447d837872` | —                                                                                             |                                                                                                                              |
| 19   | #63 | `22e76019ea96` | `324d6e1d6905` | `crates/server/src/features/records.rs`, `crates/server/tests/records.rs`                     | 재사용(ref `925f7c0`). 1번: #63의 `run` 재시도 구조 + `input.fields.clone()`, #58의 `kept` 자리에 `None`. 시험 양쪽 유지     |
| 20   | #64 | `69405470d929` | `5edd8f4545e1` | —                                                                                             | #64 새 머리(옛 `2e1d3a1` → `5edd8f4`)도 충돌 없음                                                                            |
| 21   | #68 | `510b380c4b40` | `cd2c1bddaf76` | `tests/comments.rs`·`home.rs`·`members.rs`·`time_tracking.rs`·`trash.rs`, `docs/development.md` | 시험 다섯은 재사용(ref `b89b387`, 7번 양쪽 유지). development.md는 손으로(아래 2절)                                        |
| 22   | #72 | `e72cfc0f2191` | `0213eb8be929` | —                                                                                             | #59 새 머리(D-94 EventDialog)와도 글자 충돌 없음                                                                             |
| 23   | #73 | `ec421f04a434` | `067ef3630b9e` | —                                                                                             |                                                                                                                              |
| 24   | #80 | `0ecbad83dfc0` | `48c00871864d` | `apps/web/src/lib/features/settings/DisplaySettingsDialog.svelte`                             | 재사용(ref `cd2d9b6`). 3번: `untrack` 가져오기 + 여는 효과 `untrack(() => {…})`, `api`만 + `errorText`, `appliedDocFont()` |
| 25   | #82 | `77271a6c2fc4` | `263e47163b29` | —                                                                                             | (ref-int4-dry에 없던 단계)                                                                                                   |
| 26   | #69 | `95a201e89467` | `6ed1df82f288` | —                                                                                             | 문서, 마지막                                                                                                                 |

전체 SHA: `git log --first-parent --format='%H %P %s' a5cda4e..95a201e`.

검산: #80 단계 `0ecbad8`와 ref-int4-dry 끝 `cd2d9b6`의 차이는 정확히 #59 `dbd1861→df22615`(7파일 +919/−33)와 #64 `2e1d3a1→5edd8f4`
(4파일 +943/−104)의 합이다(바뀐 파일 목록이 같고 줄 수 합 11파일 +1862/−137이 같다).

## 2. 새로 손으로 푼 충돌

- **#68 단계 `docs/development.md` 27절**(인계 5절 6·8번과 같은 덩어리 하나). ref-int4-dry와 달리 우리 쪽이 #64 새 머리로 이 파일의
  다른 곳(CI·collab 절 등)이 바뀌어 blob이 달라 재사용 조건을 벗어났다. 덩어리 자체는 같아서 #68의 27절 줄(구조의 `api` 설명,
  "이후 더해진 작업" 하위 줄, 권한 경계 개정)을 쓰고 "주기 작업" 줄에 #66 문장 "반복 회차 맞추기는 … 교착이 났다)."를 넣었다
  (ref `b89b387` 520–523줄과 같은 내용). 검산: `git diff b89b387 -- docs/development.md`의 바뀐 줄이 `git diff 2e1d3a1 5edd8f4 --
  docs/development.md`의 바뀐 줄과 정확히 같다(= ref 해결 + #64 새 변경만).

## 3. 인계 문서와 다르게 관찰된 것

- 인계 5절 5번은 "#59/#66"이라 적었지만 #59 단계의 `recurrence.rs`·development.md 26절 충돌 상대는 **#54**(S18 시험
  `moving_a_repeating_event_past_its_until_is_refused`와 0083 문장)다. #66 단계에서는 `recurrence.rs`만 충돌했고 development.md 26절은
  충돌하지 않았다. 해결 결과는 인계 설명과 같다.
- #58/#63 `records.rs` 충돌은 #63 단계에서 났다(우리 쪽에 #58이 이미 있음). 해결은 1번 그대로.

## 4. 마이그레이션 0080→0093

통합 트리(`95a201e`·`ae68989`)의 `crates/server/migrations/` 실제 파일 기준(`migrations-0079-0093.txt`):

- 0080~0093이 하나씩, 번호 중복 없음. 앞번호가 같은 파일 없음(`cut -c1-4 | uniq -d` 빈 결과).
- 각 파일을 처음 들여온 첫 부모 병합: 0080 #51, 0081 #52, 0082 #53, 0083 #54, 0084 #58, 0085 #57, 0086 #59, 0087 #62, 0088 #65,
  0089 #66, 0090 #67, 0091 #74, 0092 #75, 0093 #81 — 인계 4절 표와 같고 병합 순서대로 번호가 오른다.
- 번호 파일 92개 = main의 78개 + 14개. 전체에서 빠진 번호는 0020 하나이며 main에도 없다(`migrations/README.md` 10줄 "현재 이력에는
  `0020`이 없으며"). 새로 생긴 누락 없음.
- **0094는 #64 머리 `5edd8f4`에 없다.** `origin/wip/audit-d95-stand-in-record`에만 `0094_collab_stand_in_editors.sql`이 있다(인계 3.3절과 같음).
  D-95를 마친 #64가 들어오면 0093 다음에 0094가 와야 한다(#64는 #81 뒤 병합이라 순서는 맞음).

## 5. 검사·시험 결과

로그: `target/audit-evidence/w5/logs/`(작업 트리 경로는 `<worktree>`로 바꿈, 비밀값 없음). 환경: WSL2 Linux, 32코어, 73GiB.

| SHA                     | 명령                                                         | 결과                                                                                                                                                                                                                                                                                                       | 로그                                                                     |
| ----------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `95a201e`(병합만)       | `pnpm install --frozen-lockfile`                             | PASS                                                                                                                                                                                                                                                                                                       | `pnpm-install.log`                                                       |
| `95a201e`(병합만)       | `cargo xtask check --skip-licenses`                          | **FAIL**. PASS: fmt, clippy, Rust 단위, openapi 최신, prettier, i18n, rhwp 고지. FAIL: eslint 2건·svelte-check 5건·third-party notices(vite build)·vitest 1건(아래 6절 1~4), cargo deny = 도구 없음                                                                                                       | `check-95a201e-raw.log`, `web-{eslint,svelte-check,vitest}-95a201e-raw.log` |
| `ae68989`(+통합 전용)   | `cargo xtask check --skip-licenses`                          | cargo deny 빼고 **모두 PASS**(vitest 121파일·638개). cargo deny(advisories·bans·sources)는 **NOT RUN**: 이 환경에 `cargo-deny`가 없음(`error: no such command: deny`). 설치하지 않았다(기계 전역 도구 변경)                                                                                                | `check-ae68989.log`                                                      |
| `ae68989`               | `cargo xtask test --oidc` ×5                                 | **FAIL** 다섯 번 모두. `search.rs` 4~5건(아래 7절 환경 원인)으로 cargo가 그 바이너리에서 멈춤(31/45 바이너리). 2회차는 그보다 앞의 `recurrence.rs` 1건(6절 5)으로 25번째에서 멈춤. OIDC 시험 5건은 매번 통과                                                                                               | `test-oidc-ae68989.log`, `-run2`~`-run5.log`                             |
| `ae68989`               | `cargo xtask test --oidc` + 임시 `--no-fail-fast`            | 45/45 바이너리 실행, **330 통과·5 실패(모두 `search.rs`)**. 진단용으로 `xtask/src/test.rs` 29줄에 `--no-fail-fast`를 커밋 없이 넣었다가 바로 되돌림(`git status` 깨끗)                                                                                                                                     | `test-oidc-ae68989-nofailfast.log`                                       |
| main `a5cda4e`(기준)    | `cargo xtask test --oidc`                                    | **FAIL**, 같은 `search.rs` 4건, 같은 증상 → 통합이 만든 실패가 아님                                                                                                                                                                                                                                        | `baseline-main-a5cda4e-test-oidc.log`                                    |
| `ae68989`               | 5회차 동안 `docker events`(시험 Meilisearch 컨테이너)        | `oom` → `die exitCode=137`. 시험 중 Meilisearch가 메모리 초과로 죽음                                                                                                                                                                                                                                       | `docker-events-meili-run5.log`                                           |

`95a201e`(병합만)에서는 `cargo xtask test --oidc`를 따로 돌리지 않았다. `ae68989`와의 차이가 `apps/web/**` 다섯 파일뿐이라(`git diff --name-only
95a201e ae68989`) Rust 통합 시험 결과는 같다.

NOT RUN: `--supabase --oidc`, `--supabase --session-pooler --oidc`(지시 범위 밖·사전 점검), cargo deny, `cargo xtask licenses`, 실제 브라우저.

## 6. 통합 전용으로 필요했던 변경 → PR별 갱신 목록

통합 전용 커밋 `ae68989`(`integration-only-ae68989.patch`)에 모았다. PR 브랜치에는 넣지 않았다. 인계 5절 끝의 원칙대로 "뒤에 병합되는 PR"의
갱신 커밋에 넣어야 최종 `git diff <통합> origin/main`이 빈다.

1. **#61 갱신**(#59 병합 뒤): `apps/web/tests/component/home-week-dates.test.ts`(#59 파일) 고정 값에
   `up_next_counts: { overdue: 0, today: 0, upcoming: 0 },` 추가. 없으면 svelte-check `23:7 Property 'up_next_counts' is missing ... 'HomeView'`. (인계 5절 2번)
2. **#62 갱신**(#58 병합 뒤): `apps/web/tests/component/item-screen-form.test.ts:273`(#58 파일)의 `predecessor_ids: [],` 삭제. 없으면
   svelte-check `'predecessor_ids' does not exist in type 'TaskView'`. (4번)
3. **#56 갱신**(#62 병합 뒤): `apps/web/tests/component/task-csv-import.test.ts:30`의 `predecessor_ids: [],` 삭제(`TaskDetailView`). (4번)
4. **#82 갱신 — 새로 찾은 것 둘**(#62 병합 뒤):
   - `apps/web/tests/component/task-conflict-save-mine.test.ts:41`의 `predecessor_ids: [],` 삭제(`TaskDetailView`, #82가 main 기준으로 새로 쓴 고정 값).
   - **`display.formatNumber` 복원**: #62(D-92, `26615d1`)가 "호출자 없음"으로 `apps/web/src/lib/display.ts`의 `formatNumber`를 지웠는데,
     #82의 `apps/web/src/lib/features/tasks/labels.ts:3`·`:161`(예상 시간 값 표시, `8ba72ad`·`70435e4`)이 그것을 쓴다. 글자 충돌은 없지만
     없으면 eslint(`labels.ts:159:7` no-unsafe-return, `:161:11` no-unsafe-call), svelte-check(`'$lib/display' has no exported member 'formatNumber'`),
     vite build(third-party notices), vitest(`task-conflict-save-mine.test.ts` "asks before saving my value over fields the other person changed too" —
     `TypeError: formatNumber is not a function`로 확인 상자가 안 뜸)가 모두 실패한다. 권고: #82 갱신 커밋이 main의 정의를 그대로 되살린다
     (`/** 숫자(천 단위 구분 등). */ export function formatNumber(value, settings = displaySettings()) { return new Intl.NumberFormat(localeTag(settings)).format(value); }`,
     `ae68989` 적용 후 `display.ts`는 main과 바이트 단위로 같음). 대안: #62가 `formatNumber` 삭제만 빼거나, #82가 `Intl.NumberFormat(localeTag())`를 직접 쓴다.
5. **#66 — 불안정 시험(통합 전용 아님, 권고)**: `crates/server/tests/recurrence.rs` `a_lane_lock_failure_skips_only_that_item_in_the_recurrence_tick`이
   6번 중 1번 실패(`test-oidc-ae68989-run2.log`: `the other item lost its occurrences`, 직전 경고 `recurrence sync of series … failed with SQLSTATE FV001`).
   원인(추정 — 소스·로그 근거, 직접 재현하지는 않음): 시험이 `app_system_recurrence_tick_ids()`로 전체 반복의 새 id 수를 본 뒤 `app_system_recurrence_tick`을 한 번만 부르는데, 같은
   바이너리의 다른 시험이 그 사이 반복을 만들면 id가 모자라(FV001) 뒤 반복이 건너뛰어진다. 작업 프로세스는 FV001에서 다시 하지만(`worker.rs:91`,
   `SHORT_ID_ATTEMPTS`) 시험은 그러지 않는다. 고칠 방향: 남는 id는 버려지므로(`db/mod.rs:105`) `need`보다 넉넉히 주거나, FV001 경고가 나면 되돌리고
   다시 하기. #66 단독에도 있는 경합이고, 통합에서 같은 바이너리의 시험(#54·#59 일정 시험)이 늘어 더 잘 날 수 있다(추정).
6. (nit, 실패 아님) #68의 `apps/web/tests/component/api-task-conflict.test.ts:25` 타입 없는 고정 값에 `predecessor_ids`가 남아 있다. #62 뒤
   정리하려면 #68 갱신에서 지운다.

## 7. 환경 문제(통합과 무관, R8 전에 coordinator 판단 필요)

- **시험 Meilisearch OOM**(OOM 자체는 `docker events`로 확인. 원인 해석은 추정): `deploy/compose.test.yaml`의 meilisearch는 `memory: 512m` 한도에
  데이터가 tmpfs(`/meili_data`)다. tmpfs 사용량이 컨테이너 메모리 한도에 들어가고 코어 수가 많아 색인 메모리가 커졌을 것으로 보지만 확인하지 않았다.
  이 기계(WSL2, 32코어)에서 `search.rs` 병렬 시험 중 컨테이너가 `oom`·exit 137로 죽고, 진행 중이던 요청이 같은 순간 `search service request`로
  실패해 `drain`이 "made no progress"를 낸다. main `a5cda4e`에서도 같다. 이 기계에서 R8을 돌리려면 시험 구성 변경(별도 PR·사용자/coordinator 판단)이나
  다른 환경이 필요하다. 이 작업에서는 고치지 않았다.
- **cargo-deny 없음**: `cargo xtask check`의 cargo deny 단계와 `cargo xtask licenses`를 이 기계에서 돌릴 수 없다.

## 8. 이 시험 뒤 움직인 PR 머리 (2026-10-22T02:40:29Z 재확인)

| PR  | 시험한 머리 | 지금 머리  | 더해진 커밋                                                                                                    |
| --- | ----------- | ---------- | -------------------------------------------------------------------------------------------------------------- |
| #59 | `df22615`   | `21d92f4`  | `fix(events): trim like the server and cover newer-version paths after an event conflict (D-94 review)`         |
| #60 | `d913314`   | `81c9d99`  | `fix(web): check the signed-in account before a document save goes out (review)`                              |
| #82 | `263e471`   | `1640dbc`  | `fix(web): keep the task overwrite question through late replies and compare fields as the server stores them` |

나머지 23개는 그대로. 6절 4번은 #82 `1640dbc`에도 그대로 해당한다(`labels.ts:3`·`:181`이 `formatNumber`를 씀, `task-conflict-save-mine.test.ts:42`에
`predecessor_ids`). 새 머리들로는 병합·시험하지 않았다(최종 통합에서 coordinator가 다시 한다).

## 9. 산출물

- `README.md`(이 파일), `pr-heads.txt`(PR 번호·브랜치·머리·base·상태), `first-parent-log.txt`, `merge-one.sh`·`reuse-ref-resolution.sh`(쓴 보조 스크립트),
  `pr68-development-md-cc.diff`(손으로 푼 병합의 `git show --cc`), `integration-only-ae68989.patch`, `migrations-0079-0093.txt`, `logs/`.
- 로컬 브랜치 `audit/int5-dry` = `ae68989`(병합 끝 `95a201e` + 통합 전용 커밋 1개). push하지 않았다.
