# 재생성 대상 읽기 전용 조사

조사: 2026-10-23 12:34–12:43 UTC, 소스 `8ffd8c3b940924eca8cf7ec739a85494243c0479`.
이 조사에서는 기존 데이터·컨테이너·볼륨·Cloud 객체를 변경하지 않았다.

## 후속 최종 관측

초기 관측 아래의 Cloud 미확인·로컬 개발 자원 없음은 조사 시작 시점의 상태다.
이후 동일 작업에서 다음이 추가로 확인됐다.

- Cloud 읽기 전용 inventory: <https://github.com/fvoci/GroundZero/actions/runs/39956303784>.
  결과 정본은 `target/sql-consolidation/cloud-inventory/report.json`이다.
  인증된 session pooler route의 실제 프로젝트 참조는 `fpbqhckdkhhnhqvsixnr`이며 과거 참조와 일치한다.
  호스트 `aws-0-ap-southeast-1.pooler.supabase.com`, 포트 `5432`, 실제 `current_database()`는
  `postgres`, 서버 `17.11` / `170011`, session/current role `postgres`이다.
  TLS, 연결, REPEATABLE READ READ ONLY 조회, ROLLBACK, 종료가 PASS이며 changes는 NOT RUN이다.
- 이 Cloud에는 `fvoci` 스키마, `fvoci_app`/`fvoci_migrator` 역할, GroundZero 객체와 SQLx 이력이
  없다. 조회한 GroundZero 관계·함수·타입·기본권한·외부 의존성·행수 목록도 비어 있다.
  현재 삭제할 기존 GroundZero 객체/데이터/이력은 0개로 관측됐다. 실제 새 설치에는 기존에 없는 제품 역할과
  그 로그인 자격 증명이 필요하므로, 사용자 별도 확인 없이 이를 새로 만들지 않는다.
- Cloud에는 `auth`, `extensions`, `graphql`, `graphql_public`, `pgbouncer`, `public`, `realtime`,
  `storage`, `vault` 관리 스키마가 있으며 모두 보존한다. 다른 서비스와의 공유는 NOT DETERMINED,
  Supabase 관리 백업·보존기간·복구가능성은 NOT VERIFIED다. 기존 객체가 없다는 이유로 백업 생략을 승인받았다고 해석하지 않는다.
- 로컬에는 새 빈 개발 DB 검증을 위해 `fvoci-leatherback-b57275-dev` 프로젝트를 만들었다.
  DB는 `fvoci`, PostgreSQL 18.3, SQLx 이력 `[202610230001]`이다. 기존 DB 재생성이 아니다.
  생성 컨테이너는 `fvoci-leatherback-b57275-dev-postgres-1`, `fvoci-leatherback-b57275-dev-keycloak-1`,
  `fvoci-leatherback-b57275-dev-meilisearch-1`이다. 연결 볼륨은 각각 프로젝트의 `_pgdata`, `_kcdata`,
  `_meilidata`이며 다른 컨테이너와의 공유 없이 이 프로젝트가 사용한다.
  새 합성 사용자 2명·워크스페이스 2개·모듈 5개·항목 5개·업무 22개·seed 문서 10개를 생성했다.
  브라우저 검증으로 예제 문서 1개를 추가했다. 원시 실행 증거는 `target/sql-consolidation/browser/`에 있다.
- 기존 자원 삭제·초기화는 0건이다. 새 개발 DB·Keycloak 계정·검색 데이터·볼륨은 검증 뒤에도 보존한다.
  소유 불명 기존 볼륨과 다른 checkout 자원은 전혀 변경하지 않았다.

## 현재 관측

- 실행 호스트: `DESKTOP-BLN60K6`, Linux `6.18.33.2-microsoft-standard-WSL2`.
- Orca 실행 호스트: `local` 하나. 현재 checkout:
  `/home/kinesis/orca/workspaces/GroundZero/leatherback`, branch `fvoci/leatherback`.
- Docker context: `default`, endpoint `unix:///var/run/docker.sock`, daemon name `DESKTOP-BLN60K6`.
- 조사 시작 시 Docker 컨테이너는 정지 상태를 포함해 0개였으며, 네트워크는 기본 `bridge/host/none`만 있었다.
  이후 이번 작업의 격리 시험이 생성하는 자원과 구분한다.
- `xtask::compose::project_name`과 같은 계산의 예상 프로젝트:
  `fvoci-leatherback-b57275-dev`, `fvoci-leatherback-b57275-test`,
  `fvoci-leatherback-b57275-supabase`. 세 프로젝트의 기존 컨테이너·볼륨은 없었다.
- 개발 구성의 예상 DB 이름은 `fvoci`이고 PG18.3을 사용한다. 현재 실제 연결 가능한 해당 DB는 없으므로
  DB 내부 객체·SQLx 이력·행 수는 NOT RUN이다. `dev`의 새 최초 실행은 기존 DB 재생성이 아닌 빈 DB 생성이다.
- `leatherback`, `dungeness`, `policy-browser`, `policy-browser2`의 `target/dev-files`는 존재하지 않았다.
- `fvoci-dev_pgdata`, `fvoci-dev_searchdata`는 남아 있다. 둘 다 2026-09-22 생성이고 Compose project
  라벨은 `fvoci-dev`이다. 현재 checkout으로 연결하는 컨테이너/working-dir 증거는 없다.
  같은 머신에 별도 `https://github.com/AISFlow/fvoci.git` checkout도 있으므로 GroundZero 소유로 단정하지 않는다.
  그 밖의 `fvoci-ubuntu2604-*`, `fvoci-github-local-dev-*`, `fvoci-backup-*` 및 익명 볼륨도 소유 범위 미확인으로 제외한다.

## 별도 과거 정리 기록

`dungeness/target/container-cleanup-result.json`에는 이전 작업이 GroundZero worktree 자원
컨테이너 118개·볼륨 22개·네트워크 76개를 정리한 기록이 있다. 현재 조사의 삭제 실적이 아니다.
해당 파일의 `verified_remaining`은 각각 0이며 현재 Docker 관측과 부합한다.
`policy-browser`/`policy-browser2`의 옛 DB·검색·Keycloak 볼륨도 그 이전 목록에 포함돼 있다.
기존 기록을 이번 작업의 재생성 승인이나 백업 존재 증거로 사용하지 않는다.

## Cloud 연결 가능 범위

- 현재 프로세스에는 `SUPABASE_TEST_HOST/PORT/DATABASE/USER/PASSWORD/SSL_ROOT_CERT`,
  `SUPABASE_ACCESS_TOKEN`, `DATABASE_ADMIN_URL`, `DATABASE_MIGRATION_URL`, `DATABASE_URL`이 모두 없다.
- 저장소 GitHub Secrets에는 기존 `SUPABASE_TEST_*` 7개 이름이 존재한다. Secret 원문은 읽거나 출력하지 않았다.
- 기존 main 전용 `supabase-cloud-readonly.yml`을 한 번 실행했다:
  <https://github.com/fvoci/GroundZero/actions/runs/39955012651>, 소스 `8ffd8c3b940924eca8cf7ec739a85494243c0479`.
  이 workflow는 TLS VerifyFull + session pooler + 읽기 전용 SELECT 1/서버 버전만 허용하며
  호스트·프로젝트·객체/이력·백업을 조사하지 않는다.
  **PASS**: 12:40:43 UTC에 connection/TLS/SELECT 1/server version 조회 성공.
  `server_version_num = 170011`(PostgreSQL 17.11), `changes = NOT RUN`.
  연결 옵션의 DB는 `postgres`, session port는 `5432`로 코드가 검증·고정하므로 이 연결 대상 DB명·포트는 확인된다.
  `current_database()`를 별도로 질의한 것은 아니다(NOT RUN).
  원본은 `cloud-readonly-precheck.log`, run/job 메타데이터는 `cloud-readonly-run.json`에 보존했다.
  이 값이 과거 프로젝트 `fpbqhckdkhhnhqvsixnr`의 버전이라는 뜻은 아니다. 기존 Secret이 가리킨 대상을
  현재 workflow가 익명화해 기록하므로 프로젝트 동일성은 아직 증명되지 않았다.
- 과거 프로젝트 `fpbqhckdkhhnhqvsixnr`와 현재 Secret 대상의 동일성, 실제 호스트, DB 내부 GroundZero
  객체·SQLx 이력, 다른 서비스와 공유 여부는 **미확인**이다. 기존 자격 증명 접근이 가능한 환경을
  받기 전에는 정확한 Cloud 삭제 대상 목록을 확정할 수 없다.

## 백업과 삭제 승인 경계

- 확인한 GroundZero checkout의 `target/`에서 제품 `manifest.json`/`.copy`/`.dump` 백업을 발견하지 못했다.
  다른 위치·Cloud 관리 백업 존재와 보존 기간은 미확인이다. 기존 SQL 검증 로그나 worktree archive는 DB 백업이 아니다.
- 현재 checkout에는 삭제할 기존 DB가 없다. 소유 불명 볼륨은 승인 목록에 넣지 않는다.
- Cloud는 대상과 공유 관계가 확인될 때까지 DROP/TRUNCATE/초기화가 차단된다.
- 삭제 전 백업을 만들 경우 별도 비공개 디렉터리(예: 운영자가 지정한 `backups/sql-baseline/<UTC>-<project>/`)에
  기존 릴리스의 GroundZero 백업 또는 PG 버전에 맞는 `pg_dump --schema=fvoci`와 SQLx 이력·소유/ACL 목록을 보존한다.
  첨부가 존재하면 같은 시점의 첨부 백업도 함께 확인한다. 비밀·개인 데이터가 들어 있는 백업은 Git/PR에 올리지 않는다.
  권한 0700/0600, 파일 목록·SHA-256·dump 목록 확인과 별도 빈 DB 복구 시험으로 검증한다.
- 기존 릴리스의 백업은 기존 릴리스·마이그레이션 기준의 별도 빈 DB로 복구한다. 새 통합 SQL로의 직접 복구는
  호환 경로가 아니며 시험하지 않았으므로 NOT RUN이다. 백업 없이 진행하는 선택은 사용자가 명시적으로 승인해야 하고,
  다른 복구본이 없다면 삭제 데이터는 복구할 수 없다.
- 대상이 검증되고 승인된 뒤에도 GroundZero의 정확한 객체 목록만 처리한다. `DROP DATABASE`, Supabase 프로젝트 reset,
  `auth/storage/realtime/extensions` 등 관리 스키마/역할, 다른 앱 객체, Keycloak, 첨부 저장소, 다른 worktree/Compose 볼륨,
  전역 prune, `cargo xtask down`은 제외한다. 외부 의존성이 있으면 그 파괴적 단계만 중단한다.
- 검색 색인은 별도 승인 대상으로 정한다. DB 삭제와 함께 자동 삭제하지 않는다.

## 재현한 조회

```sh
docker context show
docker context inspect default --format '{{json .Endpoints.docker.Host}}'
docker ps -a --format '{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}'
docker volume inspect fvoci-dev_pgdata fvoci-dev_searchdata
gh secret list --repo fvoci/GroundZero --json name,updatedAt
gh run view 39955012651 --repo fvoci/GroundZero --json status,conclusion,jobs,url
```

`project-names.tsv`와 `selected-remaining-volumes.txt`는 이 조사에 함께 보존한 비밀 없는 결과다.
