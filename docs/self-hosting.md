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

처음 실행하면 이미지를 만들고(수 분. HWP·HWPX 미리보기용 rhwp WASM을 고정한 상위 커밋에서 함께 빌드하므로 처음에는 10분 안팎과 Docker 디스크 수 GB가 더 들고, 입력이 같으면 다음부터는 캐시를 쓴다), PostgreSQL과 Meilisearch가 준비되면 역할·스키마 준비(`db bootstrap`)와 마이그레이션(`db migrate`)이 한 번씩 돌고 앱이 뜬다. 상태 확인:

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml ps
```

앱이 `healthy`이면 역방향 프록시가 `FVOCI_LISTEN`(기본 `127.0.0.1:8080`)으로 보내도록 설정하고 `FVOCI_PUBLIC_URL`을 연다. 처음 로그인한 사람은 워크스페이스를 직접 만들거나 초대를 받는다(첫 로그인 사용자를 자동으로 관리자로 만들지 않는다, X-10). 운영자가 `.env`의 `FVOCI_INITIAL_ADMIN_SUBJECT`에 관리자로 정한 사람의 OIDC subject(Keycloak은 사용자 화면의 ID)를 넣으면, 그 사람이 처음 로그인할 때 `FVOCI_INITIAL_WORKSPACE` 이름(기본 `fvoci`)의 첫 워크스페이스가 만들어지고 그 사람이 소유자가 된다(D-51, 한 번만).

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

PostgreSQL 이미지가 Alpine에서 Debian(`postgres:18.3-bookworm`, D-85)으로 바뀐 판으로 올릴 때는 위 명령으로 바로 올리지 않는다. 데이터 파일 소유자와 정렬 라이브러리가 달라 기존 `pgdata`를 그대로 쓸 수 없으므로, 이전 판에서 백업을 만들고(5절) 새 판을 새로 설치한 뒤 복구한다.

새 마이그레이션은 앱이 뜨기 전에 `migrate`가 적용한다. 올리기 전에 백업을 만든다. 되돌릴 때는 이전 릴리스로 새로 설치한 뒤 그 릴리스에서 만든 백업을 복구한다(마이그레이션은 되돌리지 않는다).

## 7. 캐시와 CDN(D-77)

앱이 경로마다 `Cache-Control`을 정하므로 CDN 없이 역방향 프록시만 두어도, Cloudflare·CloudFront를 앞에 두어도 같은 규칙으로 동작한다. 프록시·CDN은 이 머리글을 바꾸지 않아야 한다.

| 경로                                                                                            | 앱이 보내는 값                                                 | 뜻                                                                      |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `/assets/*`, `/rhwp/<입력 해시>/*`                                                              | `public, max-age=31536000, immutable`, `Vary: Accept-Encoding` | 파일 이름에 해시가 있어 바뀌지 않는다. 없는 파일은 404(SPA 화면이 아님) |
| `/`, SPA 경로, `/pptx-frame.js`, `/rhwp/manifest.json`                                          | `no-cache`, `Vary: Accept-Encoding`                            | 매번 재검증                                                             |
| `GET /api/documents/{id}`의 200·304                                                             | `private, no-cache, no-transform` + 강한 `ETag`                | 브라우저만 저장, 매번 세션·권한 확인 뒤 같으면 304                      |
| 그 밖의 `/api/*`, `/auth/*`, `/healthz`(오류·로그인·로그아웃·내보내기·첨부·익명 공유 링크 포함) | `no-store, no-transform`                                       | 어디에도 남기지 않는다                                                  |

- **조건부 요청**: 문서 저장(`PUT /api/documents/{id}`)은 `If-Match`(단건 조회의 ETag)를 받아 잠근 행 기준으로 비교하고, 다르면 412와 서버 최신본을 준다(편집 화면은 내 초안을 지킨다). `If-Match`가 없으면 기존 `base_revision`(409)으로 판단한다. 약한 ETag(`W/"…"`)는 `If-Match`에서 맞지 않으므로 프록시·CDN이 API 응답을 압축하거나 고치면 안 된다. 앱은 API 응답을 압축하지 않고 `no-transform`을 붙인다.
- **역방향 프록시(CDN 없음)**: 앞의 표대로 머리글을 그대로 넘긴다. `/api`·`/auth`에 압축(`encode`·`gzip on`)을 켜지 않는다(nginx `gzip`은 강한 ETag를 약하게 바꾼다). 메서드·`Cookie`·`Set-Cookie`·`Origin`·`If-Match`·`If-None-Match`와 304·412 응답, 웹소켓 업그레이드를 그대로 넘긴다.
- **Cloudflare**: `deploy/cdn/cloudflare-cache-rules.json`(캐시 규칙)과 `cloudflare-compression-rules.json`(압축 규칙)이 예시다. 민감 경로를 Bypass하는 규칙을 **맨 뒤**에 둔다(캐시 규칙은 겹쳐 적용되고 뒤의 규칙이 이긴다. 페이지 규칙의 Cache Everything은 캐시 규칙보다 약하다). 그 규칙에서 Respect Strong ETags를 켜고 브라우저 TTL은 원본을 따른다. 해시 자원만 캐시하고 Cloudflare가 다시 압축하게 둔다. 영역 전체의 Browser Cache TTL 덮어쓰기와 Cache Response Rules로 민감 경로를 캐시하게 만들지 않는다. 앱 주소에는 Rocket Loader·Email Obfuscation·Automatic HTTPS Rewrites를 켜지 않는다(본문을 고침). 네트워크 설정의 WebSockets를 켠다. 업로드 조각은 요청마다 `FVOCI_UPLOAD_CHUNK_BYTES`(기본 8 MiB, 최대 64 MiB)이므로 요금제의 요청 본문 한도보다 작게 둔다(한도는 Cloudflare 문서에서 지금 값을 확인).
- **CloudFront**: `deploy/cdn/cloudfront-cache-behaviors.json`이 배포 설정의 행동 부분 예시다. `/api/*`·`/auth/*`·`/healthz`와 기본 행동(HTML)은 관리형 CachingDisabled + AllViewer(모든 머리글·쿠키·쿼리 전달: `Cookie`·`Authorization`·`Origin`·`If-Match`·`If-None-Match`·웹소켓 머리글), 쓰기 메서드 허용, 압축 끔. `/assets/*`·`/rhwp/*`만 CachingOptimized(최소 TTL 1초라 `no-cache` 응답도 1초 남으므로 HTML·manifest에는 쓰지 않는다). 원본이 자기 이름의 `Host`를 요구하면 AllViewerExceptHostHeader를 쓴다. `FVOCI_PUBLIC_URL`은 CloudFront 주소(앱이 `Origin`을 이것과 비교).
- **격리를 캐시 키로 대신하지 않는다**: 사용자 쿠키를 캐시 키에 넣어 개인 응답을 나눠 캐시하는 설정을 쓰지 않는다. 민감 경로는 캐시하지 않는 것이 규칙이다.
- **세 가지 ETag·체크섬을 섞지 않는다**: 객체 저장소(R2·S3)의 객체·조각 ETag는 저장소가 조각을 맞추는 데만 쓰고(앱 밖으로 내보내지 않음), 파일 무결성은 조각마다 `x-content-sha256`과 완성 파일의 SHA-256으로 확인하며, API의 ETag는 JSON 표현의 검증자다. 비공개 첨부는 앱이 권한을 확인하고 내려주며(`no-store`) 영구 공개 URL로 바꾸지 않는다. 업로드 완료 단계는 파일을 다시 보내지 않는다.
- **확인**: `curl -sI https://<주소>/assets/<파일>`은 `immutable`, `curl -sI https://<주소>/api/me`는 `no-store`와 CDN의 캐시 상태가 우회(Cloudflare `cf-cache-status: BYPASS`/`DYNAMIC`, CloudFront `x-cache: Miss from cloudfront`)여야 한다. 실제 CDN 계정 설정·유료 서비스 생성은 운영자가 따로 정한다.

## 8. 운영 로그(D-83)

앱은 stdout에 한 줄에 JSON 객체 하나(JSON Lines)를 쓴다. Docker가 이것을 `json-file`로 남기고 서비스마다 `FVOCI_LOG_MAX_SIZE`(기본 `10m`) × `FVOCI_LOG_MAX_FILE`(기본 `5`)까지만 두고 돌린다. 앱이 따로 로그 파일을 쓰지 않으며 로그 서버·외부 전송은 없다.

- **필드**: 모든 줄에 `timestamp`(UTC RFC 3339), `level`, `target`(모듈), `event`(기계용 코드, 예: `http.request`·`http.error`·`job.failed`), `message`. 요청 중이면 `request_id`, 주기 작업 중이면 `job`·`job_id`. 요청마다 접근 줄 하나(`http.request`: `method`, `route`(경로 틀, 쿼리 없음), `status_code`, `duration_ms`, 거절이면 `error_code`). 서버 쪽 실패는 한 번 `error_code`·`detail`(분류만)과 함께 남는다. 입력 오류·권한 거부(4xx)는 오류 줄을 남기지 않는다. `/healthz`와 정적 파일은 DEBUG(5xx면 WARN). 패닉은 위치만 담은 `event: "panic"` 한 줄로 남는다.
- **남기지 않는 것**: 비밀번호·토큰·쿠키·`Authorization`·OIDC 코드·서명 URL·문서 본문·첨부 내용, 요청·응답 본문, 머리글, 쿼리, 설정 값, DB 바인딩 값, 바깥 서비스 오류 원문. SQL 문장(느린 쿼리 포함)도 남기지 않는다. 레벨을 올려도 같다(HTTP·TLS·DB 라이브러리는 `info`까지만, 첨부 내용을 찍는 PDF 라이브러리는 아예 나오지 않는다).
- **사용자 화면과 잇기**: 서버 오류 화면에는 요청 ID가 나온다(응답 머리글 `x-request-id`와 같다). 앞단 프록시가 안전한 모양(영문·숫자·`-_.` 8~64자)의 `x-request-id`를 주면 그 값을 쓴다(추적용일 뿐 권한과 상관없다).
- **감사 기록은 따로**: 생성·삭제·이름 변경·권한 변경 같은 감사 이벤트는 DB(`audit_log`)에 남고 로그 레벨과 상관없다.

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml logs --since 1h app
```

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml logs --no-log-prefix app | jq -c 'select(.level == "ERROR" or .level == "WARN")'
```

```bash
docker compose --env-file .env -f deploy/compose.selfhost.yaml logs --no-log-prefix app | jq -c 'select(.request_id == "<화면의 요청 ID>")'
```

**잠깐 자세히 보기와 원복**: 레벨·모듈 거르기는 시작할 때 한 번 읽으므로 바꾸면 앱 컨테이너를 다시 만든다(다른 서비스와 데이터는 그대로).

1. `.env`에 `FVOCI_LOG=info,fvoci_server=debug`(특정 모듈만이면 `info,fvoci_server::collab=debug`)를 넣는다. 잘못 쓰면 앱이 시작하지 않고 이유를 출력한다.
2. `docker compose --env-file .env -f deploy/compose.selfhost.yaml up -d app`
3. 확인이 끝나면 그 줄을 지우고(기본 `info`) 2를 다시 한다.

로컬 개발에서는 `FVOCI_LOG_FORMAT=pretty`로 같은 사건·필드를 한 줄 글로 본다. 운영은 기본 `json`을 쓴다.

## 9. 알아 둘 것

- 비밀 값은 `.env`에만 있고 이미지·로그에 쓰지 않는다. `.env`는 소유자만 읽게 둔다.
- Meilisearch는 사용 통계 전송을 끈 운영 모드로 뜬다. 색인이 지워지면 앱이 다시 만든다.
- 실시간 공동 편집(D-70)은 워크스페이스마다 관리자가 워크스페이스 설정에서 켠다(기본 꺼짐). 켜면 문서 화면이 웹소켓(`/api/documents/{id}/collab`)으로 붙으므로 역방향 프록시가 웹소켓 업그레이드(`Upgrade`·`Connection` 헤더)를 앱으로 넘겨야 한다(Caddy는 기본으로 넘김, nginx는 `proxy_http_version 1.1`과 두 헤더 설정). 앱은 브라우저의 `Origin`이 `FVOCI_PUBLIC_URL`과 같을 때만 연결을 받는다. 편집 방은 앱 프로세스 안에 있으므로 앱은 한 개만 띄운다. 연결된 사람의 권한은 `FVOCI_COLLAB_RECHECK_SECS`(기본 30초)마다 다시 확인한다. 꺼진 워크스페이스는 저장할 때 리비전으로 남고, 동시에 고치면 충돌을 알리고 내 초안을 지킨다(D-53).
- 앱 한 프로세스가 API와 백그라운드 작업(휴지통 만료, 반복 업무, 검색 색인, 저장소 정리)을 함께 한다(`FVOCI_ROLE=all`).
- 지우기: `docker compose --env-file .env -f deploy/compose.selfhost.yaml down`은 데이터를 남긴다. `down -v`는 모든 볼륨(데이터)을 지운다.
