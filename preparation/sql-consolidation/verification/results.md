# SQL 통합 준비 검증 결과

2026-10-22에 누적 마이그레이션 A와 새 설치 후보 B를 서로 다른 빈 DB에 적용했다. PostgreSQL 18.3과 Supabase PostgreSQL 17.6의 객체 비교 및 양쪽 행동 시험은 모두 PASS다. Supabase는 direct와 session pooler 경로를 각각 확인했다. 이는 새 설치 동등성 근거이며 실제 마이그레이션 파일·제품 설치 경로를 바꾸지 않는다.

## 입력과 재현 명령

| 입력                                | 값                                                                    |
| ----------------------------------- | --------------------------------------------------------------------- |
| 누적 마이그레이션 기준              | `cb65cc31156c0fd8d28cf856bc132d4a506c8885`, 93개 파일, 마지막 버전 94 |
| 최종 A/B 실행의 도구 소스           | `efdaa5a7ffa1248ed3587b46bcbe8f8927606bd7`                            |
| `sql_compare.rs` SHA-256            | `aafcc111df3f62792abf6faa2f4570e9a25d3bd8ae8aa9fd7034ed996444330b`    |
| `catalog.sql` SHA-256               | `e0f8ee937a1e12fbf2d87ccd53a4bb2ae132eb7e7b3ba2bdecfc7ce110f26c3e`    |
| 후보 `0001_final_state.sql` SHA-256 | `b8172cfb85f20145e5a9dd2fb65d473d47195549a21024de1ec8c11af2c55725`    |
| 셀프호스트 Supabase 고정 revision   | `bc27d2661edd09a5d0b8d76ce33da6d4071a756e`                            |

실제 작업 디렉터리는 `/home/kinesis/orca/workspaces/GroundZero/sql-prep-b`였다. 실행한 명령은 다음과 같다. 재실행 시 `--output`은 아직 존재하지 않는 경로로 바꾼다. 합친 체크아웃에서는 `--candidate preparation/sql-consolidation/candidate`를 사용하면 된다.

```sh
cargo xtask sql-compare --output target/sql-compare-aa-final

cargo xtask sql-compare \
  --candidate /home/kinesis/orca/workspaces/GroundZero/sql-prep-a/preparation/sql-consolidation/candidate \
  --output target/sql-compare-postgres-final --behavior

cargo xtask sql-compare \
  --candidate /home/kinesis/orca/workspaces/GroundZero/sql-prep-a/preparation/sql-consolidation/candidate \
  --output target/sql-compare-supabase-final --supabase --behavior
```

A/A 대조군은 도구 소스 `63c1483167ad0d2fb0375080604887518674d7f9`에서 실행했다. 최종 A/B 소스와 카탈로그·정규화는 같으며, 이후 수정은 행동 시험의 일반 서비스 환경 변수를 격리 주소로 고정한 것이다. A/A에는 행동 시험을 요청하지 않았다. 파일별 누적 SQL 해시와 실행 ID는 [대조군](runs/control/inputs.json), [PostgreSQL](runs/postgres/inputs.json), [Supabase](runs/supabase/inputs.json)의 입력 기록에 있다.

## 객체 비교

| 실행                         | A/B 설치·스냅숏 | 의미 카탈로그 차이 | 정규 덤프 차이 | 변조 검출·원복 |
| ---------------------------- | --------------- | ------------------ | -------------- | -------------- |
| PostgreSQL 18.3 A/A          | PASS            | 0                  | 0              | 4/4 PASS       |
| PostgreSQL 18.3 A/B          | PASS            | 0                  | 0              | 4/4 PASS       |
| Supabase PostgreSQL 17.6 A/B | PASS            | 0                  | 0              | 4/4 PASS       |

각 비교는 행동 시험이 데이터를 넣기 전에 수행했다. 앱의 55개 테이블 초기 행은 모두 비어 있었으며 양쪽이 같았다. 함수 정의·소유권·ACL·함수 설정, RLS 정책, FK 내부 트리거 상태, 기본 권한, 역할 속성과 멤버십을 포함한다. 상세 범위와 제한은 [비교 설명](README.md#객체-비교)에 있다. 결과 JSON은 [대조군](runs/control/result.json), [PostgreSQL](runs/postgres/result.json), [Supabase](runs/supabase/result.json), 실제 빈 차이 목록은 각각의 `differences.json`에 있다.

정책 추가는 `policy/fvoci.documents.sql_compare_probe`, DELETE 권한 추가는 `relation/fvoci.sessions`, 함수 변경은 `function/fvoci.app_user_id()`, CHECK 완화는 `constraint/fvoci.files.files_name_check` 차이로 검출됐다. 네 변조를 하나씩 적용했고 각 ROLLBACK 후 원래 전체 스냅숏으로 돌아왔다. 변조 전후 값은 [PostgreSQL 증거](runs/postgres/self-test.json)와 [Supabase 증거](runs/supabase/self-test.json)에 있다.

PostgreSQL 18.3과 Supabase 17.6의 실제 버전 문자열은 각 `a-environment.json`, `b-environment.json`에 있다. 확장 버전·역할·멤버십·기본 ACL·스키마 권한은 [PostgreSQL 환경 카탈로그](runs/postgres/catalog-environment.json), [Supabase 환경 카탈로그](runs/supabase/catalog-environment.json)에 보존했다. 두 이미지 모두 `pg_catalog.uuid_extract_version(uuid)`를 제공했다. Supabase 쪽은 `supabase_admin` 소유, `LANGUAGE internal`, IMMUTABLE/PARALLEL SAFE/STRICT/LEAKPROOF이고 PUBLIC EXECUTE를 가진다. 후보가 별도 UUID adapter나 생성 함수를 만들지 않았다.

SQLx 이력 테이블 구조도 비교했다. 이력 행은 A가 성공 93건·최대 버전 94, B가 실제 후보 적용 성공 1건·버전 1이다. 이는 명시적인 차이로 [PostgreSQL 이력 요약](runs/postgres/migration-ledger-summary.json), [Supabase 이력 요약](runs/supabase/migration-ledger-summary.json)에 분리했다. 과거 성공 행을 위조하지 않았다.

## 앱 역할 행동 시험

| 환경·연결 경로               | A    | B    | 경로별 Rust 결과                             |
| ---------------------------- | ---- | ---- | -------------------------------------------- |
| PostgreSQL 18.3 direct       | PASS | PASS | 353 passed, 0 failed, 0 ignored, 17 filtered |
| Supabase 17.6 direct         | PASS | PASS | 364 passed, 0 failed, 0 ignored, 6 filtered  |
| Supabase 17.6 session pooler | PASS | PASS | 364 passed, 0 failed, 0 ignored, 6 filtered  |

숫자는 각 `cargo test --workspace --locked` 실행의 단위·통합·문서 시험 전체를 합산한 값이며 API 시험만의 숫자가 아니다. 경로당 47개의 `test result` 그룹이다. 전체 시험명과 결과는 [PG A](runs/postgres/a-direct.txt), [PG B](runs/postgres/b-direct.txt), [Supabase A direct](runs/supabase/a-direct.txt), [A session](runs/supabase/a-session.txt), [B direct](runs/supabase/b-direct.txt), [B session](runs/supabase/b-session.txt)에 있다.

기존 시험의 CRUD, `rls_hides_rows_from_other_users_under_app_role`, `user_context_does_not_outlive_commit_or_rollback`, 개인 문서·시간 기록, 공유 철회, stale save·멤버십·이동 경합과 409, 반복 일정, DST·타이머, 검색 접근 제어 및 백업 경로를 실행했다. 별도 축약 SQL 시험으로 대체하지 않았다.

Session 단계의 일반 앱 연결과 `db_context`는 실제 Supavisor session pooler를 사용한다. **새 DB를 만드는 `FreshDb` 기반 백업·검색 시험은 기존 규칙대로 direct 연결을 사용한다.** 따라서 전체 364건이 pooler를 통과했다는 뜻은 아니다. OIDC Keycloak 6건은 모든 경로에서 NOT RUN이며 PostgreSQL direct에서는 Supabase 전용 11건도 NOT RUN이다. Supabase Cloud 및 transaction pooler는 실행하지 않았다.

## 발견한 차이와 수정

1. 초기 후보는 빈 `search_path`를 유지해 SQLx의 이력 INSERT가 실패했다. 후보 마지막에 `SET LOCAL search_path = fvoci`를 두어 해결했다. [원래 실패 결과](diagnostics/candidate-search-path-result.json)를 보존했다.
2. 최초 Supabase 덤프는 로컬 소켓 peer 인증 때문에 실패했다. 동일 컨테이너의 TCP로 migrator 인증을 수행하도록 도구를 고쳤다. 권한 확대 없이 최종 양쪽 스냅숏이 통과했다. [원래 실패 결과](diagnostics/supabase-peer-auth-result.json)를 보존했다.
3. 최초 A/B 카탈로그 차이는 0건이었으나 `files_name_check`의 덤프에서 아래 AND 묶음만 달랐다. [원래 실패 결과](diagnostics/check-rendering-result.json)와 [전체 테이블 블록](diagnostics/check-rendering-difference.json)을 보존했다.

```sql
-- 누적 마이그레이션 A
CHECK ((((length(btrim(name)) >= 1) AND (length(btrim(name)) <= 255)) AND (name !~ '[\x00-\x1f/\\]'::text)))
-- 후보를 적용한 B
CHECK (((length(btrim(name)) >= 1) AND (length(btrim(name)) <= 255) AND (name !~ '[\x00-\x1f/\\]'::text)))
-- 양쪽의 pg_get_constraintdef(oid, true)
CHECK (length(btrim(name)) >= 1 AND length(btrim(name)) <= 255 AND name !~ '[\x00-\x1f/\\]'::text)
```

정확한 TABLE 블록과 CHECK 이름에 대응하는 한 줄만 카탈로그 표현으로 바꾸었다. 일반적인 괄호 제거·CHECK 제외는 하지 않았다. 이후 실제 `CHECK (true)` 변조를 검출하고 원복하는 시험도 통과했다.

4. 일반 저장소·검색 환경 변수가 외부에서 상속될 여지를 검토 후 고정했다. 이전 실행의 실제 관련 환경 변수는 [값 없이 기록한 점검](diagnostics/inherited-environment.txt)에서 모두 unset이었다. 최종 A/B는 수정한 도구로 다시 실행했으며 기본 저장소 local, 검색·S3는 이 실행의 격리 서비스였다. 이전 실행의 성공을 최종 행위 근거로 재사용하지 않았다.

## 후속 정리 실패 경로 검증

코드 검토 후 정리 전용 수정 `60e56dbbff9c13e1935b0ee47692fc5214171ca5`를 추가했다. DB·서비스를 시작 전에 등록해 부분 시작 실패도 정지 대상으로 유지한다. 정지 실패가 생겨도 나머지 자원을 모두 시도하고 `result.json`에 `cleanup: FAIL`, `passed: false`를 기록한다. 원래 작업 오류가 있으면 보존하며, 없으면 첫 정지 오류를 반환한다.

`cargo test -p xtask sql_compare::tests --locked`는 3/3 PASS다. DB·서비스 시작 실패의 등록 유지, 연속된 두 정지 실패 후 나머지 자원 시도와 실패 결과 파일 기록, 작업·정지·기록이 함께 실패했을 때 원래 오류 보존을 확인했다. `cargo clippy -p xtask --all-targets --locked -- -D warnings`도 PASS다.

수정한 소스로 `cargo xtask sql-compare --output target/sql-compare-cleanup-aa`를 실행해 A/A 객체 차이 0, 변조 검출·원복 4/4, `cleanup: PASS`를 확인했다. [실행 입력](runs/cleanup-control/inputs.json), [결과](runs/cleanup-control/result.json), [정지 로그](runs/cleanup-control/orchestration.txt)를 보존했다. 위의 여섯 행동 경로는 표에 명시한 기존 소스의 결과이며, 자원 수명 관리만 바꾼 이 수정 후 다시 실행하지 않았다.

## 후속 출력 경로 경합 검증

출력 경로 존재 확인과 디렉터리 생성 사이의 경합도 수정했다. 부모 경로만 `create_dir_all`로 준비한 뒤 출력 디렉터리 자체는 원자적인 `create_dir`로 생성한다. 이미 존재하면 파일 기록이나 Docker 시작 전에 거부한다.

8개 스레드가 동일한 새 출력 경로를 동시에 요청하는 시험에서 하나만 성공하고 나머지는 `AlreadyExists`로 실패했다. 이후 같은 경로에 다시 요청해도 승자가 기록한 증거가 보존됨을 확인했다. `cargo test -p xtask sql_compare::tests --locked` 4/4 PASS, `cargo clippy -p xtask --all-targets --locked -- -D warnings` PASS다. 출력 준비 단계만 바꿨으므로 위의 SQL·행동 매트릭스는 다시 실행하지 않았다.

## 실행 자원과 보존

첫 A/A 진단에는 보존 요청과 어긋난 정리 명령이 한 번 실행됐다. 수정 전 바이너리가 다음 전용 프로젝트의 B PostgreSQL 컨테이너와 네트워크를 삭제했다.

```sh
docker compose --project-name sqlprep-b-fvoci-sql-prep-b-28dc62-test-01a163c19209705884cf864b2cc4d6f8-b --file deploy/compose.test.yaml down --volumes --remove-orphans
```

[원본 전체 로그](diagnostics/initial-control-cleanup.txt) 31행이 명령이며 34–37행이 이 컨테이너와 네트워크의 제거 기록이다. 이 로그의 생성 시각은 `2026-10-22T22:16:22.128483746+09:00`, 마지막 수정 시각은 `2026-10-22T22:16:46.428293994+09:00`이다. 로그에 행별 시각이 없고 Docker event가 남아 있지 않아 이 범위보다 정확한 제거 시각은 확인하지 못했다. [유효 Compose 저장소 구성](diagnostics/initial-control-postgres-storage.json)은 `volumes: null`, `tmpfs: ["/var/lib/postgresql"]`이다. 이 프로젝트에는 이름 붙은 DB 볼륨이 없었으며 공유 컨테이너·DB·볼륨은 대상이 아니었다.

이후 도구는 해당 실행 프로젝트에 `stop`만 사용하도록 수정했다. 진단 실행·수동 baseline export·최종 실행 종료 시 남은 전용 컨테이너는 37개였고, 후속 A/A를 포함한 39개 모두 exited다. 이름 붙은 볼륨 5개는 남아 있다. 자원 상태와 로그 감사 결과는 [컨테이너 처리 기록](container-disposition.txt)에 있다. 공유 개발 자원에는 정리 명령을 내리지 않았다. 중지한 DB의 tmpfs 내용은 사라지므로 DB 자체를 보존했다고 주장하지 않는다.

PR에 보존한 것은 입력 해시, 결과·차이·변조 JSON, 환경 카탈로그, 전체 행동 로그, 각 실행의 `orchestration.txt` 및 위 진단 증거다. 텍스트 로그는 줄 끝 공백과 마지막 빈 줄만 정리했으며 내용과 명령은 보존했다. 가공 전 로컬 실행 로그 해시는 [원본 로그 해시](diagnostics/original-orchestration-logs.sha256)에 있다. 중복된 전체 원본 카탈로그·덤프·이력은 이 작업 공간의 `target/sql-compare-postgres-final/{a,b}/`, `target/sql-compare-supabase-final/{a,b}/`에만 남겼다. 경로와 SHA-256은 각 실행의 `local-artifacts.sha256`에 있으며 **이 로컬 파일 자체는 PR에 포함되지 않는다**. 최초 baseline export는 로컬 `target/sql-compare-baseline-export/`에 있다.

기존 DB에서 누적 이력을 후보 버전으로 바꾸는 업그레이드, 과거 버전 백업 복구, 실제 운영 데이터, Cloud 환경의 실행은 검증하지 않았다. 특히 버전 94와 후보 버전 1의 백업 호환성은 새 설치 시험 결과로 판단할 수 없다.
