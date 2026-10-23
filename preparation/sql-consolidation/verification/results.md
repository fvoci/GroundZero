# SQL 통합 준비 검증 결과

> **역사 기록이며 현재 상태를 나타내지 않음.** 본문 판단·상태·결과는 명시된 조사/실행 SHA에만 적용한다. 관련 결함이나 SHA를 조사할 때만 참조하며, 현재 기준과 분리 검증은 [증거 안내](../evidence.md)를 따른다.

보존본의 마지막 A/B 실행은 아래 정책 반영 후 후보 검증이다. 이번 증거 분리 작업의 검증과는 별개이며, 이번 작업에서 A/B는 NOT RUN이다.

## 정책 반영 후 후보의 검증 (2026-10-23, 기록된 최종 실행)

채택 정책 D-101~D-105가 구현된 최종 `fvoci/sql-policy` `b01d74a`를 #92에 병합(`3db81d1deae24b16478581ebd6a06de2b2a2d8c9`)하고 [재생성 명령](../inventory/README.md#재생성-명령)으로 후보와 목록을 다시 만든 뒤, 행렬 전체를 **새 출력 경로**에서 차례로 실행했다. 이전 A/B 결과는 하나도 재사용하지 않았다. 모든 실행은 같은 소스와 같은 입력이다.

| 입력                                | 값                                                                            |
| ----------------------------------- | ----------------------------------------------------------------------------- |
| 실행 소스(도구·후보·시험)           | `f5b5a9d4a42dc6c87deddddcda0d0ea41f7092f9` (재생성 결과 커밋, 부모 `3db81d1`) |
| 누적 마이그레이션                   | 97개 파일, 마지막 버전 98. `crates/server/migrations/`는 `b01d74a`와 같다     |
| 후보 `0001_final_state.sql` SHA-256 | `24a0a3dfaf7b2cf09152a11f80ff2100dd6004990b31cb661bf52a688ae9eea0`            |
| `sql_compare.rs` / `catalog.sql`    | `04a606a8…356925f` / `e0f8ee93…10f26c3e` (전체 값은 각 `inputs.json`)         |
| 셀프호스트 Supabase 고정 revision   | `bc27d2661edd09a5d0b8d76ce33da6d4071a756e`                                    |
| 서버 버전(각 `*-environment.json`)  | PostgreSQL 17.9, 18.3(`compose.test.pg18.yaml`), Supabase PostgreSQL 17.6     |

| 실행 (`runs/`)                                                                                                                                                               | 명령 옵션                                                                               | 객체·변조·정지                | A와 B 각각의 Rust 결과                                               | 로컬 출력 `target/sqlp2-…-p2-20261023t065441z` |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------- | ---------------------------------------------- |
| [PG17 A/A](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-pg17-aa)                 | `--postgres-major 17`                                                                   | 차이 0/0, 4/4, PASS           | NOT RUN(요청 안 함)                                                  | `pg17-aa`                                      |
| [PG17 A/B 전체](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-pg17-full)          | `--postgres-major 17 --candidate … --behavior`                                          | 차이 0/0, 4/4, PASS           | 47그룹, 377 passed, 0 failed/ignored, 18 filtered                    | `pg17-full`                                    |
| [PG17 A/B 전체+OIDC](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-pg17-oidc)     | 위 + `--oidc`                                                                           | 차이 0/0, 4/4, PASS           | 47그룹, 383 passed, 0 failed/ignored, 12 filtered                    | `pg17-oidc`                                    |
| [PG17 제한 관리자](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-pg17-restricted) | `--postgres-major 17 --candidate … --restricted-admin --behavior --behavior-scope core` | 차이 0/0, 4/4, 권한·정지 PASS | 7그룹, 60 passed, 0 failed/ignored/filtered                          | `pg17-restricted`                              |
| [PG18 A/B 전체+OIDC](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-pg18-oidc)     | `--postgres-major 18 --candidate … --behavior --oidc`                                   | 차이 0/0, 4/4, PASS           | 47그룹, 383 passed, 0 failed/ignored, 12 filtered                    | `pg18-oidc`                                    |
| [Supabase A/B](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs/policy-supabase)            | `--candidate … --supabase --behavior`                                                   | 차이 0/0, 4/4, PASS           | direct·session 각각 47그룹, 389 passed, 0 failed/ignored, 6 filtered | `supabase`                                     |

「차이 0/0」은 의미 카탈로그·정규 덤프 차이 수, 「4/4」는 정책·권한·함수·CHECK 변조의 검출·원복이다. 각 A/B 실행의 A 이력은 97건·최대 98, B는 후보 1건·버전 1이며 A/A는 양쪽 97건이다(`migration-ledger-summary.json`). 양쪽 `catalog.json` SHA-256은 실행마다 A와 B가 같다(`local-artifacts.sha256`).

- filtered는 기존 규칙으로 걸러진 시험이다. 표준 PG의 `--oidc` 없는 실행은 OIDC 6건과 Supabase 전용 12건(18), `--oidc` 실행은 Supabase 전용 12건, Supabase 실행은 OIDC 6건이다. 합계가 서로 맞는다(377+6=383, 377+12=389).
- `--oidc` 실행의 OIDC 6건은 `oidc_*` Keycloak 통합 시험 5건과 `auth_info_is_public_and_reports_whether_oidc_is_configured`이며 PG17·PG18의 A와 B에서 모두 통과했다. Keycloak은 각 실행 전용 검색·S3 프로젝트의 `oidc` 프로필 서비스이고 A·B가 차례로 같은 인스턴스를 썼다. 사용자가 요청한 후보 OIDC 실행은 이 PG17 행이다. PG18은 `--oidc`를 붙인 한 번으로 실행했고 OIDC 없는 PG18 별도 실행은 하지 않았다(NOT RUN).
- 제한 관리자 실행은 A·B 각각 기대 SQLSTATE `42501` 거부 20개, bootstrap·SQLx 재실행 동일성, 역할 속성·멤버십·자격 증명 보존을 다시 확인했다(`a-capabilities.json`, `b-capabilities.json`). 범위는 core 7개 바이너리이며 백업/복구·FreshDb 검색·OIDC·S3·Supabase 전용 경로는 이 행에서 NOT RUN이다.
- Supabase session 단계의 일반 앱 연결과 `db_context`는 실제 session pooler를 쓰고, FreshDb 기반 백업·검색 시험은 기존 규칙대로 direct 연결이다. 389건 모두가 pooler를 통과했다는 뜻이 아니다. Supabase에서는 `--oidc`를 붙이지 않았다(NOT RUN).
- 정책 반영 후 시험에는 #97·#103이 더한 판 검사·검색·업무 만들기 시험이 들어 있어 이전 수치(358·364 등)와 직접 비교하지 않는다.

**지원 메이저(D-100).** 최소 버전은 PostgreSQL 17이며 이 후보로 실제 검증한 메이저는 표준 17(17.9)과 18(18.3), 셀프호스트 Supabase 17(17.6)뿐이다. `17 이상`을 미검증 차기 메이저의 지원 약속으로 쓰지 않는다.

**NOT RUN.** 이 결과는 빈 DB 새 설치의 동등성이다. 기존 데이터가 있는 DB의 누적 이력(최대 98)→후보(버전 1) 전환, 기존 백업의 새 버전 복구, SQLx 이력 교체, 0095의 전체 재색인을 기존 데이터에서 실행하는 경로는 검증하지 않았다. Supabase Cloud 실행·변경, Supabase의 `--oidc`, transaction pooler, 실제 운영 데이터도 NOT RUN이다.

**자원.** 각 실행은 자체 Compose 프로젝트(17개)만 만들고 `stop`했다. 그 컨테이너 28개는 모두 exited다. Keycloak 2개는 realm 파일, Supabase 컨테이너는 고정 revision의 구성 파일(pooler 설정 등)을 bind mount로 읽고, Supabase A·B DB 컨테이너는 기존 Supabase 구성의 이름 붙은 `db-config` 볼륨 2개를 남겼다(지우지 않았다). 공유 개발 자원에는 손대지 않았다. 텍스트 로그는 줄 끝 공백만 정리했고 원본 콘솔 로그 해시는 [원본 로그 해시](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/diagnostics/original-orchestration-logs.sha256)에 더했다.

**시험 실행(dry run, 증거 아님).** 정책 브랜치가 아직 최종이 아니던 `874d866`을 병합한 `5c26622`·`4f53d82`로 재생성 절차와 PG17 A/A·PG17 전체 A/B를 먼저 돌렸다(`target/sqlp2-*-p2-20261023t060246z`). 이후 #103이 판 비교를 바꿨으므로 그 결과는 기록하지 않고 위 실행으로 대체했다.

## 대체된 기록과 재현

- 정책 반영 전 후보 `b8172cf…55725`는 `cb65cc3`의 93개 마이그레이션 기준이다. 해당 PASS와 중간 FAIL 진단은 [전체 과거 결과](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/results.md)에 보존하며 현재 후보 검증으로 재사용하지 않는다.
- 과거 실패에는 CHECK 출력 표현, 후보 search_path, 제한 역할 시험 환경과 Supabase peer 인증 진단이 포함된다. 최종 기록과 구분하고 오류 원문은 보존 SHA에서만 확인한다.
- PG16 거부·PG17 ACL/자격 증명 사전 검사는 소스 `9c122173e4c6ec5d33c6be261975f68753008015`의 결과다. [fixture와 명령](fixtures/README.md)은 로컬에 유지한다.
- 후보 [재생성 명령](../inventory/README.md#재생성-명령)과 [A/B 실행 명령](README.md)은 유지한다. 출력은 새 `target/` 경로를 사용하며, 재현에는 해당 소스 SHA와 도구·이미지 조건이 필요하다.
