# Supabase Cloud 시험 DB 재생성

근거는 [#245 마1 사용자 결정](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6551869203),
[준비 절차 4절](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6547954125), D-107이다.
구현 PR은 실행 승인이 아니다. Environment·승인자·Secret은 사용자가 등록하고 실제 실행도 사용자와 함께 한다.
기존 `supabase-cloud-readonly`는 그대로 읽기 전용이다. #113의 Cloud 상시 회귀 운영은 별도 범위다.

## 실행 전 사용자 준비

1. 전용 시험 프로젝트, 옛 `fvoci` 데이터의 소실 영향, 공유 의존, 기존 백업 생략 승인과 복구 한계를 확인한다.
   운영·개인 개발 DB는 대상이 아니다. Supabase 설정에서 `fvoci`가 Data API 노출 스키마에 없는지도 확인한다.
2. GitHub Environment **`supabase-cloud-rebuild`**를 만들고 required reviewers, 자기 승인 금지,
   main만 허용하는 배포 브랜치를 설정한다. 승인자가 실제로 적용되는지 확인한 **뒤에** 아래 Secret을 이 Environment에 등록한다.
   YAML의 `environment` 이름만으로 required reviewers가 설정되지는 않는다.
3. 직접 DB 연결의 DNS·네트워크·TLS 인증서 검증 가능 여부를 확인한다.
   이 경로는 `db.<project-ref>.supabase.co:5432/postgres`만 받는다.
   Session·transaction pooler는 모두 거부한다. Session 관리자·migrator 경유는 별도 입증이 필요하므로 이번 경로에 넣지 않았다.
   [Supabase 연결 안내](https://supabase.com/docs/guides/database/connecting-to-postgres)에 따라 직접 연결의 네트워크 조건을 확인하며,
   비용이 드는 옵션을 자동으로 추가하지 않는다.
4. 검토·병합된 main의 같은 SHA로 세 단계를 각각 수동 실행한다. 작업 중 다른 연결의 제품 DDL·역할·권한 변경을 멈춘다.
   main이 전진하면 새 SHA에서 조사와 승인을 다시 한다. 태그·배포는 이 절차에 포함하지 않는다.

## 필요한 Environment Secret

모두 빌드가 끝난 뒤 실행 단계의 프로세스 환경에만 주입한다. 값·등록 목록을 PR 작성 중 조회하지 않는다.
`PORT=5432`, `DATABASE=postgres`는 코드에 고정되어 별도 Secret이 없다.

| 이름                                 | 용도·필요 권한                                                                                                                                                         | 사용하는 단계    | 기존 읽기 전용 6개와의 관계                                                                                  |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------ |
| `SUPABASE_REBUILD_HOST`              | 전용 시험 프로젝트의 직접 DB 호스트                                                                                                                                    | 조사·재설치·확인 | 기존 `SUPABASE_TEST_HOST`는 Session pooler용이므로 재사용하지 않는다                                         |
| `SUPABASE_REBUILD_ADMIN_USER`        | 관리자 로그인. PG17 이상, DB CREATE·CONNECT, CREATEROLE, 기존 두 제품 역할의 ADMIN OPTION·SET, 제품 객체의 소유자 권한 필요. bootstrap에는 DB CONNECT 부여 권한도 필요 | 조사·재설치      | 기존 `SUPABASE_TEST_USER`의 관리자 권한을 가정하지 않는다                                                    |
| `SUPABASE_REBUILD_ADMIN_PASSWORD`    | 위 관리자의 직접 연결 비밀번호                                                                                                                                         | 조사·재설치      | 기존 비밀번호와 별도로 등록한다                                                                              |
| `SUPABASE_REBUILD_SSL_ROOT_CERT`     | 프로젝트 DB의 신뢰할 CA 인증서 PEM 한 개. TLS VerifyFull                                                                                                               | 조사·재설치·확인 | 기존 `SUPABASE_TEST_SSL_ROOT_CERT`와 같은 인증서일 수 있지만 사용자가 대상과 유효성을 확인해 별도로 등록한다 |
| `SUPABASE_REBUILD_MIGRATOR_PASSWORD` | bootstrap이 `fvoci_migrator`에 설정하고 migrate·원장 검증 로그인에 쓸 비밀번호. 제품 스키마 소유자                                                                     | 재설치·확인      | 기존 6개에 해당 항목 없음                                                                                    |
| `SUPABASE_REBUILD_APP_PASSWORD`      | bootstrap이 `fvoci_app`에 설정하고 실제 앱 역할 로그인에 쓸 비밀번호. NOSUPERUSER·NOBYPASSRLS                                                                          | 재설치·확인      | 기존 6개에 해당 항목 없음                                                                                    |

확인은 관리자 Secret을 주입하지 않으며, 조사는 migrator·app 비밀번호를 주입하지 않는다.
제품 SQLx 0.8은 CA를 파일 경로로 받으므로 CA만 실행 중 `RUNNER_TEMP`의 임의 0600 파일로 보관하고 종료 시 지운다.
체크아웃·캐시·아티팩트에는 쓰지 않는다. 자식 제품 프로세스에는 필요한 DB 환경만 전달하며 Cloud에서는 stdout·stderr를 폐기한다.
원시 DB 오류·SQL·식별자·연결값을 출력하지 않고 숫자·고정 상태 코드·지문만 JSON으로 남긴다.
취소·강제 종료 때의 최종 정리는 일회성 GitHub 호스트 러너 폐기에 의존한다.

## 세 번의 수동 실행

워크플로는 `.github/workflows/supabase-cloud-rebuild.yml`이다. 입력 `phase`를 하나 고른다.
모든 단계는 같은 Environment 승인과 `concurrency` 그룹을 거치며 PR 이벤트에서는 실행되지 않는다.

| `phase`   | 동작·결과                                                                                                                                                                                                                                                                     | 다음 단계 조건                                                                                               |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `inspect` | `BEGIN ... READ ONLY`에서 PG 버전, 현재 사용자·DB 권한, 역할·멤버십, 스키마·객체·소유자·ACL·의존·SQLx 원장 위치를 읽고 ROLLBACK. 가린 후보 배열과 `fingerprint`, `blockers`, `can_rebuild`를 출력                                                                             | 사용자가 결과·소실 영향·SHA를 확인한다. `can_rebuild=true`이고 모든 차단 사유가 없어야 삭제를 승인할 수 있다 |
| `rebuild` | `approved_fingerprint`에 승인한 64자리 지문을 입력. 연결 대상·SHA·원시 카탈로그·삭제 SQL을 포함한 지문을 다시 대조하고, 테이블을 잠근 뒤 한 번 더 대조. 정확히 같을 때만 명시적 RESTRICT 삭제를 한 트랜잭션으로 커밋. 이후 제품 `db bootstrap` → 성공한 경우에만 `db migrate` | 삭제 승인과 Environment 승인은 별개다. 지문이 다르거나 공유 의존이 있으면 새 조사부터 한다                   |
| `verify`  | 같은 빌드에 내장한 10개 SQL 파일의 SQLx SHA-384 체크섬·버전·성공 여부를 10/10 대조. 실제 `fvoci_app` 로그인으로 합성 사용자·조직·공간·문서의 CRUD·거부·공간 격리·연결 문맥을 검사하고 전체 합성 트랜잭션을 ROLLBACK                                                           | `checksums=10/10`, 앱 로그인·CRUD·RLS·거부·문맥 검사와 합성 정리 모두 PASS                                   |

조사의 `status=PASS`는 조사 성공이다. `can_rebuild=false`이면 다음 단계는 금지다.
공유 의존이 있으면 후보 전체를 `included=false`로 제외한다. 외부 스키마의 자동 삭제 파티션도 차단한다.
`pg_depend`의 자동·내부 의존은 RESTRICT여도 제거될 수 있으므로 이 구분을 별도로 검사한다.
[PostgreSQL 의존 카탈로그](https://www.postgresql.org/docs/17/catalog-pg-depend.html),
[역할 의존 카탈로그](https://www.postgresql.org/docs/17/catalog-pg-shdepend.html)를 사용한다.
알 수 없는 객체 종류, 제품 밖 소유·권한·멤버십·역할 의존, 제품 스키마 밖 SQLx 원장도 자동 정리하지 않고 차단한다.
스키마 밖 원장을 발견하면 위치·소유를 별도로 확인하고 구현 범위를 다시 정한다.

확인은 기존 Supabase 시험과 같은 계약으로 네 Data API 역할의 스키마·테이블·정의자 함수 유효 권한,
명시적 함수 권한과 제품 역할 멤버십이 없음을 검사한다. PUBLIC의 순수 호출자 도우미는 스키마 USAGE 부재로 접근이 막힌다.
Data API의 노출 스키마 설정 자체는 DB 권한 검사로 입증하지 않으므로 위 사용자 확인을 따로 남긴다.
합성 확인은 새 설치를 대상으로 하며 이미 초기 관리자가 설정된 DB에서는 쓰기를 롤백하고 실패한다.

## 실패·복구 한계와 로컬 시험

삭제 트랜잭션 성공 뒤 bootstrap·migrate는 각각 별도 트랜잭션이다. 뒤 단계 실패는 앞 단계의 커밋을 되돌리지 않는다.
JSON의 `stage`, `deletion`, `bootstrap`, `migrate`와 실행 SHA를 기록하고 멈춘다.
실패 뒤 자동 전체 삭제·재bootstrap, SQLx 이력·체크섬 수정, 광범위 DROP OWNED·CASCADE는 하지 않는다.
카탈로그 의존으로 추적되지 않는 외부 앱의 동적 SQL 사용은 사용자가 공유 관계 확인 때 점검해야 한다.
백업 복구·`session_replication_role`·실제 Cloud 네트워크·권한의 PASS를 이번 로컬 시험으로 대신하지 않는다.

로컬 단독 명령은 `cargo xtask supabase-cloud-rebuild-test`다. `deploy/compose.test.yaml`의 격리 PG17만 띄우고,
명백한 deploy 시험 자격 증명으로 실패 사례부터 검사한 뒤 non-superuser 관리자의 삭제·설치·확인을 실행하고 자기 Compose 자원만 정리한다.
이 시험은 정규 `cargo xtask test`의 기본·호스트·CI `--external-services` 경로에도 들어간다.
새 시험 클러스터가 아니면 실패한다. `--keep`으로 남은 클러스터는 보존 범위를 확인해 해당 체크아웃의 `cargo xtask down`으로 정리한 뒤 시험한다.
로컬 전체 병렬 시험은 기록용이고 병합 판정은 CI `integration` 결과로 한다(#234).
