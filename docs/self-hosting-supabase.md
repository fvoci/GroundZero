# 셀프호스트 Supabase를 DB로 쓰기

GroundZero는 기본으로 자체 PostgreSQL(`deploy/compose.selfhost.yaml`, PostgreSQL 18)을 쓴다. 이미 셀프호스트 Supabase를 운영한다면
그 PostgreSQL을 GroundZero의 DB로 쓸 수 있다(D-88). 이 문서는 **검증한 셀프호스트 Supabase 구성**만 다룬다. Supabase Cloud는
확인하지 않았다(아래 7절).

GroundZero는 Supabase를 **PostgreSQL로만** 쓴다. 로그인은 그대로 OIDC(Keycloak 등)이고, 파일은 S3/R2/로컬, 검색은 Meilisearch,
작업 대기열은 GroundZero 자신의 PostgreSQL 표다. Supabase Auth·Storage·Realtime·Data API(PostgREST)와는 연결하지 않으며 화면이 DB에
직접 붙지 않는다.

## 1. 검증한 조합

| 항목          | 값                                                                                                                      |
| ------------- | ----------------------------------------------------------------------------------------------------------------------- |
| 공식 구성     | `supabase/supabase` 저장소 `docker/`, revision `bc27d2661edd09a5d0b8d76ce33da6d4071a756e`(2026-10-16)                   |
| DB 이미지     | `supabase/postgres:17.6.1.136@sha256:f371b5f3f2ac0a05703f33d6e6134515fb2498cab708fb948a0aeb7481467c00`(PostgreSQL 17.6) |
| Data API      | `postgrest/postgrest:v14.18@sha256:c7cd7e265a85b05248abf53f4473fcec7e327c282d0bd0cf60f1551996d09fcf`                    |
| 연결 풀러     | `supabase/supavisor:2.9.13@sha256:07f1a6098ffc04b80263bca4eb5c3e7bd2e03dfd1fc465aa108d7329000a1a4e`                     |
| 런타임 연결   | db 직접 연결(권장), Supavisor **session** 모드(5432)                                                                    |
| 지원하지 않음 | Supavisor transaction 모드(6543), Supabase Cloud, 공식 구성의 PG15 override(`docker-compose.pg15.yml`)                  |

다른 revision·이미지는 확인하지 않은 조합이다. 올리기 전에 아래 6절의 시험을 그 조합으로 돌린다.

## 2. 역할과 권한

- `fvoci-server db bootstrap`이 Supabase의 `postgres` 역할(superuser 아님, `CREATEROLE`·`CREATEDB`·`BYPASSRLS`)로 두 역할을 만든다.
  - `fvoci_migrator`: 스키마 `fvoci`의 소유자, 마이그레이션·백업용
  - `fvoci_app`: 서버 런타임, `NOSUPERUSER NOCREATEROLE NOCREATEDB NOBYPASSRLS`
- 다시 실행해도 같다. 이미 있는 역할은 비밀번호만 맞추고 속성은 확인만 한다. 권한 속성이 붙어 있으면 멈춘다. 스키마 소유자를 정하는
  동안만 `postgres`가 `fvoci_migrator`의 상속 없는 SET 멤버십을 받고, 끝나면 거둔다.
- 런타임은 `fvoci_app`만 쓴다. 스키마 소유자·superuser·`BYPASSRLS` 역할로 서버를 돌리지 않는다. Supabase의 `postgres`도
  `BYPASSRLS`이므로 서버 연결에 쓰지 않는다.
- RLS는 GroundZero의 트랜잭션 설정 `app.user_id`를 본다. Supabase `auth.uid()` 정책을 쓰지 않는다.
- `fvoci` 스키마를 Data API에 드러내지 않는다: `PGRST_DB_SCHEMAS`에 `fvoci`를 넣지 않는다. `anon`·`authenticated`·`service_role`·
  `authenticator`에 fvoci 권한을 주지 않는다. GroundZero는 이 역할들에 아무 권한도 주지 않는다(시험이 확인한다).

## 3. 연결

`deploy/supabase/app.env.example`에 비밀 값 없는 예시가 있다. 비밀 값은 운영자 `.env`에만 둔다.

- `DATABASE_ADMIN_URL`(bootstrap·복구), `DATABASE_MIGRATION_URL`(마이그레이션·백업)은 **db에 직접** 연결한다.
- `DATABASE_URL`(서버)은 db 직접 연결을 권한다. Supavisor session 모드를 쓰면 사용자 이름이 `fvoci_app.<POOLER_TENANT_ID>`가 되고
  포트는 공식 구성의 5432다. session 모드 테넌트는 데이터베이스 하나(`postgres`)만 받는다. session 모드는 클라이언트 연결마다
  서버 연결을 잡으므로 `POOLER_DEFAULT_POOL_SIZE`를 GroundZero 프로세스 수 × 16(서버 한 프로세스의 연결 풀 최대)보다 크게 둔다.
  모자라면 연결을 기다리다 실패한다(시험에서 기본값 20으로 동시 요청이 `XX000`으로 실패했다).
- Supavisor 이미지는 시작할 때 열린 파일 한도를 `RLIMIT_NOFILE`(기본 100000)으로 올린다. 컨테이너 한도가 그보다 낮은
  호스트(예: GitHub 러너의 Docker 기본값)에서는 시작 스크립트가 실패해 재시작만 되풀이한다. 공식 구성에 `ulimits: nofile`을 그만큼
  주거나 `RLIMIT_NOFILE`을 호스트가 허락하는 값으로 둔다(시험 구성은 `ulimits` 100000).
- transaction 모드(6543)는 쓰지 않는다. 서버는 트랜잭션마다 `app.user_id`를 트랜잭션 범위로 설정하지만, 백업·작업 프로세스는
  세션 advisory lock을 쓰고 sqlx는 준비한 문장을 연결에 남긴다.
- 공식 구성은 db 포트를 호스트에 열지 않는다. GroundZero 앱을 Supabase의 Docker 네트워크에 붙이거나 db 포트를 내부망에만 연다.
  인터넷에 열지 않는다.
- 네트워크를 건너면 TLS를 켜고 `sslmode=verify-full`(인증서 검증)로 연결한다. 검증을 끄는 방식으로 오류를 피하지 않는다.

## 4. 설치·올리기

1. Supabase가 돌고 있는 상태에서 GroundZero 이미지(또는 바이너리)로 차례로 실행한다.

   ```bash
   fvoci-server db bootstrap
   ```

   ```bash
   fvoci-server db migrate
   ```

2. 서버를 `DATABASE_URL`(앱 역할)로 띄운다. 나머지 설정은 [셀프호스트 문서](self-hosting.md)와 같다.
3. 올릴 때는 백업(5절)을 만든 뒤 새 릴리스로 `db migrate`를 실행하고 서버를 바꾼다. 마이그레이션은 되돌리지 않는다.

이 문서의 Compose 예시(`deploy/compose.selfhost.yaml`)는 자체 PostgreSQL용이다. Supabase와 함께 쓰는 Compose 구성은 아직
제공하지 않는다(앱 컨테이너를 Supabase 네트워크에 붙이는 방법은 운영 환경마다 다르다).

## 5. 백업과 복구

- 백업: `fvoci-server backup create <폴더>`(`DATABASE_MIGRATION_URL`). 한 세션에서 저장소 잠금(advisory lock)을 잡고 같은
  세션에서 푼다.
- 복구: **새로 만든 빈 Supabase**에 같은 릴리스로 bootstrap·migrate를 한 뒤 `fvoci-server backup restore <폴더>`
  (`DATABASE_ADMIN_URL`=`postgres`).
  - 표 읽기·채우기는 표 소유자(`fvoci_migrator`)로 한다.
  - `postgres`의 권한은 트리거를 잠시 끄는 `session_replication_role`에만 쓴다. 공식 이미지의 supautils가 `postgres`에게 이것을 허락한다.
- Supabase 자체의 백업(pg_dump·WAL-G 등)은 Supabase 운영 방식대로 따로 한다. GroundZero 백업은 첨부 파일까지 함께 맞춘다.

## 6. 시험

저장소의 `cargo xtask test --supabase`가 위 공식 구성을 고정 revision으로 `target/supabase/`에 받는다. 이어서
`deploy/supabase/compose.test.yaml`을 겹쳐 db·rest·supavisor만 띄운다.

- 포트는 127.0.0.1의 빈 포트이고 데이터는 tmpfs다.
- 기존 개발·시험 환경과 다른 Compose 프로젝트로 돈다.
- 관리자 `postgres`로 bootstrap·migrate를 한 뒤 전체 통합 테스트를 직접 연결로 돌린다.
- `--session-pooler`를 주면 앱 역할 연결을 Supavisor session 모드로 바꿔 한 번 더 돌린다.

빼는 공식 서비스와 이유는 다음과 같다. GroundZero가 쓰지 않으며, 빼도 DB 초기화(역할·스키마·확장)는 db 이미지와 공식 초기화 SQL이
그대로 한다.

| 서비스                                  | 빼는 이유                          |
| --------------------------------------- | ---------------------------------- |
| studio, kong/envoy                      | 관리 화면, API 관문                |
| auth(GoTrue)                            | 로그인은 OIDC                      |
| storage, imgproxy                       | 파일은 S3/R2/로컬                  |
| realtime                                | 공동 편집은 GroundZero 서버가 한다 |
| meta, functions, analytics/vector(로그) | 쓰지 않음                          |

## 7. Supabase Cloud(확인하지 않음)

Cloud는 이번에 시험하지 않았다. 옮기기 전에 따로 확인해야 하는 것:

- `postgres` 역할의 `CREATEROLE`과, 역할을 만든 사람에게 주는 권한(`createrole_self_grant`)이 셀프호스트와 같은지
- supautils가 `session_replication_role`을 허락하는지(복구)
- 직접 연결 주소(IPv6 전용 여부)와 풀러 주소·사용자 이름 규칙, TLS 인증서
- 여러 데이터베이스를 만들 수 있는지(시험은 같은 클러스터에 빈 DB를 만든다)
- 플랫폼이 자동으로 주는 권한·확장·이벤트 트리거가 fvoci 스키마에 닿지 않는지

## 8. 이전 설치에서 올릴 때

D-88에서 id 생성을 DB에서 앱으로 옮기며 이미 적용된 마이그레이션 일부를 고쳤다. 실제 배포 전 결정이다. 그 전에 만든 DB에서는
`db migrate`가 checksum 불일치로 멈추므로 새로 설치해야 한다. 같은 마이그레이션 버전으로만 복구되므로, 이전 판의 백업을 새 판에
복구할 수도 없다.
