# Supabase Cloud 시험 DB 재생성

근거는 [#245 마1 사용자 결정](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6551869203),
[준비 절차 4절](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6547954125), D-107이다.
구현 PR은 실행 승인이 아니다. Environment·승인자·Secret은 사용자가 등록하고 실제 실행도 사용자와 함께 한다.
기존 `supabase-cloud-readonly`는 그대로 읽기 전용이다. #113의 Cloud 상시 회귀 운영은 별도 범위다.

## 실행 전 사용자 준비

1. 전용 시험 프로젝트, 옛 `fvoci` 데이터의 소실 영향, 공유 의존, 기존 백업 생략 승인과 복구 한계를 확인한다.
   운영·개인 개발 DB는 대상이 아니다. Supabase 설정에서 `fvoci`가 Data API 노출 스키마에 없는지도 확인한다.
2. 조사 전용 계정을 사용자가 준비한다. LOGIN·DB CONNECT만 필요하며 superuser·BYPASSRLS·CREATEROLE·CREATEDB·REPLICATION,
   다른 역할 멤버십, 제품 스키마 USAGE·CREATE·데이터 권한이 없어야 한다. 공개 시스템 카탈로그만 읽는다.
   GitHub Environment **`supabase-cloud-rebuild`**를 만들고 required reviewers, 자기 승인 금지,
   main만 허용하는 배포 브랜치를 설정한다. 승인자가 실제로 적용되는지 확인한 **뒤에** 아래 표의 위치에 Secret을 등록한다.
   YAML의 `environment` 이름만으로 required reviewers가 설정되지는 않는다.
3. 직접 DB 연결의 DNS·네트워크·TLS 인증서 검증 가능 여부를 확인한다.
   이 경로는 `db.<project-ref>.supabase.co:5432/postgres`만 받는다.
   Session·transaction pooler는 모두 거부한다. Session 관리자·migrator 경유는 별도 입증이 필요하므로 이번 경로에 넣지 않았다.
   [Supabase 연결 안내](https://supabase.com/docs/guides/database/connecting-to-postgres)에 따라 직접 연결의 네트워크 조건을 확인하며,
   비용이 드는 옵션을 자동으로 추가하지 않는다.
4. 검토·병합된 main의 같은 SHA로 세 단계를 각각 수동 실행한다. 작업 중 다른 연결의 제품 DDL·역할·권한 변경을 멈춘다.
   main이 전진하면 새 SHA에서 조사와 승인을 다시 한다. 태그·배포는 이 절차에 포함하지 않는다.

## 필요한 Secret

모두 빌드가 끝난 뒤 실행 단계의 프로세스 환경에만 주입한다. 값·등록 목록을 PR 작성 중 조회하지 않는다.
`PORT=5432`, `DATABASE=postgres`는 코드에 고정되어 별도 Secret이 없다.

| 이름                                 | 등록 위치                               | 용도·필요 권한                                                                                              | 단계        | 기존 읽기 전용 6개와의 관계                                     |
| ------------------------------------ | --------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ----------- | --------------------------------------------------------------- |
| `SUPABASE_REBUILD_HOST`              | Repository와 보호 Environment에 같은 값 | 전용 시험 프로젝트 직접 DB 호스트                                                                           | 전부        | Session pooler용 `SUPABASE_TEST_HOST`는 재사용하지 않음         |
| `SUPABASE_REBUILD_SSL_ROOT_CERT`     | Repository와 보호 Environment에 같은 값 | 프로젝트 CA PEM 한 개, TLS VerifyFull                                                                       | 전부        | 기존 인증서와 같을 수 있으나 대상·유효성 확인 후 별도 등록      |
| `SUPABASE_INSPECT_USER`              | Repository                              | 위 최소 권한 카탈로그 읽기 계정                                                                             | 조사        | 기존 `SUPABASE_TEST_USER`의 권한을 가정하지 않음                |
| `SUPABASE_INSPECT_PASSWORD`          | Repository                              | 조사 계정 직접 연결 비밀번호                                                                                | 조사        | 기존 비밀번호와 별도 등록                                       |
| `SUPABASE_REBUILD_ADMIN_USER`        | 보호 Environment                        | PG17 이상, DB CREATE·CONNECT·CONNECT 부여 권한, CREATEROLE, 두 제품 역할 ADMIN OPTION·SET, 제품 소유자 권한 | 재설치      | 기존 6개에 해당 권한 없음. DB 소유권·세션 종료 권한 추가 불필요 |
| `SUPABASE_REBUILD_ADMIN_PASSWORD`    | 보호 Environment                        | 위 관리자 직접 연결 비밀번호                                                                                | 재설치      | 기존 비밀번호와 별도 등록                                       |
| `SUPABASE_REBUILD_MIGRATOR_PASSWORD` | 보호 Environment                        | bootstrap 설정·migrate·원장 검증·재생성 표식 기록. 제품 스키마 소유자                                       | 재설치·확인 | 기존 6개에 해당 항목 없음                                       |
| `SUPABASE_REBUILD_APP_PASSWORD`      | 보호 Environment                        | bootstrap 설정·실제 앱 로그인. NOSUPERUSER·NOBYPASSRLS                                                      | 재설치·확인 | 기존 6개에 해당 항목 없음                                       |

조사 job에는 Environment가 없고 관리자·migrator·app Secret도 주입하지 않는다.
재설치와 합성 쓰기 확인 job만 보호 Environment 승인을 거친다. 확인에는 관리자 Secret을 주입하지 않는다.
제품 SQLx 0.8은 CA를 파일 경로로 받으므로 CA만 실행 중 `RUNNER_TEMP`의 임의 0600 파일로 보관하고 종료 시 지운다.
체크아웃·캐시·아티팩트에는 쓰지 않는다. 자식 제품 프로세스에는 필요한 DB 환경만 전달하며 Cloud에서는 stdout·stderr를 폐기한다.
조사 JSON에는 검토에 필요한 `fvoci` 스키마 객체 이름·종류·허용 여부를 남긴다.
호스트·프로젝트 ref·사용자명·소유자·역할명·원시 DB 오류·SQL·연결값은 계속 가린다.
취소·강제 종료 때의 최종 정리는 일회성 GitHub 호스트 러너 폐기에 의존한다.

## 세 번의 수동 실행

워크플로는 `.github/workflows/supabase-cloud-rebuild.yml`이다. 입력 `phase`를 하나 고른다.
모든 단계는 같은 `concurrency` 그룹을 거치며 PR 이벤트에서는 실행되지 않는다. 조사는 승인 대기가 없다.
재설치·확인에는 `approved_fingerprint`와 `safety_confirmation`이 필수다. 사용자는 시험 전용 프로젝트이며
다른 앱·쓰기 주체가 없고 실행 중 다른 작업을 하지 않으며 승인 SHA·지문이 직전 조사와 같음을 확인한 뒤
다음 문자열을 정확히 입력한다: `TEST_ONLY_NO_OTHER_APPS_OR_WRITERS_NO_OTHER_WORK_MATCHED_SHA_AND_FINGERPRINT`.

| `phase`   | 동작·결과                                                                                                                                                                                                                                          | 다음 단계 조건                                                                                       |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `inspect` | 최소 권한 읽기 계정의 `BEGIN ... READ ONLY`에서 PG 버전, 현재 사용자·DB 권한, 역할·멤버십, 스키마·객체·소유자·ACL·의존·SQLx 원장 위치를 읽고 ROLLBACK. 객체 이름·종류·허용 여부와 `fingerprint`, `blockers`, `can_rebuild` 출력                    | 사용자가 명시 목록·소실 영향·SHA 확인. `can_rebuild=true`이며 차단 사유가 없어야 삭제 승인 가능      |
| `rebuild` | `approved_fingerprint`에 승인한 64자리 지문 입력. 연결 대상·SHA·전체 카탈로그·삭제 SQL 지문 대조. 테이블 잠금 뒤 같은 트랜잭션 전체 재조사·명시적 RESTRICT 객체 삭제 커밋. 별도 역할 재확인·삭제 커밋 뒤 `db bootstrap` → 성공 시에만 `db migrate` | 삭제 승인과 Environment 승인은 별개다. 지문 불일치나 공유 의존이면 새 조사부터 한다                  |
| `verify`  | 성공한 재생성이 스키마 COMMENT에 기록한 SHA·승인 지문을 현재 빌드 SHA·입력 지문과 먼저 대조. 10개 SQL의 SQLx SHA-384·버전·성공 10/10 대조. 실제 앱 로그인 합성 CRUD·거부·RLS·문맥 검사 뒤 ROLLBACK                                                 | 표식이 없거나 SHA·지문 불일치면 합성 쓰기 전에 실패. SQL이 같아도 main이 전진하면 재조사·재생성 필요 |

조사의 `status=PASS`는 조사 성공이다. `can_rebuild=false`이면 다음 단계는 금지다.
`xtask/src/supabase_cloud_rebuild/allowed-objects.json`은 같은 소스의 기준 SQL 열 개를 격리 DB에 설치해 얻고
검토하는 명시 허용 목록이다. SQL 체크섬과 목록을 함께 고정하며 정규 시험은 설치 결과와 정확히 대조한다.
Cloud에서 관측한 목록을 자동 승인하지 않는다. 목록 밖 객체는 소유자가 제품 역할이어도 차단한다.
옛 설치의 객체 이름이 목록에 없으면 검토된 목록 개정 없이 재설치할 수 없다. 누락 객체가 있는 옛 스키마는
부분 설치 복구를 위해 허용하되 관측된 모든 이름·종류·열·제약·정의·의존은 승인 지문에 들어간다.
지문은 열 정의를 포함한다. ANALYZE·autovacuum으로 바뀌는 통계·동결 진행 값은 제외하고,
늦게 정리되는 객체 존재 플래그는 실제 카탈로그 행에서 계산한다. 열 이름·타입·길이와 실제 인덱스 변화는 계속 감지한다.
enum 라벨, 주석, 인덱스·sequence·파티션 키 정의, 초기 ACL, 상속, 보안 레이블, 역할 속성·멤버십·설정도 포함한다.
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

현재 삭제 순서는 **테이블 잠금 → 같은 트랜잭션 전체 재조사 → RESTRICT DROP → 커밋**이다.
승인 지문의 카탈로그·차단 조건·이름·삭제문·역할 목록은 **단일 SELECT 문**의 결과로 구성한다.
READ COMMITTED의 여러 문이 서로 다른 시점을 조합하는 것을 막는다. REPEATABLE READ는 첫 조회인
advisory lock 획득 전부터 시점이 고정되어 테이블 잠금 대기까지 창에 넣으므로 이 경로에서는 선택하지 않았다.
[PG17 격리 규칙](https://www.postgresql.org/docs/17/transaction-iso.html)에 따라 단일 문도 스냅숏은 문 시작에 잡힌다.
따라서 남는 창은 재조사 문 끝 이후만이 아니라 **그 문 시작부터 해당 DROP까지**다.
역할의 속성·멤버십 지문과 소유 객체 0개도 별도 역할 트랜잭션의 단일 SELECT로 함께 조회한다.
함수·타입의 객체 잠금은 DROP 시점에 취득한다. 재조사 스냅숏 시점과 해당 DROP 사이에는
잠그지 못한 비테이블 객체가 바뀔 창이 남는다. 역할의 직전 재확인과 DROP 사이에도 클러스터 전역 변경 창이 남는다.
다른 연결 차단·강제 종료·DB 소유권 추가는 하지 않는다. 이 제한 때문에 전용 프로젝트·다른 쓰기 주체 및 작업 없음
체크리스트가 필수다. 동시 변경을 완전히 차단했다고 해석하지 않는다.

스키마 객체 삭제, 역할 삭제, bootstrap·migrate는 각각 별도 트랜잭션이다.
역할은 맨 마지막 별도 삭제 트랜잭션에서 승인 당시 속성·멤버십·설정·주석 지문과 소유 객체 0개를 재확인한다.
불일치나 DROP 실패 시 역할 삭제 트랜잭션을 롤백하고 실패한다. 역할만 남아도 성공이 아니며 새 조사부터 다시 시작한다.
뒤 단계 실패는 앞 단계의 커밋을 되돌리지 않는다. 원장 10/10 대조가 끝난 뒤에만 스키마 COMMENT에 SHA·지문 표식을 커밋한다.
JSON의 `stage`, `schema_deletion`, `roles_deletion`, `deletion`, `bootstrap`, `migrate`와 실행 SHA를 기록하고 멈춘다.
실패 뒤 자동 전체 삭제·재bootstrap, SQLx 이력·체크섬 수정, 광범위 DROP OWNED·CASCADE는 하지 않는다.
카탈로그 의존으로 추적되지 않는 외부 앱의 동적 SQL 사용은 사용자가 공유 관계 확인 때 점검해야 한다.
백업 복구·`session_replication_role`·실제 Cloud 네트워크·권한의 PASS를 이번 로컬 시험으로 대신하지 않는다.

로컬 단독 명령은 `cargo xtask supabase-cloud-rebuild-test`다. `deploy/compose.test.yaml`의 격리 PG17만 띄우고,
명백한 deploy 시험 자격 증명으로 실패 사례부터 검사한 뒤 non-superuser 관리자의 삭제·설치·확인을 실행하고 자기 Compose 자원만 정리한다.
이 시험은 정규 `cargo xtask test`의 기본·호스트·CI `--external-services` 경로에도 들어간다.
새 시험 클러스터가 아니면 실패한다. `--keep`으로 남은 클러스터는 보존 범위를 확인해 해당 체크아웃의 `cargo xtask down`으로 정리한 뒤 시험한다.
로컬 전체 병렬 시험은 기록용이고 병합 판정은 CI `integration` 결과로 한다(#234).
