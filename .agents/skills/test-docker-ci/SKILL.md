---
name: test-docker-ci
description: fvoci 시험 도구(xtask)·Docker Compose·Dockerfile·CI 워크플로를 바꿀 때 무엇을 읽고 어떤 검사를 돌리는지
---

# 시험·Docker·CI 작업

규칙 본문은 결정 기록에 있다. 이 문서는 읽을 곳과 돌릴 검사만 가리킨다.

## 먼저 읽을 것

- 결정 기록: `docs/decisions/delivery.md`("현재 규칙 요약"). Compose 격리 D-10, CI 구조 D-64, 이미지 D-65·D-85·D-87, 검토 D-130. 수락 시험은 `product.md#d-128`, 백업·복구는 `operations.md`.
- xtask(`xtask/src/`)
  - `compose.rs`: 프로젝트 이름 `project_name`(체크아웃 경로 해시, 종류별 접미사), 같은 체크아웃 실행 잠금 `Compose::lock_run`(`target/<종류>/run.lock`), 중단 신호 `watch_interrupts`·`check_interrupt`, 강제 종료 뒤 복구 명령 `recovery_command`, 이 프로젝트만 지우는 `down`, 이 체크아웃 전체 `down_all`(`cargo xtask down`).
  - `test.rs`(`cargo xtask test`, `--keep`·`--oidc`·`--supabase`·`--external-services`), `browser.rs`(`cargo xtask browser`, 원시 출력 `target/browser/runs/`, 공개 증거 `target/browser/artifacts`), `dev.rs`, `supabase.rs`, `check.rs`, `ci_plan.rs`(PR 변경 경로별 job 선택, #146·#155).
- Compose: 시험 `deploy/compose.test.yaml`(PG17, 상위 메이저는 `compose.test.pg18.yaml` 덧씌움), 개발 `deploy/compose.dev.yaml`, Supabase `deploy/supabase/compose.test.yaml`, 셀프호스트 `deploy/compose.selfhost.yaml`(개인용 `--profile personal`, D-129). 시험 서비스는 호스트 포트를 `127.0.0.1::<포트>`로 Docker가 고르고 DB는 tmpfs다. 고정 포트 예외는 개발 Keycloak 하나다(`compose.dev.yaml`의 `FVOCI_DEV_KEYCLOAK_PORT`).
- 이미지: `deploy/docker/dev.Dockerfile`(CI 베이스·빌드), `deploy/docker/app.Dockerfile`(제품). release smoke의 실행 소유 자원(라벨 `fvoci.smoke=<실행>`, `SMOKE_RECOVER`)은 `.github/workflows/release-image.yml`의 `env.SMOKE` 머리 주석(#125).
- CI: `.github/workflows/ci.yml`(`changes` job이 `cargo xtask ci-plan`을 부르고 나머지 job이 그 결과로 돈다), `base-image.yml`, `release-image.yml`.
- 서버 통합 시험 배치: `crates/server/tests/it/main.rs` 머리 주석(공유 DB 전체 작업·프로세스 전역 상태를 쓰는 시험은 `tests/` 바로 아래 각자의 실행 파일로).
- 여러 워크트리에서 나란히 돌릴 때: `docs/parallel-worktrees.md`.

## 바꿀 때 지킬 것

- 정리는 그 실행이 만든 자원만 한다. 넓은 `prune`·전역 삭제를 넣지 않고, `--keep`과 생성 기록의 정리 계약을 유지한다(D-10, #93·#125·#130).
- 검사 로직은 xtask에 두고 워크플로는 xtask 진입점을 부른다. YAML에 긴 셸을 두지 않는다(D-64).
- 서비스가 필요한 시험은 환경이 없으면 실패한다. 건너뛰어 통과시키지 않는다(D-42).
- 이미지·서비스는 digest로 고정한다(D-85). 새 서비스·외부 액션·유료 러너는 사용자 확인 대상이다(`AGENTS.md`).

## 돌릴 검사

- `cargo test -p xtask`(잠금·신호·복구 명령·CI 경로 판단 단위 시험), `cargo xtask check`.
- 시험 도구를 바꿨으면 그 명령 자체: `cargo xtask test`, `cargo xtask test --oidc`, `cargo xtask browser`. Supabase 경로는 CI `supabase` job과 같은 `cargo xtask test --supabase --session-pooler --oidc`.
- CI 경로 판단을 바꿨으면 문서 전용·코드·섞임 세 경우의 PR 실행 기록을 남긴다(#146).
- 보고는 명령·대상 SHA·PASS/FAIL/NOT RUN·남은 제한(D-42).
