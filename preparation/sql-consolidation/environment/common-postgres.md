# 공통 PostgreSQL 구현: 권한 계약과 제한권한 재현

작성일 2026-10-22. 제품 소스의 기준은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`이며 아래 제품 파일
행 번호는 이 기준이다. 검토한 통합 후보·기존 실행 기록은 `a32d8bf` 시점이다. C는 문서·소스·공식 자료를 읽었고
DB에 연결하지 않았다. 로컬 제한권한 시험은 B의 실제 실행 기록으로 판정하며, 실제 Cloud 설치·변경 시험은 **NOT RUN**이다.

## 공통화 원칙

동일한 후보 파일·체크섬을 일반 PostgreSQL과 Supabase에 적용한다. 테이블·함수·제약·RLS·정의자·ACL을 환경별로
분기하지 않는다. Supabase Auth·Storage·Realtime·Data API를 호출하거나 `auth.uid()`로 사용자 문맥을 바꾸지 않는다.
기존 `app.user_id`와 제품 서버의 권한 검사를 유지한다. 설치자는 세 URL과 자격 증명을 제공하고 기존
`db bootstrap` → `db migrate` → 서버 시작을 수행하며 내부 RLS SQL을 편집하지 않는다.

허용되는 초기·접속 차이는 기존 DB의 준비 주체, 관리자 자격 증명, endpoint·TLS·네트워크와 pooler 사용자 표기다.
일반 PG는 자체 관리자가 빈 DB를 준비할 수 있고 Cloud는 제공된 DB를 대상으로 한다. 제품 bootstrap은 어느 쪽에서도
DB 자체를 만들지 않는다. `DATABASE_ADMIN_URL`, `DATABASE_MIGRATION_URL`, `DATABASE_URL` 계약을 유지하고
host/user/password 분할 변수를 다시 만들지 않는다. 직접 접속은 역할명, shared session 접속은
`<role>.<project-ref>`를 URL 사용자로 쓰되 DB 안의 실제 역할은 같은 `fvoci_migrator`/`fvoci_app`이다.
[Supabase 연결 안내](https://supabase.com/docs/guides/database/connecting-to-postgres).

## 현재 구현이 요구하는 권한

| 단계                     | 최소 조건                                                                                                               | 기준 소스와 제한                                                                                                                     |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 관리자 로그인·대상 DB    | LOGIN과 대상 DB CONNECT. 스키마 생성용 DB CREATE, CONNECT를 제품 역할에 부여할 권한                                     | `db/admin.rs:244–258`. DB 소유권 자체는 필요조건이 아니며 `CONNECT WITH GRANT OPTION`으로 시험 가능                                  |
| 두 제품 역할 생성·재사용 | CREATEROLE. 기존 역할은 ADMIN OPTION과 허용된 속성                                                                      | `db/admin.rs:50–115,291–315`. 두 기존 역할을 DDL 전에 검사. superuser·BYPASSRLS는 설치 필요조건이 아님                               |
| 스키마 소유자 지정       | 설치 관리자가 잠시 migrator로 SET할 수 있어야 함. 실제 `CREATE SCHEMA ... AUTHORIZATION`은 관리자 역할로 실행           | `db/admin.rs:152–176,254–261`. 이 단계 때문에 migrator에 DB CREATE를 주지 않음                                                       |
| 임시 권한 반환           | 자신이 추가·변경한 멤버십의 grantor/INHERIT/SET 상태 복원                                                               | `db/admin.rs:179–237`. 기존 ADMIN-only 레코드를 없애거나 플랫폼 멤버십을 일괄 회수하지 않음                                          |
| 마이그레이션             | `fvoci_migrator` 자체 LOGIN·CONNECT, 소유한 `fvoci` 스키마의 객체 생성·변경                                             | `db/admin.rs:264–280,333–340`. 후보 `0001_final_state.sql:7–10`부터 동일 소유자 DDL 적용. 앱 계정·관리자 대행 실행으로 대체하지 않음 |
| 앱                       | `fvoci_app` 자체 LOGIN·CONNECT와 공통 SQL이 부여한 권한. 객체 비소유자, 관리자/migrator 권한을 상속하거나 SET할 수 없음 | `db/mod.rs:67–95`, `tests/supabase.rs:104–123`. 요청 문맥은 트랜잭션 범위이며 앱에는 DB/역할 생성·RLS 우회 불필요                    |
| 백업 생성·복구           | 생성은 migrator. 현 복구는 별도로 관리자 SET 매개변수 권한과 임시 migrator 전환 필요                                    | `backup.rs:136–151,257–305,314,487–490`. 아래 미해결 복구 항목과 분리하여 판정                                                       |

위 파일은 `crates/server/src/` 아래다. `tests/supabase.rs`는 `crates/server/tests/` 아래다.
PostgreSQL은 다른 역할 소유의 스키마 생성에 그 역할로 SET할 권한과 실행자의 대상 DB CREATE를 요구한다.
[PG17 CREATE SCHEMA](https://www.postgresql.org/docs/17/sql-createschema.html).
CREATEROLE 비-superuser가 만든 역할에는 기본적으로 생성자의 ADMIN만 자동 부여되고 INHERIT/SET은 false다.
생성자가 ADMIN을 이용해 필요한 SET 멤버십을 추가할 수 있으므로 기존 bootstrap의 임시 멤버십 경로를 그대로 시험한다.
[PG17 역할 속성](https://www.postgresql.org/docs/17/role-attributes.html).

## B에게 전달한 로컬 제한권한 조건

이 프로필은 Cloud를 복제했다는 주장이 아니라 위 최소 조건으로 공통 코드가 동작하는지 확인하는 시험이다.
격리 클러스터의 fixture 준비만 별도 superuser가 수행하고, 제품 bootstrap부터는 다음 제한 계정으로 실행한다.

- 설치 관리자: `LOGIN CREATEROLE NOSUPERUSER NOCREATEDB NOBYPASSRLS NOREPLICATION`.
  기존 멤버십·전역 읽기/쓰기 역할·매개변수 SET 권한 없음. `createrole_self_grant=''`를 확인한다.
- 대상 DB: fixture superuser가 소유. 설치 관리자에게 이 DB의 `CREATE`와 `CONNECT WITH GRANT OPTION`만 부여한다.
  CREATE의 grant option이나 DB 소유권은 주지 않는다. DB 이름도 `postgres`라는 가정에 의존하지 않는다.
- 제품 역할: 기존 bootstrap이 생성. 두 역할 모두 LOGIN이며 SUPERUSER/CREATEROLE/CREATEDB/BYPASSRLS/REPLICATION은 false.
  다른 역할의 멤버가 아니며 앱은 제품 객체를 소유하지 않는다. 생성자를 향한 ADMIN-only 멤버십과 구별한다.
- 최초/반복 bootstrap 뒤 `pg_auth_members`의 grantor·ADMIN·INHERIT·SET과 스키마 owner를 확인한다.
  별도 로그인에서도 임시 SET이 남지 않아야 한다. 같은 관리 능력만 있고 기존 제품 역할의 ADMIN이 없는 두 번째
  관리자는 변경 전에 거부되며 카탈로그가 보존되어야 한다.
- 같은 후보 체크섬으로 실제 SQLx 최초 설치·재실행, 이력과 객체 소유·ACL·RLS 비교를 수행한다.
  실제 `fvoci_app` URL로 핵심 API·익명 거부·사용자 격리·허용/거부 쓰기·문맥 rollback을 확인한다.
- `CREATE DATABASE`, superuser 역할로 SET, `session_replication_role` 변경이 거부됨을 별도 확인하여 fixture의
  높은 권한이 제품 작업으로 유입되지 않았음을 입증한다. 부정 시험도 해당 격리 DB에서만 수행한다.

`createrole_self_grant`의 기본값은 빈 문자열이다.
[PG17 연결 설정](https://www.postgresql.org/docs/17/runtime-config-client.html#GUC-CREATEROLE-SELF-GRANT).
기존 전체 시험의 `fresh_database`는 별도 DB를 만들고 직접 접속하므로 제한 관리자 시험에서 그대로 실행할 수 없다
(`crates/server/tests/common/mod.rs:300–342`). 선택한 핵심 시험 목록·실행 수·제외 이유를 결과에 명시하고,
그 성공을 기존 전체 시험 성공으로 표기하지 않는다. 기존 PG/Supabase 전체 시험은 별도 증거로 유지한다.
실제 session 경유와 직접 접속도 구별한다. 로컬 제한 프로필만으로 Cloud 인증·TLS·플랫폼 이벤트 트리거·관리 확장이나
pooler 상태 정리가 재현되는 것은 아니다. [세션 검증 범위](session-state.md).

## PG 버전과 공통 기본 함수

후보 `0001_final_state.sql:581,727,853` 등의 UUID 버전 CHECK는 `pg_catalog.uuid_extract_version(uuid)`를 사용한다.
이 함수는 PostgreSQL 17 공식 문서와 17 릴리스 노트에 있는 기본 함수이며 반환형은 `smallint`다.
Supabase 전용 확장·패치가 있어야 하는 함수로 취급하지
않는다. UUIDv7 **생성** 함수의 도입 시기와 UUID 버전 **추출** 함수를 혼동하지 않는다.
[PG17 UUID 함수](https://www.postgresql.org/docs/17/functions-uuid.html),
[PG17 릴리스 노트](https://www.postgresql.org/docs/17/release-17.html),
[PG17.0 태그의 함수 선언](https://github.com/postgres/postgres/blob/REL_17_0/src/include/catalog/pg_proc.dat#L8848),
[PG18 UUID 함수](https://www.postgresql.org/docs/18/functions-uuid.html).

사용자가 최소 PostgreSQL 버전을 17로 확정했다(D-100). 검증한 지원 메이저는 17·18이며 미래 메이저를 자동으로 포함하지 않는다.
일반 PG17.9에서 기존 SQL과 후보의 객체·권한 비교, 각 전체 시험 358건과 제한된 설치 관리자 조건의 핵심 시험 60건을 통과했다.
일반 PG18.3 및 셀프호스트 Supabase PG17.6 결과와 함께 [실행 기록](../verification/results.md)에 조건별로 구분한다.
실제 Cloud 대상의 `server_version_num`, `to_regprocedure('pg_catalog.uuid_extract_version(uuid)')`, 반환형과
두 제품 역할의 EXECUTE 권한, UUIDv7 입력의 결과는 기존 SELECT 1 점검에서 확인하지 않았다. 버전 숫자나
프로젝트 생성일을 추측해 통과 처리하지 않는다. Supabase는 프로젝트별 업그레이드와 현재 서비스 버전 확인을 안내하므로
실제 대상의 버전을 기록해야 한다. [Supabase 업그레이드 안내](https://supabase.com/docs/guides/platform/upgrading).
함수가 없거나 실행할 수 없는 대상은 명확한 미충족 조건으로 기록하며 공통 SQL을 환경별 대체 함수로 분기하지 않는다.
사용자 요청으로 기존 Cloud 읽기 전용 명령에 `server_version_num` 조회만 추가했다. 실제 버전 조회 결과와 다른 능력의
미검증 상태는 [전용 점검 기록](../../../docs/supabase-cloud-readonly-precheck.md)에서 구분한다.

## 실제 Cloud와 공통 복구에 남는 증거

Supabase는 사용자에게 superuser를 제공하지 않으며 custom LOGIN 역할 생성을 안내한다.
[공식 권한 제약](https://supabase.com/docs/guides/database/postgres/roles-superuser),
[공식 역할 안내](https://supabase.com/docs/guides/database/postgres/roles).
이 설명은 특정 프로젝트의 CREATEROLE, 기존 역할 ADMIN, 대상 DB 부여 권한, 매개변수 SET의 실측을 대신하지 않는다.
기존 Cloud 증거는 TLS·session·읽기 `SELECT 1`뿐이다. 실제 설치·마이그레이션·앱·RLS·함수 가용성·복구는
[Cloud 계획](cloud-plan.md)의 별도 시험으로 남긴다. Cloud의 관리 스키마·기본 역할·기존 객체는 보존하고 자동 권한
확대로 실패를 우회하지 않는다. 로컬 관리형 카탈로그 비교의 범위도 전체 관리 객체·데이터 불변 증명으로 확대하지 않는다.

현 복구는 `SET LOCAL session_replication_role=replica`로 FK를 포함한 트리거를 잠시 끈다. 표준 PostgreSQL에서
이는 superuser 또는 해당 매개변수 SET 권한이 필요하다.
[PG17 매개변수 설명](https://www.postgresql.org/docs/17/runtime-config-client.html#GUC-SESSION-REPLICATION-ROLE).
Supabase는 `supautils`를 통해 `postgres`가 role-level로 변경할 수 있는 설정 목록에 이 매개변수를 명시한다.
이는 공식적인 허용 안내다. 그러나 실제 대상에서 제품 복구의 `SET LOCAL`이 성공했다는 실행 증거는 아니며,
기존 셀프호스트 성공 역시 실제 Cloud의 실행 결과를 대신하지 않는다.
[Supabase 관리 설정](https://supabase.com/docs/guides/database/custom-postgres-config).
로컬 제한 관리자에서 거부가 확인되면 현재 복구가 최소 설치 권한만으로 완료되지 않는다는 경계 증거가 된다. Cloud도 거부한다고
단정하거나 복구 기능을 제외하는 결정이 아니다. 공통 복구 검증은 아직 남아 있다.

현재와 같은 복구 SQL에 필요한 능력은 일반 PG에서 운영자가 표준
`GRANT SET ON PARAMETER session_replication_role TO <restore_admin>`으로 준비할 수 있다.
[PG17 GRANT](https://www.postgresql.org/docs/17/sql-grant.html).
Cloud에서는 제공된 관리자 권한으로 같은 SQL이 가능한지 확인하는 것이 먼저다. 기존 셀프호스트 Supabase의 복구
PASS와 공식 허용 안내는 이 공통 경로를 유지할 근거이며, 실제 Cloud 시험은 남아 있다.
이렇게 동일한 복구 구현에 필요한 능력을 초기 준비하는 차이는 최소 환경 설정에 해당한다. 제품이 스스로 관리자
권한을 늘리거나 앱/migrator에 매개변수 권한을 주는 뜻이 아니다. 순수 PG 설치 최소 권한의 거부만으로 새로운
복구 구현이 필요하다고 결론 내리지 않는다.

실제 Cloud에서 필요한 능력을 사용할 수 없다고 확인된다면, 그때 플랫폼 역할을 확대하지 않는 공통 복구 방법을
검토할 수 있다. 소유한 제품 표에 한정한
트리거/FK 처리와 모든 제약의 재검증, 오류 시 트랜잭션 rollback, 기존 빈 DB 복구 정책을 함께 입증해야 한다.
`DISABLE TRIGGER ALL`도 내부 FK 트리거 권한 문제를 자동 해결하지 않으므로 단순 치환으로 간주하지 않는다.
[PG17 ALTER TABLE](https://www.postgresql.org/docs/17/sql-altertable.html).
이 문서는 별도 제품 복구 구현·정책 완화·Cloud 권한 변경을 추가하지 않는다.
