# W13: 최종 통합 리허설(지금 머리 + #89) — `audit/int6`

- 작업: task_9820d26251fc / dispatch ctx_31c269c9615a. 2026-10-22.
- 기반: main `a5cda4e169e2`. 로컬 브랜치 `audit/int6`(push하지 않음). 원격·PR 브랜치는 고치지 않았다.
- PR 머리: 2026-10-22T03:58:24Z에 `gh pr view <n> --json headRefName,headRefOid,baseRefName,state`로 기록(`pr-heads.txt`). 04:18:20Z에
  다시 봤을 때 27개 모두 그대로였다. #82는 여전히 `1640dbc`(W11의 `String(estimate_minutes)` 변경 전)라 그 변경은 흉내 커밋으로 넣었다(아래 3절 7).
- 병합: 각 PR 머리 SHA를 `git merge --no-ff -m "Merge #N (<브랜치>) in merge order"`로 첫 부모 순서대로(`merge-one.sh`, W5와 같은 스크립트).
- **SHA 요약**: 병합 끝 `0384ab85250c`, 사전 점검 항목 커밋 뒤 `17df953`, 이번에 새로 찾은 고정 값 수정 두 커밋 뒤 **`242754402dde`(시험한 통합 SHA)**.

## 1. PR 머리와 병합 단계 (첫 부모 순)

충돌 해결 "재사용"은 그 파일의 우리 쪽·상대 쪽 blob이 W5 `audit/int5-dry`의 같은 단계 병합 부모와 바이트 단위로 같을 때만 그 병합 결과를
가져온 것이다(`reuse-ref-resolution.sh`). "재사용+재기반"은 한쪽만 달라졌을 때 W5 결과에 그 새 변경만 3-way(`git merge-file`)로 얹은 것이다
(`reuse-ref-rebased.sh`, 충돌 없이 끝난 것만 씀). 각 해결의 `git show --cc`는 `resolutions/`.

| 순서 | PR  | PR 머리        | W5 머리와  | 병합 SHA       | 충돌·해결                                                                                                     |
| ---- | --- | -------------- | ---------- | -------------- | ------------------------------------------------------------------------------------------------------------- |
| 1    | #89 | `251872409dd7` | (새)       | `cafa1efdc743` | —                                                                                                             |
| 2    | #51 | `eb881939efc8` | 같음       | `e3b317ad0c25` | —                                                                                                             |
| 3    | #52 | `591f93917e5f` | 같음       | `7e04496942de` | —                                                                                                             |
| 4    | #53 | `3fc48a5a7fd0` | 같음       | `11cbb2c6d3e0` | —                                                                                                             |
| 5    | #54 | `4c15e3a072c4` | 같음       | `4696b8053ae6` | —                                                                                                             |
| 6    | #58 | `c9a2bceb6359` | 같음       | `0d1da06a9934` | —                                                                                                             |
| 7    | #57 | `40f54cb9ffaf` | 같음       | `e3d1555e70bb` | —                                                                                                             |
| 8    | #59 | `21d92f4f7354` | `df22615`→ | `41fae73fef46` | `tests/recurrence.rs` 재사용(W5 `e7a5cfb`). `docs/development.md` 재사용+재기반(W5 결과 + #89 한 줄)            |
| 9    | #62 | `26615d1493b5` | 같음       | `6602c4063d57` | —                                                                                                             |
| 10   | #65 | `b9f5c2231a46` | `5400977`→ | `f12103876334` | —                                                                                                             |
| 11   | #66 | `5142fb0dab5d` | `611e664`→ | `03fbde7bf491` | `tests/recurrence.rs` 재사용+재기반(W5 `5e0a679` 결과 + #66 새 커밋 `5142fb0`의 +14/−2)                         |
| 12   | #67 | `e3d0df79c398` | 같음       | `16cb1b381d2e` | —                                                                                                             |
| 13   | #74 | `01f028fa3550` | 같음       | `1783e046597b` | `tests/search.rs` 재사용(W5 `634e207`)                                                                        |
| 14   | #75 | `235e95d59c0f` | 같음       | `a6857cb2c499` | —                                                                                                             |
| 15   | #81 | `bf5569669060` | 같음       | `f3dc3e963680` | —                                                                                                             |
| 16   | #55 | `a34a5300ca93` | 같음       | `e17565c20641` | `tests/time_tracking.rs` 재사용(W5 `1698b45`)                                                                 |
| 17   | #56 | `1d7f9fc8b159` | 같음       | `bfcd404a7e45` | —                                                                                                             |
| 18   | #60 | `0345026665c9` | `d913314`→ | `b03ef0c1bc38` | —                                                                                                             |
| 19   | #61 | `97447d837872` | 같음       | `be64037cfeee` | —                                                                                                             |
| 20   | #63 | `324d6e1d6905` | 같음       | `b52b08971172` | `src/features/records.rs`·`tests/records.rs` 재사용(W5 `22e7601`, 인계 5절 1번)                               |
| 21   | #64 | `04467463113a` | `5edd8f4`→ | `a1b4d1be5ca3` | — (0094 `0094_collab_stand_in_editors.sql`가 이제 #64에 있음)                                                 |
| 22   | #68 | `cd2c1bddaf76` | 같음       | `66e131fc6841` | 시험 다섯(`comments`·`home`·`members`·`time_tracking`·`trash`) 재사용(W5 `510b380`). development.md 재사용+재기반 |
| 23   | #72 | `0213eb8be929` | 같음       | `baa97da20565` | —                                                                                                             |
| 24   | #73 | `067ef3630b9e` | 같음       | `8d5436b94046` | —                                                                                                             |
| 25   | #80 | `48c00871864d` | 같음       | `b71fce05206f` | `DisplaySettingsDialog.svelte` 재사용(W5 `0ecbad8`, 인계 5절 3번)                                             |
| 26   | #82 | `1640dbc8db79` | `263e471`→ | `aece90bb6c46` | —                                                                                                             |
| 27   | #69 | `6ed1df82f288` | 같음       | `0384ab85250c` | —                                                                                                             |

전체 SHA: `first-parent-log.txt`(`git log --first-parent --format='%H %P %s' a5cda4e..2427544`), 단계별 출력은 `merge-progress.txt`.

### 1.1 검산

- **재기반한 세 파일**: 결과와 W5 결과의 차이가 정확히 새 변경뿐이다. #59 단계 development.md는 `git diff e7a5cfb` = #89의 59줄 한 줄.
  #66 단계 `recurrence.rs`는 `git diff 5e0a679 HEAD`의 바뀐 줄 = `git diff 611e664 5142fb0`의 바뀐 줄(diff 비교로 같음). #68 단계 development.md는
  `git diff 510b380`의 바뀐 줄 = `git diff 6940547(W5 #64 단계) a1b4d1b(이번 #64 단계)`의 바뀐 줄(같음. #89 한 줄 + #64 `5edd8f4→0446746`의 collab 절).
- **전체 표류 없음**: `git diff 95a201e(W5 병합 끝) 0384ab8(이번 병합 끝)`은 41파일 +3264/−385이고, 바뀐 파일 목록이 #89(`a5cda4e...2518724`)와
  움직인 여섯 머리(#59·#65·#66·#60·#64·#82, 모두 fast-forward)의 차이 파일 합집합과 정확히 같다. 줄 수도 그 합(+223+449+14+287+1794+491+6 /
  −13−48−2−4−264−54)과 같다.
- **마이그레이션**(`migrations-0079-0094.txt`): 0080~0094가 하나씩, 앞 네 자리 중복 없음(`cut -c1-4 | sort | uniq -d` 빈 결과). 처음 들여온 첫 부모
  병합이 인계 4절 표와 같다(0094 ← #64 `0446746`, #81(0093) 뒤).

## 2. 움직인 PR 머리 (W5 대비)

| PR  | W5      | 이번      | 더해진 커밋                                                                                                                                                                                                                           |
| --- | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #59 | df22615 | 21d92f4   | `fix(events): trim like the server and cover newer-version paths after an event conflict (D-94 review)`                                                                                                                              |
| #65 | 5400977 | b9f5c22   | `fix(files): publish a locally assembled upload only while holding the completion claim (#65)`, `fix(storage): treat parts removed while staging as already published (review)`                                                     |
| #66 | 611e664 | 5142fb0   | `test(tasks): keep the two lane lock tests from racing each other and the id bound`                                                                                                                                                 |
| #60 | d913314 | 0345026   | `fix(web): check the signed-in account before a document save goes out (review)`, `fix(web): keep the unsaved-edit warning on an account switch when the draft cannot be kept (review)`                                             |
| #64 | 5edd8f4 | 0446746   | D-95 여섯 커밋(`e564f03` 편집자 기록, `0fb6b04` 이동 잠금 한도, `6b22bd7` 방 없는 복원, `132777f`, `fb3a11e` 문서, `0446746` 복원 409 선언)                                                                                         |
| #82 | 263e471 | 1640dbc   | `fix(web): keep the task overwrite question through late replies and compare fields as the server stores them (D-94 review)`                                                                                                        |
| #89 | —       | 2518724   | `fix(test): cap test Meilisearch CPUs so it stays under its memory limit`, `docs: state the two-CPU minimum for the integration test stack (review)`                                                                                 |

## 3. PR별 갱신 커밋 (통합 브랜치의 별도 커밋, 패치는 `pr-update-patches/`)

각 커밋은 그 PR 갱신 커밋에 그대로 들어갈 모양이다(커밋 메시지에 대상 PR을 적음). PR 브랜치에는 넣지 않았다.

| #   | 커밋      | 대상 PR 갱신           | 내용                                                                                                                                                                | 패치                               |
| --- | --------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| 1   | `548e5ab` | #61 (#59 병합 뒤)      | `apps/web/tests/component/home-week-dates.test.ts`의 `HomeView` 고정 값에 `up_next_counts: { overdue: 0, today: 0, upcoming: 0 },` (사전 점검, 인계 5절 2번)          | `pr61-548e5ab.patch`               |
| 2   | `bdb35e3` | #62 (#58 병합 뒤)      | `item-screen-form.test.ts:273` `predecessor_ids: [],` 삭제 (사전 점검, 인계 5절 4번)                                                                                 | `pr62-bdb35e3.patch`               |
| 3   | `5e188f9` | #56 (#62 병합 뒤)      | `task-csv-import.test.ts:30` `predecessor_ids: [],` 삭제 (사전 점검)                                                                                                  | `pr56-5e188f9.patch`               |
| 4   | `fca9be6` | #82 (#62 병합 뒤)      | `task-conflict-save-mine.test.ts:42` `predecessor_ids: [],` 삭제 (사전 점검)                                                                                          | `pr82-fca9be6.patch`               |
| 5   | `17df953` | #82 — **흉내(W11)**    | `tasks/labels.ts`: `import { formatDay } from '$lib/display'`, 예상 시간 값을 `String(input.estimate_minutes)`로. W11이 #82에 넣을 변경을 흉내 냄. #82 원격 머리는 아직 `1640dbc` | `pr82-simulated-w11-17df953.patch` |
| 6   | `ec7ffe5` | #60 (#57 병합 뒤) **새로 찾음** | #57의 `document-draft-offer.test.ts`·`document-draft-conflict.test.ts` `beforeEach`에 `vi.spyOn(api, 'me').mockResolvedValue({ id: 'user-a', … })`. 아래 4절 1 | `pr60-ec7ffe5.patch`               |
| 7   | `2427544` | #64 (#57 병합 뒤) **새로 찾음** | #57의 `revision-compare-race.test.ts` `RevisionView` 고정 값에 `edited_by_names: null,`(#64 자신의 `revision-restore-conflict.test.ts`와 같은 값). 아래 4절 2 | `pr64-2427544.patch`               |

(nit, 실패 아님, 바꾸지 않음) W5 6절 6과 같이 #68의 `api-task-conflict.test.ts:25` 타입 없는 고정 값에 `predecessor_ids`가 남아 있다.

## 4. 이번에 새로 찾은 통합 문제 (사전 점검 목록에 없던 것)

`17df953`(사전 점검 항목까지)에서 `cargo xtask check`가 **FAIL**(`logs/check-17df953.log`): svelte-check 1건, vitest 3파일 6건.

1. **#57 × #60 `81c9d99`**: #60 새 머리가 `DocumentEditor`의 모든 저장 전에 `api.me()`로 이 창의 계정을 확인하고, 확인하지 못하면 `retrying`으로
   보내지 않는다. #57의 두 시험은 `saveDocument`만 흉내 내 jsdom에서 `/api/me` 요청이 실패한다 → `document-draft-offer.test.ts` 2건
   (`expected '재시도 대기' to be '저장됨'`), `document-draft-conflict.test.ts` 1건(충돌 화면 제목을 못 찾음). 글자 충돌은 없다. #60이 #57 뒤에 병합되므로
   #60 갱신 커밋에서 #57 시험에 `api.me` 흉내를 넣는다(커밋 6). 대안: #60이 `me` 확인을 시험 설정 공통(setup)에서 흉내.
2. **#57 × #64 `e564f03`(D-95)**: `RevisionView`에 `edited_by_names`가 필수로 생겨 #57의 `revision-compare-race.test.ts:26` 고정 값이 svelte-check에서
   `Property 'edited_by_names' is missing`, vitest 3건 `TypeError: Cannot read properties of undefined (reading 'length')`. #64 갱신 커밋에서 넣는다(커밋 7).

두 수정 뒤 세 파일 vitest 10/10 통과, 그 뒤 전체 `cargo xtask check` PASS(5절).

## 5. 검사·시험 결과 (통합 SHA `242754402dde`)

로그는 `logs/`(작업 트리 경로는 `<worktree>`, 홈은 `<home>`으로 바꿈, 비밀값 없음). 환경: WSL2 Linux, 32코어. 시험 결과 줄은 "바이너리 46" =
`Running` 45개(xtask 단위 포함) + `Doc-tests fvoci_server` 1개의 `test result` 줄 수다.

| SHA       | 명령                                                       | 결과                                                                                                                                                                                                                                                                  | 로그                                                         |
| --------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| —         | `pnpm install --frozen-lockfile`                           | PASS                                                                                                                                                                                                                                                                  | `pnpm-install.log`                                           |
| `17df953` | `cargo xtask check`(licenses 포함)                         | **FAIL**: pnpm check(svelte-check 1건), pnpm test(vitest 3파일 6건) — 4절. 나머지(fmt·clippy·unit·cargo deny·openapi·format·lint·i18n·rhwp·third-party·licenses) PASS                                                                                                 | `check-17df953.log`                                          |
| `2427544` | `cargo xtask check`(licenses 포함)                         | **PASS** 전 단계. cargo deny(advisories·bans·sources)·licenses도 이번 기계에서 실행됨(cargo-deny 0.20.2). vitest 123파일·670개                                                                                                                                       | `check-2427544.log`                                          |
| `2427544` | `cargo xtask test --oidc`                                  | **PASS**. 46개 결과 줄, 348 통과·0 실패. `search.rs` 12/12. `docker events`에 `oom` 없음(Meilisearch는 정리 때 exit 143=SIGTERM만)                                                                                                                                   | `test-oidc-2427544.log`, `docker-events-test-oidc.log`       |
| `2427544` | `cargo xtask test --supabase --oidc`                       | **PASS**. 46개, 359 통과·0 실패. `oom` 없음                                                                                                                                                                                                                           | `test-supabase-oidc-2427544.log`, `docker-events-test-supabase-oidc.log` |
| `2427544` | `cargo xtask test --supabase --session-pooler --oidc` ×4   | **FAIL 3/4**(1·2·4회), PASS 1/4(3회). 직접 연결 단계는 네 번 모두 46개·359 통과. session pooler 단계에서 `collab.rs`(6번째 바이너리) 1~2건이 HTTP 500 `database XX000`으로 실패해 거기서 멈춤(뒤 40개 바이너리는 그 회차에 NOT RUN). 3회차는 session도 46개·359 통과. 아래 6절 | `test-supabase-session-pooler-oidc-2427544{,-run2,-run3,-run4}.log`, `supavisor-run{3,4}-2427544.log`, `supabase-db-run4-2427544.log` |

Meilisearch OOM: #89가 고쳤다. 통합의 `--oidc`·`--supabase --oidc`·session 명령 4회의 직접 연결 단계(와 끝까지 간 3회차의 session 단계)에서 모두 `search.rs` 12/12 통과. `docker events`(oom·die)는 `--oidc`·`--supabase --oidc`·session 1회차에서만 받았고 세 번 모두 `oom` 없음(나머지 회차의 근거는 로그의 `search.rs` 12/12). 반대로 #89가 없는 #64 머리
단독(`0446746`)은 같은 기계에서 `search.rs` 6건 실패로 멈췄다(`pr64-0446746-alone-test-supabase-session-pooler-oidc-stopped-at-search-oom.log`) — W5 7절 증상.

## 6. session pooler 실패: #64의 `collab.rs`가 Supavisor session 풀(64)을 넘김

- 실패한 시험(회차마다 다름): `no_room_opens_while_a_document_moves`(1·2회), `file_refs_lock_versions_before_they_are_written`(2회, main 시험),
  `trashed_document_keeps_live_file_pins`(4회). 모두 같은 순간의 새 DB 연결에서 서버가 `database XX000` 500을 낸다(실패 자리는 시험마다 다르며 시험 논리와 무관).
- 원인(로그로 확인): 4회차 Supavisor 로그 `04:41:24.340 … (EMAXCONNSESSION) max clients reached in session mode - max clients are limited to pool_size: 64`,
  0.5초 뒤 시험의 `XX000`(서버 오류 시각 `04:41:24.841`, 요청 511ms). 같은 순간 Postgres도 `postgres@postgres FATAL: remaining connection slots are reserved
  for roles with the SUPERUSER attribute`(DB `max_connections`도 가득). 통과한 3회차 로그에는 `EMAXCONN` 없음. `deploy/supabase/test.env`
  `POOLER_DEFAULT_POOL_SIZE=64`, 풀 크기를 20→64로 올린 같은 증상의 기록이 development.md 85.4절에 있다.
- 어느 PR인가: **#64 자체**(통합 문제가 아님). #64 머리 + #89만 합친 진단용 커밋 `50a77db`(로컬 태그 `w13-diag-pr64-pr89`, push 안 함)에서 같은 명령 2회: 1회 PASS(session
  45개·312 통과), 2회 **FAIL**(`rooms_acknowledge_persisted_state_with_a_snapshot`, 같은 `XX000`, Supavisor `EMAXCONNSESSION` 1건, `04:45:53.010` → 시험 오류
  `04:45:53.511`). `collab.rs` 시험 수: main 19 → W5의 #64 `5edd8f4` 27 → 지금 `0446746` 36. 한 바이너리 안의 시험이 각자 `build_state`(앱 풀 최대 16)와
  소유자 풀을 동시에 열어 session 모드(클라이언트마다 서버 연결)에서 64를 넘는다(시험 수 증가가 원인이라는 것은 추정, 연결 수를 직접 세지는 않음).
  통합 쪽이 더 자주 실패한 것(3/4 대 1/2)은 표본이 작아 구분하지 않는다.
- 이 작업에서는 고치지 않았다(시험 구성 변경 = #64 갱신 또는 별도 PR, coordinator 판단). 방향: `test.env`의 `POOLER_DEFAULT_POOL_SIZE`를 올리되 DB
  `max_connections`도 같이 봐야 함(이번에 DB 쪽도 가득 참), 또는 session 단계의 시험 동시 실행 수·시험 앱 풀 크기를 줄임, 또는 #64의 새 `collab.rs`
  시험이 풀을 덜 쓰게(예: 소유자 풀을 `max_connections(1)`로).
- 로그: `pr64-0446746+pr89-test-supabase-session-pooler-oidc{,-run2}.log`, `supavisor-pr64+pr89-run2.log`.

## 7. NOT RUN·남은 것

- 최종 R8 아님: #64(W1b)·#60(W10)·#59·#82(W11)가 갱신 중이라 머리가 또 바뀐다. #82 W11 변경은 흉내(커밋 5).
- session pooler 실패 회차의 session 단계 나머지 바이너리(collab 뒤 40개)는 그 회차에 NOT RUN. 3회차에서는 모두 통과.
- 실제 브라우저 확인 NOT RUN.
- main + #89 기준의 session pooler 실행은 NOT RUN(#64 + #89에서 재현돼 귀속에는 필요 없다고 봄).

## 8. 산출물

- `README.md`(이 파일), `pr-heads.txt`(번호·브랜치·머리·base·상태), `first-parent-log.txt`, `merge-progress.txt`, `conflicts.txt`, `resolutions/*-cc.diff`,
  `pr-update-patches/*.patch`, `migrations-0079-0094.txt`, `w5-merge-refs.txt`(재사용 기준 W5 병합 SHA), `logs/`.
- 스크립트: `merge-one.sh`·`reuse-ref-resolution.sh`(W5와 같음), `reuse-ref-rebased.sh`(이번에 더함), `run-with-heartbeat.sh`(명령 실행 + 5분마다 heartbeat).
- 로컬 브랜치 `audit/int6` = `242754402dde`. 진단용 `#64+#89` 병합은 로컬 태그 `w13-diag-pr64-pr89`(`50a77db`). 둘 다 push하지 않았다.
