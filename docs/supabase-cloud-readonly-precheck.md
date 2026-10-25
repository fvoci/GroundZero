# Supabase Cloud 읽기 전용 연결 점검

전용 Supabase Cloud의 **Session pooler에 TLS로 연결하여 읽기 전용 `SELECT 1`과 숫자형 서버 버전을 조회**하는
Rust `xtask` 명령이다. 검토·CI 통과 후 main 병합과 읽기 전용 연결 시험 1회가 승인되어 수동 workflow를
활성화했다(PR #42). 2026-10-22 사용자 요청으로 기존 자격 증명·경로를 재사용한 서버 버전 확인을 추가했다(D-100).
코드 검증 기록과 실제 Actions 실행 결과를 아래에서 구분한다.
이 점검은 Cloud 제품 지원이나 쓰기·마이그레이션·복구 지원을 확정하지 않는다.

이 점검은 일반 [ci.yml](../.github/workflows/ci.yml)과 [로컬 Supabase 시험](../xtask/src/supabase.rs)과 별도다.
`cargo xtask test --supabase`는 로컬 컨테이너의 bootstrap·migrate·통합 시험용이므로
이 점검에 사용하지 않는다. 기준은 [결정 D-88·D-100](decisions/database.md)과
D-09·D-60의 기존 Rust·의존성·검사 규칙이다.

## 설정

사용자가 Repository secrets에 등록했다고 알린 다음 여섯 항목을 사용한다. 설정 목록 API가 Forbidden을 반환하여
등록 여부와 값은 독립적으로 확인하지 못했다. 기존 URI Secret은 조회·사용·삭제하지 않는다.

| 종류              | 이름                          | 값                                                                                                      |
| ----------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------- |
| Repository Secret | `SUPABASE_TEST_HOST`          | Dashboard Connect의 shared Session pooler 호스트, URI·포트 없이 입력                                    |
| Repository Secret | `SUPABASE_TEST_PORT`          | `5432`                                                                                                  |
| Repository Secret | `SUPABASE_TEST_DATABASE`      | `postgres`                                                                                              |
| Repository Secret | `SUPABASE_TEST_USER`          | Dashboard Connect의 사용자명                                                                            |
| Repository Secret | `SUPABASE_TEST_PASSWORD`      | URI 인코딩하지 않은 비밀번호 원문                                                                       |
| Repository Secret | `SUPABASE_TEST_SSL_ROOT_CERT` | 공식 Dashboard에서 받은 CA의 전체 PEM (`BEGIN CERTIFICATE`부터 `END CERTIFICATE`까지, 실제 줄바꿈 포함) |

프로젝트 해시, 별도 대상 변수, 반복 확인 입력은 없다. 등록된 설정이 승인된 전용 프로젝트를 가리킨다는
운영 전제를 사용한다. shared pooler 호스트 형식·5432·postgres DB만 허용하며 6543(transaction)과
direct endpoint는 거절한다. 연결 종류는 `session_pooler`다. ADMIN은 계정 용도이며 direct 연결을 뜻하지 않는다.
비밀번호는 SQLx 연결 옵션으로 직접 전달하고 URI를 만들지 않는다. 실제 대상 값을 공개 소스에 넣지 않는다.
workflow는 여섯 항목 모두 `secrets` 컨텍스트에서 읽고 GitHub의 Secret 마스킹을 적용받는다.
점검 명령 자체도 호스트·사용자명·프로젝트 식별자·비밀번호·인증서 원문을 출력하지 않는다.
CA 파일은 공개 저장소에 넣지 않는다. Secret 값이나 등록 목록을 조회하지 않으며 등록은 사용자 보고에 따른다.

## 실행과 안전 경계

[workflow](../.github/workflows/supabase-cloud-readonly.yml)는 `workflow_dispatch` 전용이고
`fvoci/GroundZero`의 `refs/heads/main`에서만 실행하도록 제한한다. 임의 checkout 입력이나 PR trigger는
없으며 checkout은 dispatch의 `github.sha`를 사용한다. token은 `contents: read`, checkout의
`persist-credentials`는 false다. 동시 실행을 직렬화한다. 새 Rust 환경의 빌드를 위해 작업 전체는 15분,
빌드 step은 12분으로 제한하고, 자격 증명을 사용하는 점검 step은 별도로 1분으로 제한한다.
점검 프로그램 자체의 연결 5초·전체 20초 제한은 그대로 유지한다.

승인된 활성화 변경은 고정 `false` 차단만 제거했으며 수동/main/저장소 제한은 유지한다. 검토와 일반 CI를
통과한 PR #42를 merge commit으로 main에 반영했다. 수동 실행은 세 번이었다. 앞의 두 번은 실패했고(아래 실패 진단), CA 명시(PR #47)
뒤의 세 번째는 성공했다(아래 실패 진단 끝).
불확실한 dispatch는 run 목록으로 먼저 확인하고, 실패 후 추가 연결 시험은 별도 승인 없이 반복하지 않는다.
PR 코드에 Secret을 주거나 별도 경로로 Secret을 꺼내 실행하지 않는다. 비밀번호의 실제 사용은
GitHub Actions에서 Supabase DB 인증에만 한정한다.
[GitHub 수동 실행 안내](https://docs.github.com/actions/managing-workflow-runs/manually-running-a-workflow)에
따라 workflow 파일도 기본 브랜치에 있어야 한다.

기존 PR CI의 비밀 없는 단위 시험을 거친 코드를 사용하며, workflow는 빌드를 마친 뒤 마지막 step의
실행 파일에만 Secret을 전달한다.
제품 서버·Compose·통합 시험을 호출하지 않는다. 아티팩트·캐시·DB dump·환경 디버그 출력은 없다.

[Rust 명령](../xtask/src/supabase_cloud_precheck.rs)은 `--connect` 없이는 환경을 읽지 않고
`NOT RUN`으로 종료한다. 연결할 때는 단일 `PgConnection`을 만들며 재시도나 pool을 사용하지 않는다.
연결 5초, 문장 5초, 전체 20초로 제한한다. read-only 세션의 `BEGIN READ ONLY` 안에서 고정된
`SELECT 1`, `SELECT pg_catalog.current_setting('server_version_num')::integer`를 수행하고 `ROLLBACK`한다.
후자는 내장 함수로 설정값 하나만 읽으며 역할·스키마·기능 카탈로그의 행이나 사용자 데이터는 조회하지 않는다.
CREATE/ALTER/DROP, bootstrap, migrate, seed, restore, 역할·비밀번호 변경도 실행하지 않는다.

CA PEM은 네트워크 연결 전에 기존 native-tls 파서로 검사한다. 누락·빈 값은 `missing_ssl_root_cert`,
잘못된 PEM/X.509 값은 `invalid_ssl_root_cert`로 종료하며 연결/TLS/쿼리는 모두 `NOT RUN`이다.
정확히 한 개의 CERTIFICATE 블록만 받으며 개인키·추가 블록이나 64KiB를 넘는 입력은 거절한다.
검증한 PEM을 SQLx의 `ssl_root_cert_from_pem`으로 해당 연결에 전달하고 `PgSslMode::VerifyFull`로
인증서 체인과 호스트 이름을 확인한다. native-tls의 OS 기본 CA 목록도 유지되지만 시스템·브라우저·제품 앱의
신뢰 저장소를 수정하지 않는다. 이 설정은 인증서 지문 고정이 아니며 실제 pooler 인증서 일치는 TLS에서 검증한다.
상속된 PostgreSQL·TLS 환경 설정에 의한 우회를 거부한다. 검증 실패 시 TLS 모드를 낮추거나 검증을
생략하지 않으며 CA 없이 재시도하지 않는다. 드라이버 오류와 서버 notice는 원문 출력하지 않고 고정 오류 코드만 출력한다.

결과 JSON은 연결/TLS 상태, 읽기 전용 질의·정리 상태(`query`), 버전 조회 상태(`server_version_query`),
변경 시험 `NOT RUN`을 구분한다. 두 질의·rollback·연결 종료가 모두 성공해야 질의 상태를 PASS로 표시하며,
그때만 `server_version_num`과 `postgres_major`를 JSON 숫자로 출력한다. `version()`의 전체 문자열이나 호스트·사용자
정보를 출력하지 않는다. 예를 들어 `170006`은 메이저 17이며, 버전 조회 자체는 미지원 메이저도 관측값으로 보고한다.
연결 실패 시 연결/TLS는 미확인 상태를 유지하며 아래의 고정 오류 분류만 출력한다.
연결 후 쿼리가 실패하면 연결/TLS 성공과 쿼리 실패를 분리한다. `SELECT 1` 성공은 애플리케이션
권한·마이그레이션·쓰기·복구 지원 검증을 뜻하지 않는다.

## 실패 진단

[첫 Cloud 실행](https://github.com/fvoci/GroundZero/actions/runs/39587386404)은 빌드 성공 후
`connection_tls_or_query_failed`로 실패했다. 실행 SHA `79cd26b`의 상태 변경 순서와
`connection=UNCONFIRMED`, `tls_certificate_and_hostname=UNCONFIRMED`, `query=NOT RUN`을 대조하면
설정 형식 검증은 통과했고 `PgConnection::connect_with`에서 실패했다. 시간 초과 코드도 아니다.
`BEGIN`·`SELECT 1`·`ROLLBACK` 이전이며 변경 시험은 `NOT RUN`이다. 당시 코드는 세부 오류를 버려
DNS·TCP·TLS·인증·서버 startup 거절 중 원인을 더 좁힐 수 없다.

후속 변경은 [SQLx 0.8.6 오류 타입](https://docs.rs/sqlx-core/0.8.6/sqlx_core/error/enum.Error.html)과
[PostgreSQL SQLSTATE](https://www.postgresql.org/docs/17/errcodes-appendix.html)로 다음 고정 코드만
선택한다. 오류 메시지나 오류 체인을 문자열로 해석하지 않고, SQLSTATE는 class 28 판별에만 사용한다.
오류 원문과 SQLSTATE 값은 출력하지 않는다.

| 코드                        | 확인할 수 있는 범위                                                         |
| --------------------------- | --------------------------------------------------------------------------- |
| `connection_network_io`     | 연결 중 I/O 오류. DNS·TCP·TLS 교환 중 I/O를 따로 확정하지 않음              |
| `connection_tls`            | TLS 연결 수립 오류. 인증서·호스트 불일치 등 세부 원인은 미확정              |
| `connection_authentication` | 서버가 SQLSTATE class 28 인증·인가 거절을 반환. 비밀번호 오류로 단정 안 함  |
| `connection_database`       | 연결 중 그 밖의 서버 오류 응답. 구체적인 startup 거절 이유는 미확정         |
| `connection_protocol`       | 연결 중 드라이버 프로토콜 오류. TLS·인증·startup 단계의 세부 원인은 미확정  |
| `connection_unclassified`   | 위 분류에 해당하지 않는 연결 오류                                           |
| `query_failed`              | 연결 성공 후 `BEGIN READ ONLY`·`SELECT 1`·서버 버전 조회·`ROLLBACK` 중 실패 |

연결이 완전히 수립된 경우에만 연결/TLS를 `PASS`로 표시한다. `timeout`, `client_error`,
`unexpected_query_result`와 설정 검증 코드는 그대로 유지한다. 단위 시험은 가짜 드라이버 오류로
분류와 원문 미출력을 검증한다. [오류 분류 반영 후 실행](https://github.com/fvoci/GroundZero/actions/runs/39592575350)은
SHA `30266525b1ee8b9915a71299e779f6afe4f85029`에서 빌드 성공 후 `connection_tls`로 실패했다.
연결/TLS는 `UNCONFIRMED`, 쿼리/변경은 `NOT RUN`이며 이 결과만으로 CA 누락을 확정하지 않는다.
CA 명시 변경은 PR #47로 main에 반영되었다. 그 뒤의 [세 번째 실행](https://github.com/fvoci/GroundZero/actions/runs/39600271650)
(SHA `f9e5fd0`, 2026-10-21 12:31 UTC)은 성공했다. 결과는 연결 `PASS`(session pooler), TLS 인증서·호스트 `PASS`,
읽기 전용 질의 `PASS`, 변경 시험 `NOT RUN`이다. 이것은 Cloud 프로젝트에 TLS 검증으로 붙어 `SELECT 1`을 읽기 전용으로
실행할 수 있다는 확인일 뿐이다. 이 과거 실행은 서버 버전을 조회하지 않았다. 제품 서버 실행, 전체 마이그레이션,
앱 역할의 RLS·권한도 확인하지 않았다. 2026-10-22에 요청된 버전 확인은 아래 별도 기록으로 구분한다. 기존 dispatch API의 `Forbidden`은
재시도하거나 다른 인증·브라우저 경로로 우회하지 않는다.

## 2026-10-22 서버 버전 확인

사용자가 최소 PostgreSQL 버전 17과 검증한 메이저만의 지원 목록을 결정하고, 기존 Cloud의 실제 서버 버전 조회를
요청했다. 변경된 명령은 기존 여섯 Secret, TLS VerifyFull, main 전용 수동 workflow를 유지한다. 새 자격 증명을
설정하거나 원격 DB·역할·비밀번호·버전을 변경하지 않는다. 추가 조회는 `server_version_num` 하나다.

현재 원격 버전 조회 결과는 **NOT RUN**이다. 실행 후 run URL·소스 SHA·`server_version_num`·메이저·연결/TLS/질의 상태를
여기에 기록한다. 버전이 17 또는 18이어도 Cloud 설치·앱 RLS·복구 검증을 완료했다는 의미가 아니다. 실제 실행은
이 변경이 검토를 거쳐 main에 반영된 뒤 기존 workflow를 수동 실행하는 경로를 따른다. main 전용 게이트를 완화하거나
PR 코드를 DB 자격 증명과 함께 실행하지 않는다. 같은 요청을 여러 작업자가 중복 실행하지 않는다.
기존 성공 run `39600271650`의 job `119222705064` 로그를 2026-10-22에 재확인했으며, 당시 JSON에는
연결·TLS·질의·변경 상태만 있고 서버 버전은 없었다. 과거 로그로 현재 버전을 확정하지 않는다.

## TLS 공식 근거

- [Supabase 연결 방식](https://supabase.com/docs/guides/database/connecting-to-postgres):
  Session pooler는 5432, transaction pooler는 6543이다.
- [공식 PSQL 예제](https://supabase.com/docs/guides/database/psql): Session pooler에
  인증서와 호스트를 검증하는 `verify-full`을 사용한다. 구현은 기존 SQLx로 작성하며 psql은 필요하지 않다.
- [공식 SSL 설정](https://supabase.com/docs/guides/platform/ssl-enforcement): 별도 CA가 필요하면
  프로젝트 Dashboard → Database Settings → SSL Configuration → Download Certificate가 공식 출처다.
  사용자가 제공한 CA는 [공식 CLI 원본](https://github.com/supabase/cli/blob/b1c2e5c1beeb9968752b976ead984c3c32e407f1/apps/cli-go/internal/gen/types/templates/prod-ca-2021.crt)과 파일 SHA-256·DER 지문을 대조했다.
  사용자가 승인·등록한 CA를 점검 연결에만 명시한다. 이 출처 확인은 실제 Secret 값이나 pooler 인증서 일치를 검증한 결과가 아니다.
  임의 인증서나 서버가 보낸 인증서를 자동 신뢰하지 않으며 네트워크·SSL enforcement 설정을 바꾸지 않는다.

공식 문서 확인일: 2026-10-21. 사용자 Mac은 사용하지 않는다.

## 로컬 검증

```sh
cargo test --locked --package xtask supabase_cloud_precheck
cargo xtask supabase-cloud-precheck
cargo xtask check
```

단위 시험은 가짜 입력·시험용 CA 공개 인증서·가짜 연결 경계와 오프라인 인증서 파싱만 사용하며
DNS·DB 연결이나 실제 Secret을 사용하지 않는다.
기본 CLI는 NOT RUN과 종료 코드 2가 정상이다. 새 의존성 패키지·버전 없이 기존 SQLx·Tokio·tracing과
native-tls를 재사용한다. 인증서 파싱을 위해 기존 잠금 버전의 native-tls를 xtask 직접 의존성으로 선언한다.

아래 표는 PR #42 준비 당시의 검증 기록이다. 후속 변경의 검사 결과는 해당 PR에 기록한다.

| 검사                                                              | 결과                                                   |
| ----------------------------------------------------------------- | ------------------------------------------------------ |
| Rust 단위 시험·기본 CLI                                           | PASS: 핵심 6건, 기본·잘못된 인자는 NOT RUN/종료 코드 2 |
| `cargo xtask check`                                               | PASS: Rust 75건·웹 318건, 라이선스 포함                |
| actionlint·Prettier·`git diff --check`                            | PASS                                                   |
| 실제 Cloud 연결·TLS·SELECT 1                                      | NOT RUN                                                |
| Cloud workflow 수동 실행·DB 변경·마이그레이션·복구·전체 통합 시험 | NOT RUN                                                |

위 실제 실행 NOT RUN은 PR #42 준비 시점의 기록이다. 일반 CI의 로컬 Supabase 작업과
Cloud 연결 점검은 별개이며, 실행 후에는 해당 run의 연결/TLS/쿼리 JSON과 종료 상태를 함께 확인한다.
일반 CI나 조건 불일치로 건너뛴 job의 성공을 실제 Cloud 연결 성공으로 표시하지 않는다.
