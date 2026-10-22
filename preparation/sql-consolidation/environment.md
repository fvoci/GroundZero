# SQL 통합 준비: 설치·DB 역할·연결 환경

상태: 준비 문서. 제품 코드·SQL·설치 계약 변경 없음. 작성일 2026-10-22.

기준은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`이다. 아래 저장소 파일의 행 번호는 모두 이 SHA의
행 번호다. 작업 브랜치는 `fvoci/sql-prep-c`이며, #91은 기준에 포함되어 있다. #69의
`e4cae26dbf3104ba40d6ef39fda50b8d8b2035cb`는 미병합 참고 자료로만 읽었다. 그 문서의 95절
(`docs/development.md:3019–3028`)도 누적 마이그레이션의 성공을 통합본의 성공으로 대신하지 말라고 구분한다.
최종 #69 변경은 통합 작업자가 별도로 대조한다.

설치자는 연결 값과 배포 설정을 제공하고 기존 `db bootstrap` → `db migrate` → 서버 시작 흐름을 따른다.
RLS 정책·정의자 함수·역할 멤버십을 직접 편집하는 설치 절차는 만들지 않는다. 지금도 기본 Compose는 이 순서를
자동 실행하며 실패 시 앱을 시작하지 않는다(`deploy/compose.selfhost.yaml:66–90`,
`docs/self-hosting.md:33–39`). 직접 바이너리는 환경 변수를 명시적으로 전달해야 한다.

## 1. 환경별 설치 조건

| 환경                                 | 설치 전에 준비할 것                                                                          | 연결 사용자·DB 이름                                                                         | bootstrap·migrate                                                                                                                              | 앱 연결·확인 범위                                                                                                          |
| ------------------------------------ | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 기본 자체 PostgreSQL                 | 기본 Compose와 비밀 값. Compose가 PG18.3·`fvoci` DB를 만든다                                 | 관리자 `fvoci`, DB `fvoci`; `fvoci_migrator`·`fvoci_app`은 bootstrap 생성                   | 관리자는 초기 이미지의 관리자, migrate는 `fvoci_migrator`. 세 URL을 Compose가 조합                                                             | `fvoci_app` 직접 연결. 기존 지원 경로; 통합 후보의 새 설치는 B의 격리 시험에서 다시 확인                                   |
| 기존 자체 PostgreSQL에 바이너리 설치 | 미리 존재하는 대상 DB, 접근 가능한 관리자, TLS/네트워크. bootstrap은 DB 자체를 만들지 않는다 | 관리자 이름·DB 이름은 URL에서 지정 가능. 앱·migrator 역할과 `fvoci` 스키마 이름은 고정      | 새 역할을 만들 수 있는 관리자와 대상 DB의 스키마 생성·CONNECT 부여 권한. 기존 역할은 ADMIN OPTION 또는 superuser 필요. migrate는 스키마 소유자 | `DATABASE_URL`로 `fvoci_app` 직접 연결. 임의 DB 이름은 코드가 수용하지만 새 통합본으로 별도 설치 시험 필요                 |
| 셀프호스트 Supabase                  | D-88에서 고정한 공식 구성(PG17.6), DB 네트워크. GroundZero 전용 Supabase Compose는 아직 없음 | 관리자 `postgres`는 비-superuser; 기본 DB `postgres`. 역할은 동일                           | 현 지원 계약은 관리자·migrator 모두 직접 연결. 기존 역할의 ADMIN OPTION과 DB 권한을 확인                                                       | 앱 직접 연결 또는 Supavisor session. 공식 시험 테넌트는 `postgres` 한 DB만 라우팅. 앱 사용자 표기는 `fvoci_app.<tenant>`   |
| Supabase Cloud                       | 승인된 전용 시험 프로젝트·대상 연결·TLS CA·필요 역할 권한을 먼저 확인하는 후속 계획          | 기존 읽기 시험은 `postgres` DB와 Dashboard의 session 사용자. 앱·migrator 접속 성공은 미확인 | bootstrap·마이그레이션·역할/비밀번호 변경 모두 NOT RUN. D-88의 제품 지원 밖                                                                    | session 연결·TLS·읽기 전용 `SELECT 1`만 기존 성공. Cloud 앱·RLS·풀 재사용은 미확인. [후속 계획](environment/cloud-plan.md) |

근거: `deploy/compose.selfhost.yaml:20–45`, `docs/self-hosting-supabase.md:11–22,38–72`,
`crates/server/src/db/admin.rs:244–281,285–340`, `xtask/src/compose.rs:189–220`.
PG17/18이 현재 검증 대상이며, 더 낮은 버전 지원을 일반적인 PostgreSQL 지원 문구로 추론하지 않는다.

## 2. 역할과 기존 설정 계약

| 역할/값                                               | 사용하는 작업                    | 필요한 범위와 금지할 혼용                                                                                                                              |
| ----------------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `DATABASE_ADMIN_URL`                                  | `db bootstrap`, `backup restore` | bootstrap은 역할 생성·비밀번호 설정·대상 DB 준비용. restore는 별도로 `session_replication_role` 설정 권한이 필요. 앱 URL에 이 계정을 재사용하지 않는다 |
| `DATABASE_MIGRATION_URL`                              | `db migrate`, `backup create`    | 로그인 역할 `fvoci_migrator`, `fvoci` 스키마·객체 소유. RLS 우회 속성을 부여할 필요 없음. 소유자이므로 앱보다 강한 데이터 접근이 가능                  |
| `DATABASE_URL`                                        | API·worker 런타임                | 로그인 역할 `fvoci_app`, `NOSUPERUSER NOCREATEROLE NOCREATEDB NOBYPASSRLS`; 객체 비소유자. 고정 GRANT와 RLS·정의자 함수를 사용                         |
| `FVOCI_DB_MIGRATOR_PASSWORD`, `FVOCI_DB_APP_PASSWORD` | bootstrap                        | 두 제품 역할의 비밀번호를 만들거나 맞춤. URL의 해당 역할 비밀번호와 일치해야 함. 평문 출력 금지                                                        |
| `FVOCI_TEST_*`                                        | 격리 통합 시험                   | 테스트 하니스 입력. 운영 연결 설정의 별칭이 아님                                                                                                       |
| `SUPABASE_TEST_*` 여섯 Secret                         | 기존 Cloud 읽기 전용 workflow    | 해당 점검만의 입력. 제품의 세 URL 계약을 이 분할 입력으로 대체하거나 중복 확장하지 않음                                                                |

`Config`는 앱 URL을 받고 `DbCommandConfig`는 명령별 URL·비밀번호를 받는다
(`crates/server/src/config.rs:199,383–408`). URL은 `Secret`의 `Debug`에서 가려지고 SQL 문장 로그도
꺼진다(`config.rs:11–35`, `db/mod.rs:136–141`). 바이너리는 `.env`를 자동으로 읽지 않는다
(`docs/self-hosting.md:61–70`). URI 비밀번호는 URI 인코딩이 필요하며 기존 Compose 예시는 직접 문자열을
조합하므로 문서가 안내하는 hex 비밀번호를 유지한다. 새 host/user/database/password 변수 묶음은 필요 없다.

현재 bootstrap은 기존 역할의 속성과 관리자 권한을 **두 역할 모두 먼저 확인**하고, 역할·멤버십·스키마 작업을
한 트랜잭션으로 한다(`db/admin.rs:291–318`). 생성된 migrator의 `CREATEDB` 기본값은 false지만 기존 migrator의
`CREATEDB`는 허용한다(`admin.rs:295–306`). 일반 설치·마이그레이션에는 DB 생성 권한이 필요 없고,
추가 DB를 만드는 시험은 관리자에게 그 권한이 필요하다(`tests/common/mod.rs:300–325`). 이를 앱 권한으로
옮기거나 Cloud 설치 요구로 자동 승격하지 않는다.

비-superuser 관리자의 임시 migrator SET 멤버십은 기존 grantor·INHERIT·SET 상태를 기록한 뒤 돌려준다
(`admin.rs:119–237`). restore는 단일 관리자 연결에서 migrator로 전환하며 트리거 설정 때만 관리자 권한을
잠깐 사용한다(`backup.rs:257–282,308–316,479–490`). `session_replication_role`은 **복구 요구사항**이지
일반 앱 설치 요구사항이 아니다. 일반 PG 관리자는 별도 권한이 필요할 수 있고, 셀프호스트 Supabase의 허용을
Cloud에서도 같다고 가정하지 않는다.

## 3. 하드코딩과 설치 실패 검토

| 위치                                                   | 현재 사실                                                                             | 준비 결론                                                                                                                                                                  |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `db/admin.rs:22–27,245–251`                            | 역할·스키마 이름 고정; DB 이름은 `current_database()`에서 읽고 `%I`로 인용            | DB 이름 `postgres` 고정은 제품 bootstrap에 없음. 역할 이름 변경은 SQL GRANT·소유권·복구까지 영향을 주므로 이번 준비에서 설정화하지 않음                                    |
| `admin.rs:274–279`                                     | 두 역할의 `search_path=fvoci`를 역할 전체에 설정                                      | 클러스터에서 이름·비밀번호가 같은 역할을 공유한다. 같은 클러스터 여러 GroundZero 설치의 독립 자격 증명은 현재 계약에 없음. 기존 역할 재사용·비밀번호 갱신 영향을 사전 확인 |
| `admin.rs:333–340`, `db/mod.rs:67–77`                  | migrate는 받은 URL로 실행, 앱 ping은 `SELECT 1`. 실제 로그인 역할·소유권 검사는 없음  | 잘못된 관리자 URL이 앱 URL에 들어가도 연결 성공만으로 거부되지 않음. 아래 E1 우선 제안                                                                                     |
| `admin.rs:98–115`                                      | 기존 역할의 4개 속성을 검사; 상속/SET 멤버십은 이 검사에 포함되지 않음                | 다른 역할에서 객체 권한을 상속하거나 migrator로 SET 가능한 앱을 놓칠 수 있음. E2 제안                                                                                      |
| `deploy/compose.selfhost.yaml:21–23,35–45`             | 관리자·DB `fvoci`, 호스트 `postgres` 고정                                             | 기본 로컬 배포 토폴로지의 고정값. 외부 Supabase URL에 자동 재사용하지 않음                                                                                                 |
| `xtask/src/compose.rs:194–220`, `test.rs:203–238`      | 자체 시험 `/fvoci`, Supabase `/postgres`, external-services도 시험 자격 증명으로 조합 | 테스트 전용. Cloud에 `--external-services`를 연결하는 대안으로 쓰지 않음                                                                                                   |
| `tests/common/mod.rs:285–342`                          | 새 DB 이름을 만들고 URL path만 바꿈; session 단계에서는 직접 앱 URL 우선              | 운영 설치의 DB 생성 요구 아님. session 시험 성공 범위를 표기할 때 직접 연결 우회 구분                                                                                      |
| `xtask/src/supabase_cloud_precheck.rs:147–153,186–199` | session 5432와 DB `postgres` 허용 목록; statement cache 0                             | 승인된 읽기 점검의 범위 제한. 제품의 일반 DB 이름 검사를 이 제한으로 바꾸지 않음                                                                                           |

기준 SHA에는 설치 실패 안내가 이미 있다(`main.rs:80–105`, `docs/self-hosting.md:51–75`). 설정 오류는 어느 명령에
어떤 환경 변수 이름을 전달할지 알리고, SQL·checksum·dirty 오류는 DB·볼륨을 보존하도록 안내한다.
현재 `check_attributes` 원문은 “권한 속성을 제거하라”, `check_manageable` 원문은 GRANT 예시를 포함한다.
일반 설치자에게 내부 SQL 수정을 요구하는 안내로 확대하지 말고, 필요한 관리자 자격 증명 재선택 또는 운영자/유지보수
담당자에게 오류 코드·릴리스 전달로 연결해야 한다. 어떤 경우에도 적용 이력 삭제·정책 수정·자동 권한 확대를 제안하지 않는다.

## 4. 최소 후속 제안 — 이 문서에서는 구현하지 않음

| ID  | 변경 제안                                                                                                                                                                 | 기존 정책 안의 검증 기준                                                                                                                                                                                   |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E1  | migrate 시작과 앱의 새 물리 연결에서 실제 `session_user`·`current_user`, 대상 스키마/소유권, 필수 search_path 계약 확인. 잘못된 역할 URL은 이름이 명확한 설정 오류로 실패 | 앱은 `fvoci_app`, migrate는 `fvoci_migrator`; pooler URI의 `<role>.<tenant>` 문자열을 DB 역할 이름으로 비교하지 않음. 관리자 URL을 실수로 넣은 시험은 데이터 접근 전에 실패. 자동 SET ROLE로 교정하지 않음 |
| E2  | 기존 역할의 유효 멤버십·권한을 읽어 앱의 migrator/관리 역할 상속·SET 가능성 및 제품 객체 소유를 검사. bootstrap DDL 전에 거부                                             | 새 설치는 그대로 통과. 위험한 기존 구성은 아무 역할·비밀번호도 바꾸지 않고 이유와 담당자 조치를 표시. 플랫폼 역할을 자동 REVOKE하지 않음                                                                   |
| E3  | 기존 명령 오류 안내에 `CREATEROLE`, 기존 역할 ADMIN OPTION, DB 준비, 잘못된 URL 역할, schema owner 불일치의 단계를 명확히 연결                                            | 설치자가 RLS나 내부 함수 코드를 편집하지 않아도 설정 문제와 유지보수 문제를 구분. URL·비밀번호·인증서 원문 미출력. 기존 안내와 중복 명령을 만들지 않음                                                     |
| E4  | 통합 후보 검증에 DB 이름 `fvoci`/`postgres` 밖의 이름, 실제 앱 로그인 역할, 소유권·ACL·RLS·정의자 설정, 한 연결 강제 재사용을 추가                                        | B의 격리 검증과 협의. 새 테스트 DB만 사용하며 공유 DB reset 없음. [세션 검증 범위](environment/session-state.md)                                                                                           |

E1–E4는 D-08/D-88의 기존 분리를 실제로 확인하거나 실패 설명을 개선하는 기술 제안이다. 스키마 통합 승인이나
Cloud 제품 지원을 선행하지 않는다. E1–E3은 기존 구현의 보완 후보이며, 통합 후보 때문에 생긴 회귀가 입증되지
않은 상태에서 SQL 통합의 새 차단 조건으로 삼지 않는다. 후보 검증은 현재 지원하는 역할·환경 계약을 보존하는 데
초점을 둔다. 임의 역할 이름 지원, 독립 설치를 위한 역할 네임스페이스, Cloud 지원 확대,
transaction pooler 지원, 기존 데이터 삭제·이전, 플랫폼 권한 변경은 별도 범위/정책 판단이다.

## 5. 관리형 객체 보호와 증거 경계

통합 SQL은 GroundZero의 `fvoci` 객체와 두 제품 역할 계약에 한정한다. `auth`, `storage`, `realtime`,
`extensions`, `supabase_*`, `pg_*` 등 관리형 스키마·역할·확장·이벤트 트리거를 삭제·재소유·재생성하지 않는다.
`public`의 기존 객체나 다른 앱의 기본 권한을 일괄 수정하지 않는다. 필요 권한이 없거나 이름/소유권이 충돌하면
정확한 단계에서 멈춘다. `anon`·`authenticated`·`service_role`·`authenticator`에 제품 권한을 부여하지 않고,
`fvoci`를 Data API 노출 스키마에 넣지 않는다(D-88, `docs/decisions.md:184–190`).

기존 Supabase 시험은 위 네 역할의 유효 스키마/표/정의자 함수 접근 금지와 제품 역할 멤버십 부재를 확인한다
(`tests/supabase.rs:46–123`). 이는 기존 데이터/관리형 객체가 전혀 변경되지 않았다는 전체 카탈로그 비교를 대신하지
않는다. 통합 후보의 전후 비교에는 제품 객체의 소유권·ACL·정책을 포함하고, 관리형 객체는 읽기 전용 전후 증거로
보존 여부를 확인한다. `app_user_id()`가 문맥 없는 요청에서 NULL인 계약을 유지한다
(`migrations/0001_first_flow.sql:5–9`).

C 작업자가 확인한 것은 기준 소스, 기존 문서 기록, 공식 상위 소스, 기존 GitHub run의 메타데이터다.
C는 DB 연결·새 통합본 설치·역할 변경·컨테이너 실행·Cloud dispatch를 수행하지 않았다. 이 NOT RUN 표기는
C 자신의 실행 범위이며, B가 별도로 진행하는 후보 설치·등가성 검증의 실행 상태를 뜻하지 않는다. 후보 검증의
최종 상태는 B의 대상 SHA·실행 결과로 판정한다. Cloud 변경 시험은 계속 PLAN ONLY다.
구체적인 연결 반환 동작은 [세션 상태 문서](environment/session-state.md),
Cloud의 관측/미확인 항목과 실행 승인 전 계획은 [Cloud 계획](environment/cloud-plan.md)에 있다.
