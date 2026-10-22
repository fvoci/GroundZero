# W9: #66 반복 회차 레인 잠금 시험의 간헐 실패

- 작업: task_a3904c4a1606 / dispatch ctx_e65b1ea8a968. 2026-10-22.
- 브랜치: `fix/audit-recurrence-lane-order`(PR #66). 시작 머리 `611e6644adbe`, 수정 커밋 `5142fb0`.
- 바뀐 파일: `crates/server/tests/recurrence.rs`만(시험). 제품 코드·마이그레이션(0089 포함)은 고치지 않았다.

## 1. 원인 판정: (a) 시험 결함, 둘

### 1.1 FV001 (W5 `test-oidc-ae68989-run2.log`에서 본 것)

- 시험 `a_lane_lock_failure_skips_only_that_item_in_the_recurrence_tick`은 상한 `app_system_recurrence_tick_ids()`를 읽은 뒤
  `app_system_recurrence_tick($1)`을 **딱 그만큼**의 id로 한 번 부른다(611e664 `recurrence.rs:840-849`).
- 상한은 DB 전체 반복의 `app_task_series_bound` 합이다(`0073_app_ids_tasks.sql:623-636`). 몫은 빠진 회차(최대 50) × (3 + 2 × 체크리스트 줄)이다
  (`0073:401-432`). 회차 맞추기는 반복마다 몫을 **다시 계산**해 id 목록을 차례로 잘라 쓴다(`0089:50-59`, `v_at += v_need`). READ COMMITTED라
  두 문장의 스냅숏이 다르다. 그사이 같은 바이너리의 다른 시험이 반복을 만들거나(따라잡기 시험의 새 반복은 몫 150개) 회차를 지우면,
  앞 반복의 몫이 커진다. 그러면 뒤 반복의 구간이 목록 끝을 넘어 `app_id`가 FV001을 낸다(`0070_app_ids_core.sql:28-29`).
- 회차 맞추기는 반복마다 예외를 잡아 경고하고 건너뛴다(`0089:53-57`). 0073:611-617도 같다. 문서 `docs/development.md` 85.1절(origin/docs/audit-record
  1721-1722줄)은 이를 설계로 적는다: "반복 회차·휴지통 정리처럼 함수 안에서 대상마다 예외를 잡는 작업은 그 대상만 경고하고 다음 주기에 다시 한다."
  그래서 함수는 정상으로 끝나고, 작업 프로세스의 FV001 재시도(`worker.rs:89-108`)는 이 작업에 걸리지 않는다. 시험은 그 재시도도 거치지 않는다.
- W5 로그에서 FV001이 난 반복 `01a1616b-625c-…`는 이 시험의 뒤 항목(LKB) 반복이다. LKB 반복 설정 요청 `01a1616b-6258-…` 바로 뒤에 만들어졌고,
  잠금이 실패한 항목은 `01a1616b-61a3-…`(LKA)다. 그래서 `synced == 0` → "the other item lost its occurrences"가 났다.
- **(b) 아님**: 설계대로 그 반복만 다음 주기(15분)로 밀린다. 예외 블록은 하위 트랜잭션이라 그 반복이 만든 행은 되돌아간다(중복 없음).
  다음 주기에 빠진 회차를 다시 세어 만든다(누락 없음, 최대 50개씩 따라잡음).
- **(c) 아님**: `_ids`와 회차 맞추기의 대상 조건이 같다. 건너뛴 항목도 `v_at`을 그 몫만큼 올린다(0089:59). 한 스냅숏 안에서는 합이 맞는다.

### 1.2 레인 순서 시험의 "the tick never waited on the done lane" (이번 반복 실행에서 새로 찾음)

- `#66`의 두 시험이 둘 다 DB 전체의 회차 맞추기를 돌리며 열을 쥔다.
  - 열 순서 시험: `run_all_now`, 잠금 시간 제한 없음.
  - 잠금 실패 시험: 다른 연결이 LKA 완료 열을 세션 잠금으로 쥔 채 `lock_timeout = 300ms`로 돌린다.
- 열 순서 시험의 작업이 먼저 앞 항목들의 열을 쥔 채 LKA에서 기다린다. 그러면 잠금 실패 시험의 작업은 그 항목마다 300ms 시간 초과를 겪는다.
  LKA 잠금은 그 작업이 끝나야 풀리므로, 두 작업이 서로를 기다리는 모양이 시간 초과로만 풀린다. 그동안 열 순서 시험은 자기 열(LKO)에서
  기다리는 작업을 10초 안에 보지 못하고 실패한다(LKA가 LKO보다 앞일 때. 이 순서 조건은 추정이다: 계측 중에는 실패를 잡지 못했고, 시간 증가만 측정했다).
- 증거(`logs/diag-summary.txt`, 수정 전 + 계측): 잠금 실패 시험의 회차 맞추기 시간이 반복 있는 항목 수에 비례했다.
  - 8개: 0.39s / 33개: 9.7s / 58개: 17.2s / 78개: 23.3s. 항목당 약 0.3초다.
  - 작업 순서가 반대인 실행(diag 8·10)은 0.5s였다.
- 반복 실행은 한 시험 스택(DB)을 이어 쓰므로 항목이 쌓여 이 경합이 커진다. 새 DB(정상 `cargo xtask test`)에서는 항목이 적어 10초를 넘기 어렵다.
  그러나 #54·#59 시험이 더해진 통합 바이너리에서도 같은 결합이 있다.

## 2. 수정 (`5142fb0`, 시험만)

- `SPARE_TICK_IDS = 10_000`: 잠금 실패 시험이 상한 + 10,000개의 id를 준다. 남는 id는 버린다(`db/mod.rs:105`).
  - 이 바이너리의 반복은 체크리스트 줄이 없어 반복당 한 번에 최대 150개를 쓴다. 상한 뒤에 수십 개의 반복이 몫을 늘려도 남는다.
  - 재시도 방식은 쓰지 않았다. "그 항목만 건너뜀"이 깨진 회귀(교착으로 다른 항목까지 건너뜀)도 같은 증상이라, 재시도하면 이 시험이 잡아야 할 것을 가린다.
- `LANE_TICK` 뮤텍스: 두 열 잠금 시험이 차례로 돈다. `tests/collab.rs`의 `GLOBAL_PURGE`(DB 전체 정리를 부르는 시험끼리 차례로)와 같은 방식이다.

## 3. 재현율과 시험 결과

반복 실행 도구: `xtask/src/test.rs`에 커밋하지 않은 훅을 넣었다(`temp-harness-uncommitted.patch`).
`W9_REPEAT_SCRIPT=…/repeat.sh cargo xtask test`로 시험 스택을 한 번 띄우고, `cargo test -p fvoci-server --locked --test recurrence`를 N번 돌린다.
스크립트는 `repeat.sh`다. 끝나면 xtask가 자기 스택만 내린다(`--keep` 없음). 커밋 전에 훅과 탐침은 되돌렸다.

| 단계 | 소스 | 명령 | 결과 | 로그 |
| --- | --- | --- | --- | --- |
| 수정 전 | `611e664` | 바이너리 40회 | **1/40 FAIL**: 1.2절 레인 순서 시험(`recurrence.rs:761`), FV001 0회 | `logs/repeat-before-611e664.out`, `logs/before-11.log` |
| 수정 전 탐침 | `611e664` + 임시 탐침 | `w9_probe_bound_growth_between_ids_and_tick`, 여유 0 | **FAIL**(결정적). `need=6`, `ticked=Ok(0)`, `synced=0`, FV001 경고 2건 | `logs/diag-probe.log` |
| 수정 전 계측 | `611e664` + 계측 | 바이너리 15회(16회째 중단) | 15 PASS. 회차 맞추기 시간 0.39s→23.3s 증가 | `logs/repeat-diag-611e664.out`, `logs/diag-summary.txt`, `logs/diag-16-interrupted.log` |
| 수정 후 탐침 | `5142fb0` + 임시 탐침 | 같은 탐침, 여유 10,000 | **PASS**. `ticked=Ok(51)`, `synced=1` | `logs/after-probe.log` |
| 수정 후 | `5142fb0` | 바이너리 40회 | **0/40 FAIL**. 바이너리 1.28s→2.77s(항목이 쌓여도 거의 그대로) | `logs/repeat-after-5142fb0.out` |

- 탐침의 동작: 앞 항목(PR0, 더 작은 id)에 119일 전 업무를 둔다. 상한을 읽은 뒤 HTTP로 반복을 건다(몫 150개). 그 뒤 회차 맞추기를 부른다.
  탐침 코드는 `temp-harness-uncommitted.patch`에 있다.
- W5 FV001 재현율은 통합 트리에서 6번 중 1번이었다. #66 단독 40회에서는 FV001이 나지 않았다(같은 바이너리 시험이 8개로 적음).
  그래서 탐침으로 결정적으로 재현했다.

전체 검사 결과는 아래 4절.

## 4. 전체 검사 (`5142fb0`)

| 명령 | 결과 | 로그 |
| --- | --- | --- |
| `cargo xtask check`(라이선스 포함, cargo-deny 0.20.2) | **PASS**: fmt, clippy, 단위 시험, deny, openapi, pnpm format·lint·check, i18n, rhwp 고지, licenses | `logs/check-5142fb0.log` |
| `cargo xtask test --oidc` | **FAIL**: `search.rs` 3건에서 멈춤(첫 실패 바이너리). 그 앞 바이너리는 모두 통과(`recurrence.rs` 8/8 포함) | `logs/test-oidc-5142fb0.log` |
| 같은 스택, `cargo test --workspace --locked --no-fail-fast -- --skip supabase_`(임시 훅, `nofailfast.sh`) | 45개 대상 중 **44개 PASS**. `search.rs` 6건만 FAIL(`oidc.rs` 5/5, `recurrence.rs` 8/8) | `logs/test-oidc-nofailfast-5142fb0.log` |

- `search.rs`의 실패는 모두 `search service connect`·`search service request`·`search indexing made no progress`다. 이 환경의
  시험 Meilisearch OOM(W8이 다룸, main도 같음)이다. 이번 변경과 상관없다.
- 로컬 NOT RUN: `--supabase`·`--session-pooler`. GitHub CI(run 39716240684, 머리 `5142fb0`)의 check·integration·licenses·oidc·rhwp·supabase 6개 job은 모두 PASS다.
- push: `611e664..5142fb0`(fast-forward, 원격이 조상인지 `git merge-base --is-ancestor`로 확인).

## 5. 남은 문제

- 열 순서 시험(`the_recurrence_tick_takes_lane_locks_in_the_same_order_as_board_moves`)은 작업 경로(`run_all_now`)를 그대로 쓴다.
  그래서 상한과 같은 수의 id만 준다. 뮤텍스로 잠금 실패 시험은 그 창에서 빠졌지만, 다른 6개 시험이 그 시험의 `_ids`와 회차 맞추기 사이에
  반복을 만들 수 있다(따라잡기 반복은 몫 150개). 그러면 같은 모양(FV001 → 건너뜀 → "lost its occurrence")이 날 수 있다. 창이 작고(`t1`에서
  기다리는 동안) 관찰된 실패가 없어 고치지 않았다. 통합 바이너리에서 다시 보이면 이 시험도 직접 SQL + 여유 id로 바꾸는 것이 방향이다.
- 제품 쪽 관찰(결함 아님, 설계): 회차 맞추기는 반복마다 FV001을 잡아 그 반복을 다음 주기로 미룬다. 그래서 `worker.rs`의 FV001
  재시도는 이 작업에는 걸리지 않는다. 몫이 밀리면 몫이 늘어난 반복만이 아니라 구간이 목록 끝을 넘는 뒤의 반복도 건너뛴다(탐침에서 두 반복 모두 FV001).

## 6. CodeRabbit

- `5142fb0` push 뒤 자동 리뷰(Run `0efad5d0-…`, `611e664..5142fb0`, 파일 `recurrence.rs` 1개): "No actionable comments were generated in the recent review."
  Actionable·Outside diff·Nitpick 없음. 03:00Z 뒤로 새 인라인 댓글과 새 리뷰 객체도 없다. 예전 인라인 스레드(0089:49)는 이미 `04b98aa`에서 답했다.
