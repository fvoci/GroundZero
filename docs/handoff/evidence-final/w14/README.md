# W14: session pooler 시험에서 #64 collab 시험이 Supavisor 풀·PG 연결을 다 쓰는 문제

- 작업: task_0509ffec6947 / dispatch ctx_86fb710ebf75. 2026-10-22. 기계: WSL2 Linux, 32코어(`nproc` 32).
- 진단 브랜치 `diag`(로컬, push 안 함; 뒤에 `diag2` = #64 새 머리 `2acebe9` + #89 = `2ddb17c`, 2절) = `origin/fix/audit-move-live-snapshot`(#64 `56e9108`) + `origin/fix/test-meili-memory`(#89 `2518724`)
  병합 `891f40e`. 기준선 = main `a5cda4e` + #89 = #89 머리 `2518724`(worktree `w14-base`).
- **결론**: 제품 결함(누수·과다)이 아니라 시험 구성 문제다. #64가 collab 시험을 19개 → 41개로 늘려 32코어 기계의 libtest 기본 스레드(32)를
  넘었고, 시험마다 따로 만든 앱 상태(풀 최대 16)가 동시에 32개 돌며 앱 연결 64(= Supavisor session `pool_size`)와 db의 non-superuser 연결 97개
  가운데 95개를 썼다. **수정**: `cargo xtask test --supabase`의 두 단계(직접·session)에서 `RUST_TEST_THREADS=4`(= 64 / 16). #89 브랜치에 커밋
  `01f17fb`(push SHA는 아래 6절).

## 1. 측정 방법

- `monitor.sh`: 시험 중 Supabase db 컨테이너 안에서 `pg_stat_activity`(client backend)를 0.25초마다 역할별로 센다(superuser
  `supabase_admin`의 연결 1개를 측정에 씀, `other` 열에 들어감). 같은 때 Supavisor·db 컨테이너 로그를 받는다(`logs/<회차>-*`).
  - 열: `total`, `app`(fvoci_app: session 단계에서는 Supavisor가 잡은 서버 연결), `owner`(fvoci_migrator: 시험의 `FVOCI_TEST_MIGRATION_URL`
    직접 연결), `pg_admin`(postgres: 시험의 관리자 직접 연결), `other`(Supabase 자체: Supavisor 메타데이터, PostgREST 등 + 측정 1).
    `app_direct` 열은 Supavisor IP 비교가 맞지 않아 `app`과 같다(쓰지 않음).
  - db `max_connections` 100, `superuser_reserved_connections` 3 → superuser가 아닌 역할(`postgres` 포함)은 97개까지
    (`logs/*-max-connections.txt`).
- `run.sh <이름> <xtask test 인자>`: 시험 + 측정 + 5분마다 heartbeat. `peaks.sh <pg-activity> [시작] [끝]`: 구간 최댓값. 단계 경계는 Supavisor
  로그의 첫 `mode=session` 줄.

## 2. 원인 증거

| 회차 | 커밋 | 결과 | 직접 단계 최고 total / app / owner | session 단계 최고 total / app / owner | `EMAXCONN` | db 슬롯 가득 |
| --- | --- | --- | --- | --- | --- | --- |
| base run1 | `2518724`(main+#89) | **PASS** 90/90 결과 줄 | 50 / 35 / 7 | 53 / 36 / 7 | 0 | 0 |
| base run2 | `2518724` | **PASS** 90/90, 586 통과 | 50 / 36 / 6 | 54 / 37 / 7 | 0 | 0 |
| diag run1 | `891f40e`(#64 `56e9108`+#89, 수정 전) | **FAIL** session collab 2건 | 89 / 62 / 19 | **95 / 64 / 18** | 2 | 3 |
| diag2 run1 | `2ddb17c`(#64 새 머리 `2acebe9`+#89, 수정 전) | **FAIL** session 2건(`workspace_export_includes_live_edits`, `no_room_opens_while_a_document_moves`) | 90 / 62 / 20 | **96 / 63 / 21** | 2 | 5 |
| diag2 run2 | `2ddb17c` | PASS 90/90, 634 통과 — 그러나 session 단계 `05:54:39`에 db 슬롯 가득 4건(`postgres` 관리자 연결 거절. 시험은 통과) | **95** / 64 / 19 | **95** / 63 / 19 | 0 | **4** |
| diag2 run3 | `2ddb17c` | **FAIL** session 2건(`workspace_export_includes_live_edits`, `write_access_removed_while_a_move_waits`) | 90 / 61 / 19 | **95 / 63 / 19** | 2 | 5 |

- coordinator 지시(05:17Z)로 진단 기준을 #64 새 머리 `2acebe9`+#89(`diag2`)로 옮겼다. `56e9108..2acebe9`는 웹·문서만 바꿨고
  (`git diff --stat 56e9108 2acebe9 -- crates xtask deploy Cargo.lock` 빈 결과) collab 시험은 그대로 41개다. 수정 전 재현율 **3번 중 2번 FAIL**,
  통과한 1번도 db 한도(97)에 닿았다. W1b의 `cfae027`+#89 1회 PASS는 이 간헐성과 맞는다.

- diag run1 실패(`logs/diag-891f40e-sp-run1.log`): session 단계 `collab.rs` `write_access_removed_while_a_move_waits`(`POST /api/workspaces`
  500 `database XX000`, 503ms), `no_room_opens_while_a_document_moves`(웹소켓 업그레이드 500). 39 통과·2 실패. 뒤 바이너리는 NOT RUN.
- 같은 순간: Supavisor `04:57:54.177`·`.194` `(EMAXCONNSESSION) max clients reached in session mode - max clients are limited to pool_size: 64`
  (`logs/diag-891f40e-sp-run1-supavisor.log` 440·442행) → 서버 500 `04:57:54.679`. db `04:57:56.810~.874` `postgres@postgres FATAL: remaining
  connection slots are reserved for roles with the SUPERUSER attribute` 3건(`-db.log` 444~446행). 측정 `04:57:54.197` app 64, total 91 →
  `04:57:56.947` total 95(`-pg-activity.txt`).
- **수요 자체가 한도에 붙어 있다**: 풀러가 없는 직접 단계에서도 collab 바이너리가 시작 0.5초 안에 앱 연결 62개(+ owner 직접 19개, total 89/97)를
  동시에 연다(`04:56:20`). session 단계는 이 수요가 64에서 잘리고(넘는 클라이언트는 기다리지 않고 거절), Supavisor가 쥔 서버 연결 64 + 직접 연결로
  db도 97에 닿는다. 넘을지는 순간의 겹침에 달려 회차마다 다르다(W13: 4번 중 3번, 2번 중 1번).
- 기준선(main collab 19개)은 32스레드에서도 전 구간 최고 app 36·total 54라 여유가 크다. 시험 수: main 19 → #64 `56e9108` 41
  (`#[tokio::test]` 수). 32보다 많아져 한 바이너리에서 32개가 동시에 돌고, 시험마다 앱 연결 약 2개(방 작업 + HTTP 요청)를 쥔다(62 / 32).

### 2.1 제품 결함(누수·과다)은 아니다

- 서버 하나의 풀 상한 16(`crates/server/src/db/mod.rs:68-71` `max_connections(16)`, `acquire_timeout` 5초)은 바뀌지 않았고, #64의 서버
  변경(`collab/room.rs`, `features/documents.rs`)에 풀 밖 연결(`PgConnection::connect`, `PgListener`)이 없다. 풀 밖 연결은
  `backup.rs`·`db/admin.rs`(명령)뿐이다.
- 이동 대기 중 재확인(`room.rs:1048-1063` `move_rechecking`): 이동 트랜잭션이 연결 1개를 쥐고 기다리는 동안 재확인이 같은 풀에서 연결을 하나 더
  차례로(`session_alive` → `peer_access`, 각각 커밋 후 반납) 쓴다. 요청 하나가 최대 2개, 풀 상한 안이다.
- 누수라면 직접 단계에서 풀이 고갈돼 `PoolTimedOut` 500이 나야 하는데, 수정 전·후 Supabase 직접 단계 9회(diag run1, 수정 후 8회. 앞의 4회는 32스레드 그대로)에서 실패가 없다. 시험이 끝나면
  연결 수가 Supabase 자체 몫(9~10)으로 돌아온다(예: diag run1 `04:57:46` app 0).
- 그러므로 늘어난 것은 "동시에 도는 시험(서버) 수 × 서버당 약 2"이고, 서버 하나는 설정 상한을 지킨다.

## 3. 수정과 근거

- 위치: `xtask/src/test.rs` `run_supabase` — 직접 연결 단계 앞에 `let _threads = sh.push_env("RUST_TEST_THREADS", "4");`(두 단계 모두 적용,
  `--supabase`만일 때도). `deploy/supabase/test.env` 주석("넉넉히" → 4개로 묶는다), `docs/development.md` 85.3절 한 항목.
- 왜 4인가: session 모드는 클라이언트마다 서버 연결을 잡고 풀(64)을 넘으면 거절한다. 시험마다 서버(풀 최대 16) 하나이므로 64 / 16 = 4면 모든
  시험의 풀이 꽉 차도 넘지 않는다. 코어 수·시험 수와 상관없이 같다. CI 러너(4코어)는 원래 4스레드라 바뀌지 않는다.
- 왜 직접 단계도: db `max_connections` 100에서 superuser 몫 3과 Supabase 자체 서비스 약 10을 빼면 87이고, 첫 수정(session 단계만, 아래
  `13b0e7a`)에서 직접 단계가 92·94·93까지 썼다(97 한도). 같은 원리로 같이 묶었다.
- 고르지 않은 것:
  - Supavisor `POOLER_DEFAULT_POOL_SIZE`·db `max_connections` 올리기: 고정 용량이라 코어가 더 많은 기계나 시험이 더 늘면 다시 난다(85.4절에 20 → 64로
    올린 같은 증상의 기록). db 쪽은 공식 db 서비스에 `command:` 덮어쓰기가 필요하다.
  - 서버 풀 크기를 시험용으로 줄이기: 상한 16은 서버 코드에 고정이라 제품 설정을 새로 만들어야 하고, 실제 수요(서버당 약 2)는 그대로라 효과가 없다.
  - collab 시험의 owner 풀(`sqlx::PgPool::connect`, 기본 최대 10)을 1로: 직접 연결만 줄고 Supavisor 64는 그대로다(#64 코드 패치 필요).
- 운영 기본값·검사 규칙은 바꾸지 않았다. 시험 수·내용도 그대로다(건너뛰는 시험 없음).
- 비용(같은 시험 묶음 634개, 빌드 끝난 뒤): 수정 전 diag2 run2(통과한 회차) 전체 3분 22초·session 단계 86초 → 수정 후 `77eea03` run2·run3 전체 4분 28초·session 118~119초. 약 +65초(직접 단계 약 +33초, session 약 +32초). CI 러너(4코어)는 원래 4스레드라 바뀌지 않는다.

## 4. 검증 (수정을 얹은 `diag`)

첫 수정 `13b0e7a`(session 단계만 4스레드, 태그 `w14-diag-session-only`, #89 쪽은 `w14-fix-session-only` = `4439fe0`, push 안 함)과 최종
`5c652a0`(두 단계, `01f17fb`를 diag에 cherry-pick). 모두 차례로(동시에 다른 내 시험 없음), `--keep` 없이.

| 커밋 | 명령 | 결과 | 직접 최고 total / app / owner | session 최고 total / app / owner | `EMAXCONN` / db 가득 | 로그 |
| --- | --- | --- | --- | --- | --- | --- |
| `13b0e7a` | `--supabase --session-pooler --oidc` ×3 | **PASS** ×3, 각 90/90 결과 줄, 634 통과 | 92 / 62 / 20, 94 / 64 / 21, 93 / 66 / 20 | 29 / 19 / 5, 29 / 19 / 5, 28 / 18 / 5 | 0 / 0 | `fix-13b0e7a-sp-run{1,2,3}*` |
| `13b0e7a` | `--oidc` | **PASS** 45/45, 306 통과 | — | — | — | `fix-13b0e7a-oidc.log` |
| `13b0e7a` | `--supabase --oidc` | **PASS** 45/45, 317 통과 | 89 / 63 / 18 | — | 0 / 0 | `fix-13b0e7a-supabase-oidc*` |
| `13b0e7a` | `cargo xtask check --skip-licenses` | **PASS** | | | | `check-13b0e7a.log` |
| **`5c652a0`** | `--supabase --session-pooler --oidc` ×3 연속 | **PASS** ×3, 각 90/90 결과 줄(직접 45 + session 45), 634 통과(317 + 317), 실패 0 | 25 / 14 / 5, 25 / 14 / 5, 25 / 12 / 5 | 29 / 19 / 5, 28 / 18 / 5, 29 / 19 / 5 | 0 / 0 | `fix-5c652a0-sp-run{1,2,3}*` |
| **`5c652a0`** | `--oidc` | **PASS** 45/45, 306 통과. NOT RUN: `supabase_*`(명령 선택) | — | — | — | `fix-5c652a0-oidc.log` |
| **`5c652a0`** | `--supabase --oidc` | **PASS** 45/45, 317 통과. NOT RUN: session pooler 단계(명령 선택) | 25 / 12 / 5 | — | 0 / 0 | `fix-5c652a0-supabase-oidc*` |
| **`5c652a0`** | `cargo xtask check --skip-licenses` | **PASS**(vitest 79파일·394) | | | | `check-5c652a0.log` |
| **`77eea03`**(diag2 + 수정, #64 `2acebe9`) | `--supabase --session-pooler --oidc` ×3 연속 | **PASS** ×3, 각 90/90 결과 줄, 634 통과, 실패 0 | 25 / 14 / 5 ×3 | 28 / 18 / 6, 27 / 17 / 5, 27 / 17 / 5 | 0 / 0 | `fix-77eea03-sp-run{1,2,3}*` |
| **`77eea03`** | `--oidc` | **PASS** 45/45, 306 통과. NOT RUN: `supabase_*`(명령 선택) | — | — | — | `fix-77eea03-oidc.log` |
| **`77eea03`** | `--supabase --oidc` | **PASS** 45/45, 317 통과. NOT RUN: session pooler 단계(명령 선택) | 25 / 13 / 5 | — | 0 / 0 | `fix-77eea03-supabase-oidc*` |
| **`77eea03`** | `cargo xtask check --skip-licenses` | **PASS** | | | | `check-77eea03.log` |
| `01f17fb`(#89+수정, worktree `w14-base`) | `--supabase --session-pooler --oidc` | **PASS** 90/90, 586 통과(기준선 `2518724`와 같은 수), 실패 0 | 22 / 12 / 4 | 27 / 17 / 4 | 0 / 0 | `pr89fix-01f17fb-sp-run1*` |

`--oidc`(시험 Compose의 PostgreSQL 18, `max_connections` 기본 100)는 측정하지 않았다(바꾸지 않음, 아래 5절).

## 5. 남은 것·NOT RUN

- `cargo xtask test --oidc`(Supabase 아닌 시험 PostgreSQL)는 이 수정 밖이라 스레드를 묶지 않았고 연결 수를 재지 않았다. 같은 collab 수요(app
  약 62 + owner 약 20)가 있지만 Supabase 자체 서비스 몫(약 10)이 없고 관리자가 superuser다. 이번 2회 PASS. 시험이 더 늘면 같은 절벽에 닿을 수
  있다(그때는 같은 원리로 묶거나 collab owner 풀을 줄이는 방향).
- `cargo xtask licenses` NOT RUN(의존성 변경 없음).
- push 뒤 #89: 원격 CI(run 39733367946) check·integration·licenses·oidc·rhwp·supabase 모두 pass. CodeRabbit이 `2518724..01f17fb`를 검토해 "No actionable comments", nitpick·outside diff 없음(남은 지적 없음).

## 6. 산출물

- 커밋: #89 브랜치용 `01f17fb`(부모 `2518724`), diag용 `5c652a0`(부모 `891f40e`). push: `git push origin w14-fix:fix/test-meili-memory` → `2518724..01f17fb`(fast-forward, push 전 `git fetch` 후 원격이 조상인지 확인). #89 제목·본문 갱신(`pr89-body.md`, `gh pr edit`가 classic Projects GraphQL 오류로 실패해 REST `PATCH /pulls/89`로). diag2 + 수정 = `77eea03`(로컬). worktree `w14-base`(브랜치 `fvoci/w14-base`)는 `01f17fb`에 남겨 둠
- 스크립트: `monitor.sh`, `run.sh`, `peaks.sh`. 로그: `logs/`(작업 트리 `<worktree>`, 홈 `<home>`으로 바꿈, 비밀값 없음).
