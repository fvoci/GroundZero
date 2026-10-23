# SQL 통합 준비: 세션·역할·풀 반환

> **역사 기록이며 현재 상태를 나타내지 않음.** 본문 판단·상태·결과는 명시된 조사/실행 SHA에만 적용한다. 관련 결함이나 SHA를 조사할 때만 참조하며, 현재 기준과 분리 검증은 [증거 안내](../evidence.md)를 따른다.

기준 SHA와 환경 분류는 [environment.md](../environment.md)를 따른다. 이 문서는 정적 소스 검토이며 새 실행 결과가 아니다.

## 두 풀의 수명을 구분한다

| 경계                                          | 실제 구현/계약                                                                                                                  | 확인 범위                                                                                                                   |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| HTTP 요청 → SQLx 트랜잭션                     | `Db::begin_as`가 `set_config('app.user_id', ..., true)` 실행. 익명 요청은 별도 `begin_anonymous`                                | `crates/server/src/db/mod.rs:81–95`. true는 트랜잭션 범위이며 commit/rollback으로 사라짐                                    |
| 트랜잭션 → SQLx 앱 풀                         | 앱 풀 최대 16, acquire timeout 5초. `after_connect`, `before_acquire`, `after_release`, `RESET ROLE`, `DISCARD ALL` hook 없음   | `db/mod.rs:67–71`. 기존 `UserTx` drop의 SQLx rollback은 문맥 해제를 제공하지만 임의 세션 설정 전체를 초기화하는 hook은 아님 |
| SQLx 물리 연결 → Supavisor session            | 앱 풀에 반환해도 TCP 연결은 살아 있음. Supavisor는 같은 클라이언트에 할당한 DB 연결을 유지                                      | 아래 공식 상위 소스에서 확인. 요청마다 Supavisor backend가 교체된다고 가정하지 않음                                         |
| Supavisor 클라이언트 연결 종료 → backend pool | v2.9.13은 조건에 맞는 idle session client 종료 때 `DISCARD ALL`을 시도하고 성공 시 반환. 그 밖의 종료 경로에서는 backend도 종료 | 이는 SQLx 풀 반환 시점의 정리가 아님. Cloud가 같은 빌드·조건이라는 실행 증거도 아님                                         |

PostgreSQL의 `SET LOCAL`은 트랜잭션 종료 시 해제되며 `SET SESSION`은 남을 수 있다.
`SET ROLE`은 `current_user`의 권한을 바꾸지만 역할에 설정한 `search_path`를 로그인처럼 다시 적용하지 않는다.
`RESET ROLE`도 연결 시 role 설정이 있으면 그것으로 돌아간다. 따라서 관리자 로그인 후 `SET ROLE fvoci_app`만
한 경우를 제품의 실제 앱 로그인과 동일하게 시험하지 않는다.
공식 근거: [PG17 SET](https://www.postgresql.org/docs/17/sql-set.html),
[PG17 SET ROLE](https://www.postgresql.org/docs/17/sql-set-role.html).

SQLx 0.8.6의 풀 반환은 설정된 `after_release`와 연결 ping을 거쳐 반환하거나 닫는다. ping은 대기 중인 rollback도
처리하지만 자동 `DISCARD ALL`이 아니다. 열린 트랜잭션은 drop 시 rollback한다.
공식 근거: [SQLx pool connection:275–327](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-core/src/pool/connection.rs#L275),
[SQLx Transaction](https://docs.rs/sqlx/0.8.6/sqlx/struct.Transaction.html).

Supavisor v2.9.13 태그는 2026-10-22 조회 시 `5a51fbfadaa6d6286c1fee0a95d4c629e81a2750`이었다.
session은 쿼리 종료 후에도 연결을 보유한다. idle client 종료 시 warm-pool 조건을 만족하면 cleanup을 요청하고
성공한 backend만 반납한다. cleanup은 `DISCARD ALL`을 보내며 연결 종료/실패 경로는 따로 존재한다.
이 상위 소스 검토는 고정 이미지 내부 바이너리 대조나 Cloud 실행 증거는 아니다.
공식 근거: [client 보유:873–883](https://github.com/supabase/supavisor/blob/5a51fbfadaa6d6286c1fee0a95d4c629e81a2750/lib/supavisor/client_handler.ex#L873),
[idle 종료 cleanup:759–777](https://github.com/supabase/supavisor/blob/5a51fbfadaa6d6286c1fee0a95d4c629e81a2750/lib/supavisor/client_handler.ex#L759),
[DB cleanup:524–548](https://github.com/supabase/supavisor/blob/5a51fbfadaa6d6286c1fee0a95d4c629e81a2750/lib/supavisor/db_handler.ex#L524),
[backend 종료:568–584](https://github.com/supabase/supavisor/blob/5a51fbfadaa6d6286c1fee0a95d4c629e81a2750/lib/supavisor/db_handler.ex#L568).

## 앱·관리 명령의 역할 전환

앱은 `DATABASE_URL`의 앱 역할로 로그인하며 역할 전환 SQL이 없다. 사용자는 DB 로그인 역할로 바꾸지 않고
트랜잭션의 `app.user_id`로 구분한다. 정의자 함수는 별도로 소유자의 권한을 사용한다.
bootstrap의 임시 SET 멤버십 확보는 스키마 소유자 지정 권한을 위한 것이고, 앱 런타임의 전환 방식이 아니다.

복구는 단일 연결에서 `SET ROLE fvoci_migrator`, 데이터 채우기 트랜잭션에서 잠깐 `SET LOCAL ROLE NONE`과
`SET LOCAL session_replication_role`, 다시 migrator 전환을 한다. 끝에서 `RESET ROLE`과 임시 멤버십 복원을 수행하며
연결이 끊겼으면 새 관리자 연결로 멤버십 복원을 한 번 더 시도한다
(`crates/server/src/backup.rs:257–282,308–316,479–490`, `db/admin.rs:179–237`).

마이그레이션도 세션 상태와 무관한 명령이 아니다. SQLx 0.8.6은 DB 이름으로 정한 세션 advisory lock을 잡고
같은 연결에서 해제한다([공식 migrate 소스:170–202](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-postgres/src/migrate.rs#L170)).
백업은 명시적인 단일 연결에서 저장소 advisory lock을 잡고 푼다(`backup.rs:136–151`).
따라서 관리자·migrator URL의 session pooler 사용을 기존 앱 session 시험만으로 승인하지 않는다.

transaction pooler는 현재 지원하지 않는다. 제품 SQLx 연결은 prepared statement를 유지하며, session 상태와
advisory lock의 연결 수명도 고려해야 한다. 단순히 statement cache를 끄는 것으로 지원을 선언하지 않는다.
Supabase 공식 연결 안내도 session 5432와 transaction 6543을 구분하고 transaction 모드의 prepared statement 제한을
명시한다([공식 연결 안내](https://supabase.com/docs/guides/database/connecting-to-postgres)).

## 기존 시험과 통합 후보의 추가 확인

`tests/db_context.rs:23–56`은 사용자 문맥 commit/drop rollback 뒤 익명 트랜잭션을 40회 열어 문맥 NULL·문서 0행을
확인한다. 이는 정상 요청 경계의 회귀 시험이다. 실제 물리 연결 ID 일치, SQL 오류/취소, 역할·search_path 오염,
Supavisor 연결 종료 후 다른 클라이언트에 반환된 backend까지 직접 단언하지는 않는다.

통합 후보에서 B와 조율할 최소 확인은 다음과 같다. 모두 전용 격리 환경에서 계획/실행 상태를 구분한다.

1. 최대 연결 1인 검증 풀에서 backend PID를 기록하고 A commit, B rollback, SQL 오류 후 rollback, 익명 요청을 반복한다.
   각 단계에서 앱 DB 역할·`app_user_id()`·실제 읽을 수 있는 행을 단언한다. 두 사용자의 서로 다른 문서를 쓴다.
2. 별도 검증 연결에서 `SET LOCAL ROLE`의 commit/rollback 원상복구를 확인한다. 앱의 migrator SET은 거부되어야 한다.
   session 범위 설정을 고의로 넣는 진단은 제품이 자동 제거한다고 기대하지 말고 진단 연결을 반드시 닫는다.
3. SQLx 풀 안 재사용과 풀을 완전히 닫은 뒤 새 클라이언트의 Supavisor backend 재사용을 따로 확인한다. 후자는 backend
   PID가 같은 경우만 재사용 증거이며 PID가 다르면 “새 연결 정상”까지만 기록한다.
4. 실제 session 경유 시험과 직접 연결 우회를 결과에 표시한다. `xtask/src/test.rs:164–170` 및
   `tests/common/mod.rs:309–325` 때문에 fresh DB를 만드는 백업·검색 등의 시험은 session 단계에서도 직접 연결이다.
5. 취소/끊김 후 연결이 안전하게 폐기되거나 rollback된 뒤 재사용되는지 확인하고, 예상 실패가 다음 정상 요청을 오염시키지
   않는지 확인한다. 실제 장애 주입 없이 소스만 읽은 항목은 “검증 완료”로 적지 않는다.
