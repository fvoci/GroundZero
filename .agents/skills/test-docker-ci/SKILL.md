---
name: test-docker-ci
description: 통합 테스트, Docker Compose, Dockerfile, CI 워크플로, xtask의 test/dev/down 명령을 바꿀 때 읽는다.
---

# 테스트·Docker·CI 변경 지침

적용 대상: `deploy/**`, `.github/workflows/**`, `xtask/src/{compose,test,dev}.rs`, `crates/server/tests/**`.

## 실행 격리 (docs/development.md 6절)

- Compose 프로젝트명은 `xtask::compose::project_name`이 체크아웃 경로로 만든다. 고정 이름을 쓰지 않는다.
- 호스트 포트는 `127.0.0.1::<port>`로 동적 할당하고 `docker compose port`로 찾는다. 고정 포트를 추가하지 않는다.
  문서화된 예외는 개발 Keycloak 하나다: 사용자 식별(발급자 + subject)이 바뀌지 않게
  `127.0.0.1:${FVOCI_DEV_KEYCLOAK_PORT:-8180}`에 고정한다(`deploy/compose.dev.yaml`, docs/development.md 12.1).
- `cargo xtask down`은 자기 프로젝트의 컨테이너·네트워크·볼륨만 지운다. 전역 `prune`을 넣지 않는다.
- 테스트 자격 증명은 `deploy/compose.*.yaml`과 `deploy/keycloak/realm-test.json`의 명백한 테스트 값이다. 운영 값과 섞지 않는다.

## 서비스

- 기본(`deploy/compose.test.yaml`): PostgreSQL 18, Meilisearch, SeaweedFS(S3 호환 테스트 저장소, D-61·O-31, `s3_*` 테스트). Keycloak은 `oidc` 프로필로만 기동한다(`cargo xtask test --oidc`). 포맷·단위 검사에는 띄우지 않는다.
- `cargo xtask test --supabase`는 PostgreSQL 대신 셀프호스트 Supabase 공식 구성(db·rest·supavisor, `deploy/supabase/compose.test.yaml`)을 DB로 쓴다(D-88).
- 새 서비스 추가는 비용·라이선스에 영향을 주므로 먼저 보고한다.
- 이미지 태그는 고정한다. healthcheck 없는 서비스를 `--wait`에 넣지 않는다. Keycloak 이미지에는 curl이 없어 bash `/dev/tcp`로 검사한다.

## 테스트 규칙

- 서비스가 필요한 테스트는 환경 변수가 없을 때 실패해야 한다. 건너뛰어 통과시키지 않는다.
- 로컬과 CI는 xtask 진입점(`cargo xtask check`·`test`·`licenses`·`rhwp`)을 호출한다. 검사 로직은 xtask에 두고 워크플로에 별도
  검사 명령을 흩뿌리지 않는다(지금 예외: `rhwp` job의 고지 파일 `git diff --exit-code`). CI job 목록은 docs/development.md 10절.

## 이미지

- `deploy/docker/dev.Dockerfile`은 빌드·검사용, `app.Dockerfile`은 실행용. 섞지 않는다.
- UID/GID를 1000:1000으로 고정하지 않는다. 생성 파일 소유권을 검증한다.
- 라이선스 고지(NOTICE)는 라이선스 검사 통과 후 추가한다.

## 보고

실행 명령·대상 커밋·PASS/FAIL/NOT RUN·남은 제한을 적는다. 실행하지 않은 것은 NOT RUN이라고 쓴다.
