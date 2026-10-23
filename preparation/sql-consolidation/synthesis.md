# 종합 판단과 전환 조건

현재 결론은 **먼저 해결할 항목 있음**이다. 현재 후보의 기준은 정책 반영 후 `3db81d1deae24b16478581ebd6a06de2b2a2d8c9`
(#92에 `fvoci/sql-policy` `b01d74a` 병합, 적용 순서 #97 → #92)이고, 처음 조사 기준은 `cb65cc31156c0fd8d28cf856bc132d4a506c8885`다.
후보 검증과 최종 main 확정은 별개의 조건이다.
실제 SQL 교체·삭제·번호 재정리, 기존 DB 이력 변경, main 병합·태그·배포는 수행하지 않는다.

## 정책 반영 후 후보에서 확인한 결과 (최신)

실행 소스 `f5b5a9d`(재생성 기준 `3db81d1`, 후보 SHA-256 `24a0a3df…e9eea0`), 모두 새 출력 경로의 2026-10-23 실행이다.

| 항목                                                                                      | 결과                                                                          |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| PostgreSQL 17.9 A/A 대조군                                                                | 객체 차이 0건, 정책·권한·함수·CHECK 변조 4/4 탐지·원복 PASS                   |
| PostgreSQL 17.9 누적 SQL / 후보                                                           | 객체 차이 0건, 양쪽 각 377개 Rust 시험 PASS, 변조 4/4 PASS                    |
| PostgreSQL 17.9 누적 SQL / 후보 + OIDC                                                    | 객체 차이 0건, 양쪽 각 383개(OIDC 6개 포함) PASS, 변조 4/4 PASS               |
| PG17.9 제한 설치 관리자 A/B                                                               | 객체 차이 0건, 핵심 앱 시험 60개 PASS, 권한 거부 20개 확인                    |
| PostgreSQL 18.3 누적 SQL / 후보 + OIDC                                                    | 객체 차이 0건, 양쪽 각 383개(OIDC 6개 포함) PASS, 변조 4/4 PASS               |
| Supabase PostgreSQL 17.6 직접·session pooler                                              | 객체 차이 0건, 양쪽·두 경로 각 389개 PASS(FreshDb는 직접 연결), 변조 4/4 PASS |
| 종합 `cargo xtask check`                                                                  | Rust 단위 104개·웹 751개(135파일), 포맷·lint·타입·계약·고지·라이선스 PASS     |
| 지원 메이저(D-100)                                                                        | 최소 17. 이 후보로 검증한 메이저는 표준 17·18과 셀프호스트 Supabase 17뿐이다  |
| Cloud 실행·변경, 기존 데이터/백업/SQLx 이력 전환, Supabase OIDC, PG18 OIDC 없는 별도 실행 | NOT RUN — 새 설치 동등성은 전환 결과가 아니다                                 |
| #69·후속 완료 및 최종 main 델타 확정                                                      | #69·#94는 main 병합 확인. #97 → #92 적용 뒤 최종 main 델타 확정은 미완료      |

## 정책 반영 전 기준 SHA에서 확인한 결과 (이력)

아래는 `cb65cc3` 기준 후보 `b8172cf…55725`의 기록이며 최신 검증으로 재사용하지 않는다.

| 항목                                    | 결과                                                                                                  |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| PostgreSQL 18.3 A/A 대조군              | 객체 차이 0건, 정책·권한·함수·CHECK 변조 4/4 탐지·원복 PASS                                           |
| PostgreSQL 17.9 누적 SQL / 후보         | 객체 차이 0건, 양쪽 각 358개 Rust 시험 PASS, 변조 4/4 탐지·원복 PASS                                  |
| PostgreSQL 18.3 누적 SQL / 후보         | 객체 차이 0건, 양쪽 각 353개 Rust 시험 PASS                                                           |
| Supabase PostgreSQL 17.6 직접 연결      | 객체 차이 0건, 양쪽 각 364개 Rust 시험 PASS                                                           |
| Supabase PostgreSQL 17.6 session pooler | 양쪽 각 364개 Rust 시험 PASS; FreshDb 시험은 아래의 직접 연결 예외 적용                               |
| Supabase 변조 검사                      | 정책·권한·함수·CHECK 4/4 탐지·원복 PASS                                                               |
| PG17.9 / PG18.3 제한 설치 관리자 A/B    | 각 환경의 양쪽에서 객체 차이 0건, 핵심 앱 시험 60개 PASS; 권한 거부 20개 확인                         |
| 종합 `cargo xtask check`                | Rust 단위 93개·웹 734개, 포맷·lint·타입·계약·고지·라이선스 PASS                                       |
| PG17 변경 후 종합 Rust 검사             | 단위 99개와 전체 대상 Clippy PASS, 포맷 검사 PASS                                                     |
| 초기화 전 거부 경로                     | PG16.13 두 경로와 PG17 권한 부족 9개 경로의 거부·카탈로그 무변경 PASS; 별도 SCRAM 자격 증명 보존 확인 |
| PG17.9 OIDC 포함 A/B 도구 확인          | 정책 반영 전 후보로 양쪽 각 366개(OIDC 6개 포함) PASS — `--oidc` 경로 확인용, 최종 증거 아님          |

수치는 경로별 전체 Rust 시험 합계이며 서로 다른 시험 수로 합산하지 않는다. 모든 완료 경로의 실패·무시는 0이다.
명령·입력 SHA·로그·진단 차이는 [DB 실행 결과](verification/results.md), 종합 검사와 실행 소스의 대응은
[품질 검사](quality-check.md)에 있다. 리뷰의 현재 상태는 [Draft PR #92](https://github.com/fvoci/GroundZero/pull/92)에서 확인한다.

CodeRabbit이 지적한 부분 기동 실패와 정지 오류의 처리도 수정했다. 이후 실패 경로 시험 3개를 포함한
Rust 단위 시험 96개, Clippy, 실제 A/A의 설치·객체·변조·정지 단계가 통과했다.
위 여섯 행동 경로는 수정 전후 동일한 후보·비교 쿼리·행동 입력으로 실행한 기록이며,
정지 처리만 바뀐 최종 도구의 후속 검증과 구분하여 보존한다.

리뷰 본문의 추가 제안 중 출력 경로의 동시 확보 경합은 원자적 생성으로 고쳤고,
8개 동시 요청 시험을 포함한 xtask 16개 시험과 Clippy가 통과했다.
프로세스 강제 중단 후 실행 소유 자원 복구는 중복 이슈를 확인한 뒤
비차단 후속 [#93](https://github.com/fvoci/GroundZero/issues/93)으로 연결했다.
일반 종료·오류의 정지 검증을 SIGINT/SIGTERM 처리까지 검증했다고 확대하지 않는다.

## 채택한 설계와 보류한 제안

| 검토 항목                                | 종합 판단                                                                                                                                                                                                                |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 누적 SQL의 단순 연결 대 최종 정의 재구성 | 실제 적용된 카탈로그의 최종 정의를 채택했다. 표·함수는 마지막 형태로 한 번 만들고, 객체 의존 순서를 보존한다. 97개 적용 이력은 원본 경로에 그대로 남긴다.                                                                |
| 과거 데이터 보정 SQL                     | 빈 설치에 불필요한 과거 행 UPDATE·전환용 DROP을 후보에 재생하지 않는다. 그 의미인 현재 CHECK·FK·번호·권한·검색 갱신 동작은 유지한다. 기존 데이터 이관으로 해석하지 않는다.                                               |
| UUIDv7와 환경 지원                       | 앱이 UUIDv7을 제공한다. PostgreSQL/Supabase의 제공 기능과 bootstrap 역할 어댑터를 유지하고 DB UUID 생성 fallback을 추가하지 않는다.                                                                                      |
| SQLx 이력                                | 후보 자체의 진짜 이력을 만든다. 누적 이력 행과 다름을 명시하며 이력 테이블 구조는 비교한다. 기존 백업의 다른 버전 복구를 자동 허용하지 않는다.                                                                           |
| 객체 비교 대 앱 시험                     | 각각 통과해야 한다. 앱 시험 성공으로 카탈로그 차이를 덮지 않으며, 카탈로그 일치로 권한·경합 동작을 추정하지 않는다.                                                                                                      |
| 설치 역할 guard(E1–E3)                   | 기존 구성의 별도 개선 제안이다. 후보가 기존보다 권한을 넓히는 회귀와 구분하며 이 준비에서 제품 동작을 바꾸지 않는다.                                                                                                     |
| P-24/25/26/28/29/30/31/35                | 채택된 P-24·25·26·30·31(D-101~D-105)은 `fvoci/sql-policy`에 구현됐고 후보·목록을 그 최종 상태에서 다시 만들었다. P-28은 현행 유지(D-106), P-29·P-35는 보류·추적(#96·#95)이다. 통합을 근거로 다른 제안을 구현하지 않는다. |
| Cloud                                    | 기존 TLS/읽기 전용 성공과 제품 설치·RLS 검증을 분리한다. 원격 DB 변경은 실행하지 않는다.                                                                                                                                 |
| 최소 PostgreSQL 버전                     | D-100에 따라 17을 최소로 정하고 검증한 17·18만 지원 목록에 둔다. PG17 미만은 bootstrap·migrate의 DDL/이력 생성 전에 거부한다. 미래 메이저 지원을 약속하지 않는다.                                                        |

권한·업무 규칙·경합/409·데이터 정합성의 최종 판단은 기존 서버와 DB에 남는다.
이번 변경은 프런트엔드를 수정하지 않으며, 화면 좌표·미리보기·입력 안내를 서버로 옮기는 변경도 없다.

추가 사용자 지시에 따라 Cloud의 권한 제약을 고려한 표준 PostgreSQL 공통 구현을 우선한다.
일반 PG용·Supabase용 제품 SQL을 분리하지 않고, 불가피한 환경 차이는 최소 초기 설정·접속 설정으로 한정한다.
Supabase 전용 서비스 의존성을 추가하지 않는다. 이 우선순위는 Cloud에서 실제 설치·권한·동작이 검증됐다는
뜻이 아니며, 원격 Cloud 변경 시험은 기존 PLAN ONLY 범위를 유지한다.

추가 검증에서 일반 PG17.9와 PG18.3의 설치 관리자는 `CREATEROLE`과 대상 DB `CREATE`,
`CONNECT WITH GRANT OPTION`만 받았다. SUPERUSER·CREATEDB·BYPASSRLS·REPLICATION과 DB 소유권은 없다.
같은 누적 SQL/후보의 bootstrap·migration 재실행 뒤 객체·원장·ADMIN-only 멤버십이 같았으며,
다른 관리자의 변경 요청 거부 후 원래 자격 증명도 유지됐다. 각 측의 금지 동작 20개가 `42501`로 거부됐다.
실제 앱 역할로 7개 기존 시험 파일의 60개 시험이 각각 통과했다. 백업·검색·S3·OIDC와 나머지 시험은 이 추가
핵심 범위에서 제외했으며, 이전 전체 환경 행렬과 구분한다. 명령·전체 로그·권한 실측은 [실행 결과](verification/results.md)에 있다.

공통화 설계는 하나의 제품 SQL과 기존 세 URL 계약을 유지한다. `uuid_extract_version`은 PG17.0부터의 표준 함수라
Supabase 보완 계층이 필요하지 않다. 복구의 매개변수 SET 권한은 최소 설치 권한보다 강하지만,
표준 PG의 초기 권한 부여와 Cloud 제공 권한으로 같은 SQL을 사용할 수 있는지 먼저 확인한다.
제한 프로필의 거부를 Cloud의 거부로 해석하거나, 복구 기능 제외·별도 제품 SQL 도입을 결론내리지 않는다.
[환경 공통 계약](environment/common-postgres.md)은 실제 Cloud 역할·함수·접속·복구 검증이 남았음을 명시한다.

PG17 최소 기준에 맞춰 기존 CI `integration`은 17.9, 기존 OIDC/S3 작업은 18.3으로 분담했다.
새 CI 행렬을 늘리지 않았고 개발·설치용 PG18 데이터 볼륨에 PG17 이미지를 연결하지 않는다.
정책 반영 전 후보의 PG17 전체 실행은 누적/후보 각각 358개 PASS였고(정책 반영 후는 위 표의 377개), 이전 PG18·Supabase 실행과 소스 SHA를 구분한다.
숫자형 Cloud 버전 조회는 기존 읽기 전용 명령에 추가했다. main 전용 자격 증명 게이트를 유지하므로
실제 Cloud 서버 버전은 아직 NOT RUN이며, 이 변경의 검토·main 반영 후 기존 수동 workflow로 확인해야 한다.

## 작업자 간 교차 검토

A의 후보와 B의 실제 SQLx 적용을 연결하면서 이력 INSERT 전 search_path 복원 누락을 발견해 고쳤다.
누적 SQL과 후보의 카탈로그가 같아도 원장 쓰기가 실패하면 설치 PASS로 기록하지 않는다.
현재 후보의 SHA-256은 `24a0a3dfaf7b2cf09152a11f80ff2100dd6004990b31cb661bf52a688ae9eea0`다(정책 반영 전 후보 `b8172cfb…55725`는 이력).

C는 B의 시험 자식 프로세스가 일반 저장소 환경 변수를 상속할 수 있음을 발견했다.
실행 중이던 프로세스에서는 해당 설정이 모두 미설정임을 값 없이 확인했고, 비교기는 일반 저장소·검색·S3 설정까지
격리된 시험 값으로 고정하도록 수정했다. 수정된 도구로 최종 행동 비교를 다시 수행해 통과했다.
C가 지적한 관리 스키마 비교 범위의 과장도 바로잡았다. 관리 객체의 메타데이터/ACL 확인을
그 객체의 모든 열·제약·정책·트리거 정의나 변경 전후 전체 보존 검증으로 주장하지 않는다.

A의 비교기 교차 검토에서는 중복 카탈로그 키와 덤프 키가 실패로 처리되고, 함수 본문·ACL·RLS·트리거·초기 행을
비교에서 숨기지 않음을 확인했다. 이름 기반 키와 단일 CHECK 정규화의 실제 탐지 능력은 별도 변조 시험으로 확인한다.
이는 새 제품 전면 감사가 아니라 이 준비 산출물 사이의 일치 여부 확인이다.

초기 A/A의 수정 전 종료 경로는 이 작업 전용 B 임시 컨테이너·네트워크를 `down --volumes --remove-orphans`로
제거했다. 해당 PostgreSQL은 tmpfs였으며 이름 붙은 볼륨은 없었다. 공유 DB나 기존 볼륨은 대상이 아니었다.
최종 도구와 이후 실행은 `stop`만 사용한다. 초기 실행도 정지만 했다는 중간 설명은 로그 확인 후 정정했다.

## 실제 앱 역할 시험과 요구사항 대응

아래는 비교 도구가 양쪽에 실행하는 기존 Rust 시험의 대표 경로다. 별도 모의 구현으로 대체하지 않는다.
전체 실행 여부·통과 수는 실행 결과에 따르며 이 표 자체는 성공 증거가 아니다.

| 요구                          | 기존 시험 경로와 대표 시험                                                                                                                                                                                           |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 생성·저장·재열기·멱등성       | `first_flow.rs`: `document_write_save_reopen_with_revisions`, `workspace_create_is_idempotent_and_listed`                                                                                                            |
| 워크스페이스/사용자 격리      | `first_flow.rs`: `other_user_cannot_see_or_change_documents`, `rls_hides_rows_from_other_users_under_app_role`                                                                                                       |
| 개인 문서와 관리자 경계       | `sharing.rs`: `shared_personal_note_does_not_reach_the_grantees_workspace_admin`, `private_team_document_is_visible_to_creator_admin_and_shares_only`                                                                |
| 즉시 권한 철회·멤버 제거 경합 | `sharing.rs`: `share_permissions_and_revocation_take_effect_immediately`; `members.rs`: `removed_member_loses_access_and_team_documents_remain`, `adding_a_member_who_is_being_removed_does_not_survive_the_removal` |
| 오래된 수정·승인과 409        | `first_flow.rs`: `save_retry_is_idempotent_and_stale_save_conflicts`; `schedule.rs`: `two_approvals_of_the_same_preview_save_once`, `link_edits_from_a_stale_view_conflict_instead_of_overwriting`; `db_errors.rs`   |
| 반복 일정·업무·잠금 순서      | `recurrence.rs`: `event_rule_saves_from_a_stale_rule_conflict`, `the_recurrence_tick_takes_lane_locks_in_the_same_order_as_board_moves`, DST 회차 시험                                                               |
| 개인 시간 기록·범위 검증      | `time_tracking.rs`: `one_timer_per_user_survives_pause_and_stays_private`, `a_running_timer_keeps_its_pauses_inside_its_span`                                                                                        |
| 검색 색인·권한 재검사         | `search.rs`의 실제 DB·Meilisearch 경로 및 `worker.rs`                                                                                                                                                                |
| UUID 공급 계약                | `ids.rs`: `functions_use_the_ids_the_server_passes_and_never_fill_in_missing_ones`, `tables_have_no_id_default_and_accept_only_uuidv7`                                                                               |
| 연결 반환 뒤 사용자 문맥      | `db_context.rs`: `user_context_does_not_outlive_commit_or_rollback`; Supabase session 단계의 일반 앱 URL로 실행                                                                                                      |
| 백업·복구와 새 DB             | `backup.rs` 및 `common::fresh_database`: 후보도 동일 후보 SQLx 이력으로 준비. session 단계에서도 새 DB는 직접 연결이라는 한계를 표시                                                                                 |

파일 위치는 모두 처음 조사 기준 SHA의 `crates/server/tests/`다. OIDC·Cloud·브라우저 검증 범위는
각 실행 기록의 NOT RUN 항목을 따른다. session 풀 반환 시 임의 세션 상태 전체가 자동 초기화된다고 주장하지 않는다.

## 실제 전환 순서

1. #69와 D-99/W21 후속 #94는 main(`6ecad60`)에 병합됐고 정책 브랜치·현재 기준에 포함된다(2026-10-23 확인).
   남은 것은 정책 PR #97을 먼저 적용한 뒤의 최종 main SHA 기록과, 현재 기준(`3db81d1`)과의 델타 재검토다.
2. SQL·호출자·seed·설치·권한 계약에 영향이 있으면 후보와 대응 시험을 갱신한다. 웹 문구만 바뀌었으면 그 근거를 기록한다.
3. 최종 후보의 PostgreSQL·셀프호스트 Supabase 직접/session 결과, 객체 차이 0건, 검사 변조 탐지, 필수 검사를 확인한다.
4. 새 설치 후보의 검증과 별개로 기존 개발 데이터 보존·이관 및 백업/SQLx 이력 전환 방식을 결정한다. 기존 DB를 자동 초기화하지 않는다.
5. 별도 구현 PR에서만 실제 마이그레이션 경로 교체와 설치 문서 전환을 검토한다. 이 Draft의 병합·태그·배포는 이번 작업에 포함하지 않는다.

실행 결과와 CodeRabbit 처리 상태는 Draft PR 및 `verification/` 근거에 갱신한다.
