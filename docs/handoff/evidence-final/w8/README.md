# W8: 시험 Meilisearch OOM (R8 차단) — 원인과 수정

- 브랜치 `fix/test-meili-memory`, 시작 main `a5cda4e`. 호스트: WSL2, cgroup v2, CPU 32, 메모리 73 GiB, Docker 29.9.2.
- 수정: `deploy/compose.test.yaml` meilisearch `deploy.resources.limits.cpus: '2'`(메모리 한도 512m·이미지·tmpfs는 그대로), `docs/development.md` 6절 한 줄.
- 감시: `monitor.sh`가 1초마다 시험 Meilisearch cgroup의 `memory.current`·`memory.peak`(첫 기준 실행에는 없음)·`memory.stat`(anon·file·shmem)·`/meili_data` 크기·`memory.events`를, `docker events`로 oom·die를 기록(`logs/*.mem.log`, `*.events.log`). 실행은 `run.sh`, 요약은 `summ.sh`.

## 1. 원인

| 가설(지시서·W5) | 판정 | 근거 |
| --- | --- | --- |
| 색인 메모리 기본값이 호스트 RAM의 2/3 | **아님** | `meilisearch --help`: 한도 없이 48.86 GiB, `-m 512m`에서는 341.33 MiB → cgroup 한도를 따른다(`probes.txt`) |
| tmpfs 데이터가 컨테이너 메모리로 셈 | **아님(이 시험에서는 작음)** | 죽기 직전 shmem 0.7 MB·file 1.9 MB·`/meili_data` 1 MB, 늘어난 것은 anon(132→415 MiB 이상, `before-a5cda4e-test-oidc.mem.log` 끝) |
| 스레드·작업자가 호스트 코어 수를 따름 | **맞음** | 컨테이너 안 `nproc`=32, actix "starting 32 workers", 색인 스레드 기본 = 코어의 절반. 쉬는 상태 anon 132 MB(32코어) 대 34 MB(4코어). search.rs 흉내 부하(`probe2.sh`, 스왑 없음): 기본값 OOM, 4코어 337 MB, 2코어 195 MB |

공식 색인 설정만으로는 부족했다: `MEILI_MAX_INDEXING_THREADS=2`+`MEILI_MAX_INDEXING_MEMORY=128MiB`로 실제 시험을 돌리면 통과는 했지만
anon 492 MB·`memory.peak` 512 MB·`memory.events max` 3934(한도에 수천 번 닿음, 스왑 덕에 생존. compose가 `memswap_limit`을 두지 않음).
HTTP 작업자 수에는 공식 설정이 없고, Rust `available_parallelism`이 cgroup CPU 할당량을 따르므로(`--cpus 4` → "starting 4 workers")
CPU 한도가 원인 전체(작업자·색인 스레드·검색 동시 수)를 한 번에 묶는다. 4(CI 러너 코어 수)는 실제 시험에서 최고 458~467 MB로 여유가 적고
색인 메모리 128MiB를 더해도 459 MB로 같아서 2로 정했다(최고 344~360 MB, search.rs 시간 12.37 s 대 4코어 12.38 s).

CI 영향: `oidc`·Supabase job은 `compose.test.yaml`로 Meilisearch를 띄우므로 4코어 러너에서 2코어로 묶인다. 기본 시험 job은 `ci.yml`의 job
`services`(메모리 한도 없음)라 바뀌지 않는다. 개발 compose(`compose.dev.yaml`)는 메모리 한도가 없어 OOM 위험이 없으므로 바꾸지 않았다.

## 2. 실행 결과 (모두 `cargo xtask`, `--keep` 없음, 같은 체크아웃)

| 로그(`logs/`) | 구성 | 명령 | 결과 | 최고 메모리 |
| --- | --- | --- | --- | --- |
| `before-a5cda4e-test-oidc` | main `a5cda4e` 그대로 | `test --oidc` | **FAIL** exit 1. search.rs 6건 실패(30/45 바이너리에서 멈춤, `search indexing made no progress`), `oom` → `die exitCode=137` | anon 최고 415 MB(1초 표본), events max 225 |
| `after-envonly-t2m128-run1-test-oidc` | 색인 스레드 2·메모리 128MiB만(채택 안 함) | `test --oidc` | PASS 282 | peak 512, anon 492, events max 3934 |
| `after-cpus4-run1/run2-test-oidc` | CPU 4(채택 안 함) | `test --oidc` ×2 | PASS 282 ×2 | peak 467 / 458, events max 0 |
| `after-cpus4-m128-run1-test-oidc` | CPU 4 + 색인 메모리 128MiB(채택 안 함) | `test --oidc` | PASS 282 | peak 459, events max 0 |
| `final-cpus2-run1-test-oidc` | **최종(CPU 2)** | `test --oidc` | **PASS** 45 바이너리·282 통과·0 실패 | peak 344, anon 315, events max 0 |
| `final-cpus2-run2-test-oidc` | **최종** | `test --oidc` | **PASS** 282·0 실패 | peak 347, anon 317, events max 0 |
| `final-cpus2-supabase-oidc` | **최종** | `test --supabase --oidc` | **PASS** 293·0 실패 | peak 360, anon 329, events max 0 |
| `check-skip-licenses` | 최종 | `check --skip-licenses` | **PASS** fmt·clippy·단위·cargo deny(advisories·bans·sources)·openapi·prettier·eslint·svelte-check·i18n·rhwp 고지·notices·vitest | — |

최종 세 번 모두 Meilisearch는 compose down의 정지(143)로만 끝났고 oom 사건은 없다. `final-cpus2-run1`은 주석을 다듬기 전에 돌렸다(설정 값은 같음).

`cargo xtask licenses`: **PASS** exit 0(`logs/licenses.log`, cargo-deny 0.20.2를 coordinator가 설치한 뒤).

NOT RUN: `test --supabase --session-pooler --oidc`(지시 범위 밖).

## 3. PR

- 커밋 `7d9529c`, Draft PR https://github.com/fvoci/GroundZero/pull/89 (base main `a5cda4e`).
- 리뷰 수정 커밋 `2518724`(docs: `cargo xtask test`는 CPU 2개 이상 필요). PR 머리 `2518724`에서 CI 7개(check·integration·licenses·oidc·rhwp·supabase·CodeRabbit) 모두 pass. `7d9529c`에서도 모두 pass.

## 4. CodeRabbit

| 지적 | 판단 | 근거·SHA |
| --- | --- | --- |
| `compose.test.yaml:48` (Minor): CPU가 2개 미만인 호스트에서 기동이 막히니 지원하거나 최소 요구사항을 문서화하라 | 유효. 문서화로 처리 | `2518724`. CodeRabbit이 해결로 응답(스레드 resolved) |
| `development.md:62` (Minor, `2518724` 리뷰): CPU 한도는 상한일 뿐이고 Docker가 기동을 거부하지 않는다 | **기각** | 32 CPU 호스트에서 `--cpus 64`와 Compose `cpus: '64'`가 모두 "range of CPUs is from 0.01 to 32.00"로 실패(`logs/docker-cpus-over-host.txt`). 답글로 근거를 남김 |

요약 본문의 Merge Risk는 Low(`2518724`까지 검토)이고, 남은 우려는 위 첫 행의 CPU 2개 미만 호스트뿐이다(문서화함).
