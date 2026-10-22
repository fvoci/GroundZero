## 무엇

`deploy/compose.test.yaml`의 meilisearch에 `deploy.resources.limits.cpus: '2'`를 더한다(메모리 한도 512m·이미지·tmpfs는 그대로). `docs/development.md` 6절에 한 줄.

## 왜

코어가 많은 기계(WSL2, 32코어)에서 `cargo xtask test --oidc`가 매번 `search.rs`에서 실패했다. 시험 Meilisearch가 `oom` → exit 137로 죽고 진행 중이던 색인이 `search indexing made no progress`로 끝난다(main `a5cda4e`에서도 같음).

원인 확인(cgroup `memory.stat`·`memory.peak`·`docker events`, 단독 컨테이너 비교):

- 색인 메모리 기본값은 **원인이 아니다**: `-m 512m`에서 기본값이 341 MiB로 cgroup 한도를 따른다.
- tmpfs도 **원인이 아니다**: 죽기 직전 shmem 0.7 MB·`/meili_data` 1 MB. 늘어난 것은 anon(132 → 415 MiB 이상).
- **원인**: 컨테이너 안에서도 보이는 CPU가 32라 actix HTTP 작업자 32개, 색인 스레드 16개 등이 코어 수를 따라 커진다. 쉬는 상태 anon이 32코어 132 MB 대 4코어 34 MB다.

공식 색인 설정(`MEILI_MAX_INDEXING_THREADS=2`, `MEILI_MAX_INDEXING_MEMORY=128MiB`)만으로는 부족했다. 통과는 했지만 `memory.peak` 512 MB, `memory.events max` 3934로 한도에 붙어 스왑으로 버텼다. HTTP 작업자 수에는 공식 설정이 없다. Rust `available_parallelism`이 cgroup CPU 할당량을 따르므로(`--cpus 4` → "starting 4 workers") CPU 한도가 전체를 한 번에 묶는다. CPU 4(CI 러너 코어 수)로는 실제 시험 최고가 458~467 MB로 여유가 적었다(색인 메모리 128MiB를 더해도 459 MB). 그래서 2로 했다. search.rs 시간은 12.37 s로 4코어의 12.38 s와 같다.

## 영향

- CI: `oidc`·Supabase job은 이 compose로 Meilisearch를 띄우므로 4코어 러너에서 2코어로 묶인다. 기본 시험 job은 `ci.yml`의 job `services`(메모리 한도 없음)라 바뀌지 않는다.
- 개발 compose(`compose.dev.yaml`)에는 메모리 한도가 없어 OOM 위험이 없으므로 바꾸지 않았다.
- 코어가 2보다 적은 기계에서는 Docker가 이 서비스 기동을 거부한다(주석에 적음).

## 검증 (같은 체크아웃, `--keep` 없음)

| 구성 | 명령 | 결과 | Meilisearch 최고 메모리 |
| --- | --- | --- | --- |
| main `a5cda4e` | `cargo xtask test --oidc` | **FAIL**: search.rs 6건, `oom`/exit 137 | anon 415 MB 이상, 한도 도달 |
| 이 PR | `cargo xtask test --oidc` (1회) | **PASS**: 45 바이너리, 282 통과 | peak 344 MB, `memory.events max` 0 |
| 이 PR | `cargo xtask test --oidc` (2회) | **PASS**: 282 통과 | peak 347 MB, max 0 |
| 이 PR | `cargo xtask test --supabase --oidc` | **PASS**: 293 통과 | peak 360 MB, max 0 |
| 이 PR | `cargo xtask check --skip-licenses` | **PASS**: cargo deny 포함 | — |


`cargo xtask licenses`도 PASS(첫 커밋 때).

## 둘째 변경: Supabase 시험의 동시 실행 수를 4로 (`01f17fb`)

### 무엇

`cargo xtask test --supabase`의 두 단계(직접 연결·session pooler)에서 `RUST_TEST_THREADS=4`(`xtask/src/test.rs`). `deploy/supabase/test.env` 주석, `docs/development.md` 85.3절 한 항목. 시험 수·내용·운영 기본값은 그대로다.

### 왜

통합 리허설(W13)과 #64 머리 + 이 PR에서 `--supabase --session-pooler --oidc`가 간헐적으로 `collab.rs`의 HTTP 500 `database XX000`으로 실패했다. 시험 중 db의 `pg_stat_activity`를 0.25초마다 세고 Supavisor·db 로그를 받아 확인했다(32코어 WSL2).

- Supavisor session 모드는 클라이언트마다 서버 연결을 잡고, 풀(`POOLER_DEFAULT_POOL_SIZE` 64)을 넘는 클라이언트는 기다리지 않고 거절한다(`EMAXCONNSESSION`).
- 시험은 저마다 앱 상태(연결 풀 최대 16)를 만들고, libtest 기본값은 코어 수(32)만큼 동시에 돈다. #64가 collab 시험을 19개 → 41개로 늘려 32개가 동시에 돌고, 시험마다 앱 연결 약 2개를 쥔다.
- 결과: session 단계 앱 연결 64, db 연결은 superuser가 아닌 한도 97개(`max_connections` 100 − 3) 가운데 95~96개. 같은 순간 Supavisor `EMAXCONNSESSION`과 db `remaining connection slots are reserved` 로그 → 500. 직접 연결 단계도 89~95개까지 썼다(Supabase 자체 서비스가 약 10개를 쓴다).
- 제품 결함(누수)은 아니다: 서버 하나의 풀 상한 16은 그대로이고 #64 서버 변경에 풀 밖 연결이 없으며, 시험이 끝나면 연결 수가 Supabase 자체 몫으로 돌아온다.

4 = 64 / 16: 모든 시험의 풀이 꽉 차도 session 풀을 넘지 않는다. 코어 수·시험 수와 상관없다.

고르지 않은 것: 풀러 `pool_size`·db `max_connections` 올리기(고정 용량이라 코어가 더 많거나 시험이 늘면 다시 남, 85.4절에 20 → 64로 올린 같은 증상 기록), 서버 풀 크기를 시험용으로 줄이기(제품 설정 추가, 실제 수요는 서버당 약 2라 효과 없음), collab 시험의 직접 연결 풀 줄이기(Supavisor 64는 그대로).

### 영향

- CI 러너는 4코어라 원래 4스레드다. 바뀌지 않는다.
- 코어가 많은 로컬 기계에서 Supabase 시험이 느려진다: 같은 시험 묶음(634개)에서 3분 22초 → 4분 28초(약 +65초).
- Supabase가 아닌 `cargo xtask test`는 바꾸지 않았다.

### 검증 (`--keep` 없음, 차례로)

| 구성 | 명령 | 결과 | 최고 연결 수 직접 / session (db total, 한도 97) |
| --- | --- | --- | --- |
| main + 이 PR 첫 두 커밋 `2518724` | `--supabase --session-pooler --oidc` ×2 | PASS ×2 (586 통과) | 50 / 53~54 |
| #64 `2acebe9` + `2518724`, 이 변경 없음 | `--supabase --session-pooler --oidc` ×3 | **FAIL 2/3** (collab 등 session 2건씩, `XX000`). 통과한 1회도 db 한도 도달 | 90~95 / 95~96 |
| #64 `2acebe9` + `01f17fb` | `--supabase --session-pooler --oidc` ×3 연속 | **PASS ×3** (634 통과, `EMAXCONN`·db 가득 0) | 25 / 27~28 |
| #64 `2acebe9` + `01f17fb` | `--oidc` / `--supabase --oidc` | PASS (306) / PASS (317) | — / 25 |
| #64 `2acebe9` + `01f17fb` | `cargo xtask check --skip-licenses` | PASS | — |
| 이 PR `01f17fb` | `--supabase --session-pooler --oidc` | PASS (586 통과) | 22 / 27 |

(#64 이전 머리 `56e9108` + 이 변경에서도 같은 결과: session ×3 PASS, `--oidc`·`--supabase --oidc` PASS.)

NOT RUN: `cargo xtask licenses`(둘째 변경은 의존성 변경 없음).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
