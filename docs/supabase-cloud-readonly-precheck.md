# Supabase Cloud 연결 점검 준비

전용 Supabase Cloud의 **Session pooler에 TLS로 연결하여 읽기 전용 `SELECT 1`만 수행**하는
Rust `xtask` 명령이다. 현재 workflow는 고정 `false` 조건으로 비활성이다. 실제 연결·TLS·SQL 실행은
**NOT RUN**이며, 이 Draft PR은 Cloud 제품 지원이나 쓰기·마이그레이션·복구 지원을 확정하지 않는다.

기존 [ci.yml](../.github/workflows/ci.yml)과 [로컬 Supabase 시험](../xtask/src/supabase.rs)은
변경하지 않는다. `cargo xtask test --supabase`는 로컬 컨테이너의 bootstrap·migrate·통합 시험용이므로
이 점검에 사용하지 않는다. 기준은 [결정 D-88](decisions.md)과
[개발 문서](development.md)의 기존 Rust·의존성·검사 규칙이다.

## 설정

사용자가 등록했다고 알린 다음 다섯 항목을 사용한다. 설정 목록 API가 Forbidden을 반환하여
등록 여부와 값은 독립적으로 확인하지 못했다. 기존 URI Secret은 조회·사용·삭제하지 않는다.

| 종류                | 이름                     | 값                                                                   |
| ------------------- | ------------------------ | -------------------------------------------------------------------- |
| Repository Variable | `SUPABASE_TEST_HOST`     | Dashboard Connect의 shared Session pooler 호스트, URI·포트 없이 입력 |
| Repository Variable | `SUPABASE_TEST_PORT`     | `5432`                                                               |
| Repository Variable | `SUPABASE_TEST_DATABASE` | `postgres`                                                           |
| Repository Variable | `SUPABASE_TEST_USER`     | Dashboard Connect의 사용자명                                         |
| Repository Secret   | `SUPABASE_TEST_PASSWORD` | URI 인코딩하지 않은 비밀번호 원문                                    |

프로젝트 해시, 별도 대상 변수, 반복 확인 입력은 없다. 등록된 설정이 승인된 전용 프로젝트를 가리킨다는
운영 전제를 사용한다. shared pooler 호스트 형식·5432·postgres DB만 허용하며 6543(transaction)과
direct endpoint는 거절한다. 연결 종류는 `session_pooler`다. ADMIN은 계정 용도이며 direct 연결을 뜻하지 않는다.
비밀번호는 SQLx 연결 옵션으로 직접 전달하고 URI를 만들지 않는다. 실제 대상 값을 공개 소스에 넣지 않는다.
Variables는 비밀 값이 아니므로 GitHub 환경 표시에서 보일 수 있다. 점검 명령 자체는 호스트·사용자명·
프로젝트 식별자·비밀번호를 출력하지 않는다.

## 실행과 안전 경계

[workflow](../.github/workflows/supabase-cloud-readonly.yml)는 `workflow_dispatch` 전용이고
`fvoci/GroundZero`의 `refs/heads/main`에서만 실행하도록 제한한다. 임의 checkout 입력이나 PR trigger는
없으며 checkout은 dispatch의 `github.sha`를 사용한다. token은 `contents: read`, checkout의
`persist-credentials`는 false다. 동시 실행을 직렬화하고 작업 전체는 5분으로 제한한다.

고정 `false` 조건은 이번 PR에서 유지한다. **실제 실행을 위해 먼저 필요한 승인은 검토한 workflow의
활성화 변경을 main에 반영하는 것**이다. 비활성 PR을 그대로 병합하는 것만으로 실행되지는 않는다.
활성화·병합이 승인되고 완료된 뒤에만 main의 수동 workflow를 실행한다. 현재 PR 코드에 Secret을 주거나
별도 경로로 Secret을 꺼내 실행하지 않는다. 기존 저장소에는 이 Cloud 설정을 사용하는 실행 경로가 없다.
[GitHub 수동 실행 안내](https://docs.github.com/actions/managing-workflow-runs/manually-running-a-workflow)에
따라 workflow 파일도 기본 브랜치에 있어야 한다.

기존 PR CI의 비밀 없는 단위 시험을 거친 코드를 사용하며, workflow는 빌드를 마친 뒤 마지막 step의
실행 파일에만 Secret을 전달한다.
제품 서버·Compose·통합 시험을 호출하지 않는다. 아티팩트·캐시·DB dump·환경 디버그 출력은 없다.

[Rust 명령](../xtask/src/supabase_cloud_precheck.rs)은 `--connect` 없이는 환경을 읽지 않고
`NOT RUN`으로 종료한다. 연결할 때는 단일 `PgConnection`을 만들며 재시도나 pool을 사용하지 않는다.
연결 5초, 문장 5초, 전체 20초로 제한한다. read-only 세션의 `BEGIN READ ONLY` 안에서 고정된
`SELECT 1`을 수행하고 `ROLLBACK`한다. 역할·스키마·기능 카탈로그, 사용자 데이터는 조회하지 않는다.
CREATE/ALTER/DROP, bootstrap, migrate, seed, restore, 역할·비밀번호 변경도 실행하지 않는다.

SQLx의 `PgSslMode::VerifyFull`과 OS 인증서 저장소를 사용하여 인증서 체인과 호스트 이름을 확인한다.
상속된 PostgreSQL·TLS 환경 설정에 의한 우회를 거부한다. 검증 실패 시 TLS 모드를 낮추거나 검증을
생략하지 않는다. 드라이버 오류와 서버 notice는 원문 출력하지 않고 고정 오류 코드만 출력한다.

결과 JSON은 연결/TLS 상태, 읽기 전용 `SELECT 1` 상태, 변경 시험 `NOT RUN`을 구분한다.
연결 실패 시 인증·TLS·네트워크 중 어느 원인인지 원문으로 추측하지 않고 미확인 상태와 고정 오류를
출력한다. 연결 후 쿼리가 실패하면 연결/TLS 성공과 쿼리 실패를 분리한다. `SELECT 1` 성공은 애플리케이션
권한·마이그레이션·쓰기·복구 지원 검증을 뜻하지 않는다.

## TLS 공식 근거

- [Supabase 연결 방식](https://supabase.com/docs/guides/database/connecting-to-postgres):
  Session pooler는 5432, transaction pooler는 6543이다.
- [공식 PSQL 예제](https://supabase.com/docs/guides/database/psql): Session pooler에
  인증서와 호스트를 검증하는 `verify-full`을 사용한다. 구현은 기존 SQLx로 작성하며 psql은 필요하지 않다.
- [공식 SSL 설정](https://supabase.com/docs/guides/platform/ssl-enforcement): 별도 CA가 필요하면
  프로젝트 Dashboard → Database Settings → SSL Configuration → Download Certificate가 공식 출처다.
  현재 구현은 별도 CA가 필요한 경우 실패한다. 승인된 공식 CA 파일을 SQLx에 명시하는 변경은 별도 검토한다.
  임의 인증서나 서버가 보낸 인증서를 자동 신뢰하지 않으며 네트워크·SSL enforcement 설정을 바꾸지 않는다.

공식 문서 확인일: 2026-10-21. 사용자 Mac은 사용하지 않는다.

## 로컬 검증

```sh
cargo test --locked --package xtask supabase_cloud_precheck
cargo xtask supabase-cloud-precheck
cargo xtask check
```

단위 시험은 가짜 입력과 가짜 연결 경계만 사용하며 DNS·DB 연결이나 실제 Secret을 사용하지 않는다.
기본 CLI는 NOT RUN과 종료 코드 2가 정상이다. 새 의존성 패키지·버전 없이 기존 SQLx·Tokio·tracing과
native-tls를 재사용한다.

| 검사                                                              | 결과                                                   |
| ----------------------------------------------------------------- | ------------------------------------------------------ |
| Rust 단위 시험·기본 CLI                                           | PASS: 핵심 6건, 기본·잘못된 인자는 NOT RUN/종료 코드 2 |
| `cargo xtask check`                                               | PASS: Rust 75건·웹 318건, 라이선스 포함                |
| actionlint·Prettier·`git diff --check`                            | PASS                                                   |
| 실제 Cloud 연결·TLS·SELECT 1                                      | NOT RUN                                                |
| Cloud workflow 수동 실행·DB 변경·마이그레이션·복구·전체 통합 시험 | NOT RUN                                                |

일반 PR CI의 로컬 Supabase 작업과 이 비활성 Cloud workflow는 별개다. 일반 CI 성공을 실제 Cloud
연결 성공으로 표시하지 않는다. 고정 `false`로 건너뛴 job 역시 실제 연결 성공을 뜻하지 않는다.
