# SQL 통합 준비: Cloud 관측과 후속 시험 계획

상태: **PLAN ONLY**. C 작업자는 원격 DB 연결·bootstrap·migrate·역할/비밀번호 변경·Cloud workflow dispatch를
실행하지 않았다. 사용자가 추가로 지시한 표준 PostgreSQL 공통 구현은 Cloud의 권한 제약을 포함해 설계한다.
일반 PostgreSQL/Supabase 제품 SQL 분기나 Supabase 서비스 의존성은 만들지 않는다. 아래 단계는 별도 승인된 전용
프로젝트에서 실행할 후속안을 구체화한 것이며, 공통 설계·로컬 시험 권한을 원격 변경 권한으로 해석하지 않는다.

## 확인한 기존 증거

| 항목                  | 결과                                                                | 근거와 한계                                                                                                                      |
| --------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| 기존 run              | `39600271650`, success                                              | [GitHub 실행](https://github.com/fvoci/GroundZero/actions/runs/39600271650). 2026-10-22에 GitHub API로 상태·SHA·step 성공 재조회 |
| 실행 SHA·시각         | `f9e5fd035601de5f5236b199400a8fc4a8fa74d4`, 2026-10-21 12:31:22 UTC | `precheck` job과 `Read-only session pooler precheck` step이 success. 새 실행 아님                                                |
| 연결·TLS·SELECT 1     | 기존 기록 PASS                                                      | `docs/supabase-cloud-readonly-precheck.md:106–110`. 실제 run 결과의 저장소 기록이며 이번에는 원격 DB에 다시 연결하지 않음        |
| 대상 역할·DB 권한     | 미확인                                                              | 점검은 역할 카탈로그를 조회하지 않음. 관리자 용도 Secret이라는 사실만으로 실제 역할·CREATEROLE·ADMIN OPTION을 입증하지 않음      |
| PG 버전·공통 함수     | 미확인                                                              | 대상의 `server_version_num`, `pg_catalog.uuid_extract_version(uuid)` 서명·EXECUTE 권한은 기존 점검에서 조회하지 않음             |
| 제품 설치·앱·RLS·복구 | NOT RUN                                                             | 기존 점검도 명시적으로 미수행. 앱 prepared statement·복수 연결·pool 재사용도 미검증                                              |

기준 소스(`cb65cc31156c0fd8d28cf856bc132d4a506c8885`)의 점검은 session pooler 5432·DB `postgres`만 허용하고,
CA를 지정한 VerifyFull, statement cache 0, 단일 연결의 read-only `SELECT 1`과 rollback만 실행한다
(`xtask/src/supabase_cloud_precheck.rs:147–153,186–199,256–279`). 호스트·사용자명·비밀번호·CA 원문은
문서나 결과에 복사하지 않는다. 기존 6개 `SUPABASE_TEST_*` Secret은 이 점검 전용으로 유지한다.

공식 연결 안내는 custom role의 shared-pooler 사용자 형식을 `<role>.<project-ref>`로 설명한다. 직접 연결은
프로젝트의 IP 조건을 따르며 session 연결은 IPv4 환경의 대안이다. 이 안내는 **해당 Cloud 프로젝트에서** 새 앱/마이그레이션
역할 로그인·권한·세션 설정이 동작했다는 증거가 아니다.
근거: [Supabase 공식 연결 안내](https://supabase.com/docs/guides/database/connecting-to-postgres)
(2026-10-22 확인). 호스트는 Dashboard에서 받은 값을 쓰고 지역 문자열로 조합하지 않는다.

공식 문서는 사용자에게 superuser를 제공하지 않는다고 설명하고 custom LOGIN 역할 생성을 안내한다. 그러나 특정
프로젝트의 관리자 속성·ADMIN OPTION·DB CREATE/CONNECT 부여 권한·매개변수 SET 권한을 모두 보증하는 것은 아니다.
관리 설정 문서는 `supautils`를 통한 `postgres`의 role-level 설정 허용 목록에 `session_replication_role`을
명시한다. 이는 허용 가능성의 공식 근거이며, 현재 대상에서 제품 복구가 사용하는 `SET LOCAL`을 실행한 증거는 아니다.
근거: [superuser 제약](https://supabase.com/docs/guides/database/postgres/roles-superuser),
[역할 생성](https://supabase.com/docs/guides/database/postgres/roles),
[관리 설정](https://supabase.com/docs/guides/database/custom-postgres-config).
정확한 최소 권한과 PG17 기본 함수 근거는 [공통 계약](common-postgres.md)을 따른다.

## 승인 전 확정할 실행안

| 단계                   | 구체적인 작업                                                                                                                                                                                     | 성공 기준·실패 시 처리                                                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| C0 대상과 범위         | 전용/폐기 가능한 프로젝트, 유지할 데이터, 릴리스 SHA, 승인된 연결 종류·역할·작업 범위를 기록. 역할 이름은 클러스터 전역임을 확인                                                                  | 기존 제품/공유 DB에 통합 후보를 적용하지 않음. 삭제·프로젝트 reset은 기본 작업에 넣지 않음                                                 |
| C1 읽기 전용 능력 확인 | 별도 승인 범위에서 실제 DB·session/current user, PG 버전, UUID 함수 서명·실행권한, DB CREATE/CONNECT grant option, 역할 CREATEROLE/ADMIN/SET·상속, 기존 `fvoci` 소유자, 관리형 객체 목록·ACL 확인 | 기존 SELECT 1 승인보다 넓은 조회임을 명시. 권한이 모자라면 실패 조건을 기록하고 종료. 관리자/관리형 역할 자동 변경 없음                    |
| C2 설치 시험 준비      | 제품의 세 URL을 동일 승인 대상에 맞추고 TLS를 검증. 앱·migrator 로그인 경로 결정. C1에서 필요한 권한과 이름 충돌이 없는지 확인                                                                    | direct와 session은 각각 별도 결과. 기존 단일 읽기 Secret에서 새 제품 비밀번호를 추출/재사용하지 않음. Secret 제공 방식은 실행 승인 시 확정 |
| C3 최초 bootstrap      | 승인된 새 역할·스키마에 기존 명령을 실행. 두 역할 속성·멤버십·스키마 소유·관리자의 임시 멤버십 전후 비교                                                                                          | 한 단계 실패 시 DDL rollback과 비밀번호/기존 역할 불변 확인. 문제가 생겼다고 수동 RLS SQL을 설치자에게 요구하지 않음                       |
| C4 최초 migrate·재실행 | 고정된 통합 후보 SHA의 `db migrate`. 성공 후 같은 명령 재실행, 적용 이력·객체 소유·권한·RLS·정의자 search_path 비교                                                                               | 무변경 재실행 성공, 제품 외 객체 불변. dirty/checksum/SQL 오류면 데이터·증거를 보존하고 중단; `_sqlx_migrations` 수정 금지                 |
| C5 실제 앱             | `fvoci_app`의 자체 자격 증명으로 로그인한 제품 SQLx 풀 사용. 익명 거부, A/B 사용자 격리, 개인 노트, 관리자 범위, 허용/거부 쓰기, 정의자 호출 확인                                                 | 소유자 계정으로 대체 실행한 성공은 인정 안 함. 허용된 제품 쓰기와 RLS 거부를 모두 기록                                                     |
| C6 풀과 실패 경계      | [세션 계획](session-state.md)의 동일 물리 연결 재사용·오류 rollback·취소·pool 종료 후 새 클라이언트를 실행                                                                                        | 앱 풀 반환과 pooler backend 반환을 구분. session 상태·역할이 다음 요청에 남지 않음. transaction 모드로 대체하지 않음                       |
| C7 백업·복구(별도)     | 먼저 저장소 포함 범위와 빈 복구 대상을 승인. `session_replication_role` 실제 설정 가능 여부, 같은 연결의 advisory lock, 임시 멤버십 회수 확인                                                     | 설치/앱 성공과 별도 판정. 권한이 없으면 공통 복구의 미해결 조건으로 기록. 기능 제외나 플랫폼 역할 권한 확대를 자동 결정하지 않음           |
| C8 보존·정리           | 실행 후 제품/관리형 카탈로그와 생성 객체 목록을 대조하고 자격 증명 노출 없는 결과 기록. 잔존 시험 자원은 이름과 소유자 보고                                                                       | 사용자 데이터·관리형 객체·전체 DB reset 없음. 승인된 생성물의 삭제도 실행 승인에 명시된 범위만 수행                                        |

기존 로컬 전체 시험 하니스는 Cloud에 바로 적용하지 않는다. `xtask test --supabase`는 공식 컨테이너를 띄우고,
`--external-services`도 시험용 계정·DB를 조합해 bootstrap을 호출한다(`xtask/src/test.rs:105–170,203–238`).
`fresh_database`는 추가 DB 생성과 직접 연결을 사용하므로 기존 Cloud 읽기 점검의 session·단일 DB 조건과 다르다
(`crates/server/tests/common/mod.rs:300–342`). 필요한 Cloud 시험 범위를 먼저 나누고, 승인된 대상을 검증한 전용 진입점이
있어야 한다. 이 문서에서 새 하니스나 변형 workflow를 만들지 않았다.

## 정책 결정과 기술 선택

Cloud 제약을 고려한 공통 설계·로컬 제한권한 검증은 승인된 작업이다. 실제 Cloud 시험·운영 비용·새 서비스·새 권한,
transaction pooler 지원, 기존 데이터 이전/삭제까지 승인된 것은 아니다. 기존 PostgreSQL·셀프호스트 Supabase의
성공, 제한권한 로컬 성공, 실제 Cloud 성공은 각각 구분해서 기록한다. 현 복구에 필요한 표준 PG의 매개변수 SET
권한과 Cloud의 제공 권한은 같은 복구 SQL을 위한 최소 초기 설정 차이로 다룰 수 있다. 공식 Cloud 허용 안내와 기존
셀프호스트 복구 PASS를 근거로 먼저 같은 구현을 시험한다. 실제 Cloud에서 필요한 능력을 사용할 수 없다고 확인되면
그때 플랫폼 관리자 권한을 넓히지 않는 공통 복구 방법을 검토할 수 있다. 소유한 표 범위의 트리거/FK 처리와 완전한
재검증, 트랜잭션·기존 빈 DB 복구 정책 보존을 함께 검토해야 하며 이 문서에서 별도 구현을 추가하지 않는다.

#69의 미승인 P-24·P-25·P-26·P-28·P-29·P-30·P-31·P-35는 SQL 통합 시 현행 정책을 보존하고 이식/보류 항목으로
명시한다. 준비 문서가 그 제안을 승인하지 않는다. 설치 환경 점검을 계기로 새로운 정책을 구현하거나, 반대로
Cloud가 아직 미검증이라는 이유로 기존 셀프호스트 지원 경로를 줄이지 않는다.
