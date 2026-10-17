# fvoci 셀프호스트 설치·운영

한 대의 서버에 Docker Compose로 fvoci를 설치하고 운영하는 방법이다. 구성 파일은 `deploy/compose.selfhost.yaml`이고 비밀 값은 운영자의 `.env`에만 둔다. 개발·테스트 환경은 `docs/development.md`를 본다.

## 1. 준비

- Docker Engine과 Compose v2(예: Docker 29, Compose 2.40 이상).
- 사람들이 열 주소(예: `https://fvoci.example.com`)와 그 앞단의 HTTPS 역방향 프록시(Caddy·nginx·Traefik 등). 앱은 기본으로 이 컴퓨터에서만(`127.0.0.1:8080`) 듣는다.
- 로그인 제공자(OIDC). 이미 쓰는 제공자(Keycloak·Authentik·Google Workspace 등 표준 OIDC)가 있으면 그것을 연결하고, 없으면 함께 띄우는 Keycloak 프로필을 쓴다(4절). fvoci는 자체 비밀번호 로그인을 하지 않는다(D-45, X-09).
- 메모리 2 GB 이상(Keycloak을 함께 띄우면 3 GB 이상)과 파일을 담을 디스크.

## 2. 설치

```bash
git clone https://github.com/fvoci/GroundZero.git fvoci
```

```bash
cd fvoci && cp deploy/selfhost.env.example .env && chmod 600 .env
```

`.env`의 `change-me`를 모두 서로 다른 긴 임의 값으로 바꾼다(예: `openssl rand -base64 32`). `MEILI_MASTER_KEY`는 16자 이상이어야 한다. `FVOCI_PUBLIC_URL`은 사람들이 여는 주소, `FVOCI_OIDC_*`는 로그인 제공자에 등록한 클라이언트 값이다. 로그인 제공자에는 되돌아올 주소로 `<FVOCI_PUBLIC_URL>/auth/callback`, 로그아웃 뒤 주소로 `<FVOCI_PUBLIC_URL>/*`를 등록한다.

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml up -d --build --wait
```

처음 실행하면 이미지를 만들고(수 분), PostgreSQL과 Meilisearch가 준비되면 역할·스키마 준비(`db bootstrap`)와 마이그레이션(`db migrate`)이 한 번씩 돌고 앱이 뜬다. 상태 확인:

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml ps
```

앱이 `healthy`이면 역방향 프록시가 `FVOCI_LISTEN`(기본 `127.0.0.1:8080`)으로 보내도록 설정하고 `FVOCI_PUBLIC_URL`을 연다. 처음 로그인한 사람은 워크스페이스를 직접 만들거나 초대를 받는다(첫 로그인 사용자를 자동으로 관리자로 만들지 않는다, X-10).

## 3. 무엇이 어디에 저장되나

| 볼륨        | 내용                                                               |
| ----------- | ------------------------------------------------------------------ |
| `pgdata`    | PostgreSQL(모든 문서·업무·권한·감사 기록, Keycloak을 쓰면 그 DB도) |
| `meilidata` | 검색 색인(지워져도 앱이 다시 만든다)                               |
| `files`     | 첨부 파일 내용(로컬 저장소일 때)                                   |
| `backups`   | `backup create`로 만든 백업                                        |

첨부 파일을 S3 호환 저장소(예: Cloudflare R2)에 두려면 `.env`에 `FVOCI_STORAGE=s3`와 `FVOCI_S3_*`를 준다(`deploy/selfhost.env.example` 참고). R2는 지역을 `auto`로 둔다.

## 4. 함께 띄우는 Keycloak(선택)

로그인 제공자가 없으면 `--profile keycloak`으로 Keycloak을 함께 띄운다. 운영 모드이고 같은 PostgreSQL 안의 전용 DB를 쓴다. 전용 DB는 **처음 설치할 때**(빈 `pgdata`) 만들어지므로, 처음부터 `.env`에 `KEYCLOAK_*`를 채우고 이 프로필로 시작한다.

- `KEYCLOAK_URL`: 브라우저와 앱 서버가 **같은 주소로** 닿는 Keycloak 주소(예: `https://auth.example.com`). 앞단 프록시가 이 주소를 `KEYCLOAK_LISTEN`(기본 `127.0.0.1:8081`)으로 보낸다.
- `FVOCI_OIDC_ISSUER`는 `<KEYCLOAK_URL>/realms/fvoci`.
- 렐름 `fvoci`와 클라이언트 `fvoci-web`은 `deploy/keycloak/realm-selfhost.json`에서 가져온다(비밀 값과 주소는 `.env`에서 채운다). 사용자는 들어 있지 않다.

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml --profile keycloak up -d --build --wait
```

사용자는 Keycloak 관리 콘솔(`<KEYCLOAK_URL>/admin`, 계정 `admin`, 비밀번호 `KEYCLOAK_ADMIN_PASSWORD`)의 렐름 `fvoci`에서 만든다. 이메일을 "확인됨"으로 두어야 그 이메일로 받은 초대를 수락할 수 있다.

## 5. 백업과 복구

백업은 DB의 일관된 스냅숏과 첨부 파일 내용, 목록(`manifest.json`, 표별 행 수·SHA-256, 파일별 크기·SHA-256)을 한 폴더에 쓴다. 세션은 넣지 않는다(복구 뒤 모두 다시 로그인).

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml exec app fvoci-server backup create /backups/2026-10-17
```

백업 폴더를 서버 밖으로 옮긴다(예: `docker compose ... cp app:/backups/2026-10-17 ./2026-10-17`). 백업에는 모든 개인 노트와 첨부가 들어 있으므로 안전한 곳에 둔다.

복구는 **새로 설치한 빈 DB**에, **백업을 만든 것과 같은 릴리스**로 한다. 새로 설치(2절)해 앱이 한 번 뜬 뒤(빈 DB가 만들어짐) 앱을 멈추고 복구한 다음 다시 띄운다.

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml stop app
```

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml cp ./2026-10-17 app:/backups/2026-10-17
```

복사한 파일은 호스트 사용자 소유로 들어가므로 앱 사용자(`fvoci`)에게 넘긴다(백업 파일은 소유자만 읽을 수 있다).

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml run --rm --no-deps --user root --entrypoint chown app -R fvoci:fvoci /backups/2026-10-17
```

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml run --rm --no-deps app backup restore /backups/2026-10-17
```

복구가 끝나면 다시 띄운다(`up -d --wait`). 복구는 표 덤프의 SHA-256을 먼저 확인하고(손상되면 아무것도 건드리지 않음), 파일 내용을 저장소에 올리며 확인한 다음 한 트랜잭션에서 표를 채운다. 마이그레이션 버전이 다르거나 DB가 비어 있지 않으면 거부한다.

## 6. 올리기(업그레이드)

```bash
git pull && docker compose --env-file .env -f deploy/compose.selfhost.yaml up -d --build --wait
```

새 마이그레이션은 앱이 뜨기 전에 `migrate`가 적용한다. 올리기 전에 백업을 만든다. 되돌릴 때는 이전 릴리스로 새로 설치한 뒤 그 릴리스에서 만든 백업을 복구한다(마이그레이션은 되돌리지 않는다).

## 7. 알아 둘 것

- 비밀 값은 `.env`에만 있고 이미지·로그에 쓰지 않는다. `.env`는 소유자만 읽게 둔다.
- Meilisearch는 사용 통계 전송을 끈 운영 모드로 뜬다. 색인이 지워지면 앱이 다시 만든다.
- 실시간 공동 편집은 아직 없다(사용자 판단 대기). 문서는 저장할 때 리비전으로 남고, 동시에 고치면 충돌을 알리고 내 초안을 지킨다(D-53).
- 앱 한 프로세스가 API와 백그라운드 작업(휴지통 만료, 반복 업무, 검색 색인, 저장소 정리)을 함께 한다(`FVOCI_ROLE=all`).
- 지우기: `docker compose --env-file .env -f deploy/compose.selfhost.yaml down`은 데이터를 남긴다. `down -v`는 모든 볼륨(데이터)을 지운다.
