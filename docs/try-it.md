# fvoci 직접 써 보기

이 Mac 한 대에서 fvoci를 띄워 직접 써 보는 안내다. 운영 설치의 상세(HTTPS 역방향 프록시, 외부 OIDC, CDN, R2)는 [셀프호스트 설치·운영](self-hosting.md)을 본다.

아래는 셀프호스트 설치본이 없는 상태에서 새로 설치하는 순서다. 기존 설치본(볼륨)이 있는지는 5단계에서 먼저 확인한다.

## 지금 바로 실행할 최소 단계

Docker Desktop이 켜져 있어야 한다. 첫 빌드는 Rust 서버와 HWP 미리보기 WASM을 함께 만들어 10분 안팎과 디스크 수 GB가 든다.

1. 작업 디렉터리를 만들고 받는다.

   ```bash
   mkdir -p ~/fvoci-try && cd ~/fvoci-try && git clone https://github.com/fvoci/GroundZero.git app
   ```

2. 비밀 값을 임의로 채운 `.env`를 만든다(값은 화면에 나오지 않는다).

   ```bash
   cd ~/fvoci-try/app && cp deploy/selfhost.env.example .env && chmod 600 .env && perl -pi -e 's/=change-me\S*/my $v = `openssl rand -hex 24`; chomp $v; "=$v"/ge' .env
   ```

3. 이 Mac 전용 주소를 넣는다.

   ```bash
   cd ~/fvoci-try/app && perl -pi -e 's{^FVOCI_PUBLIC_URL=.*}{FVOCI_PUBLIC_URL=http://localhost:18080}; s{^FVOCI_LISTEN=.*}{FVOCI_LISTEN=127.0.0.1:18080}; s{^FVOCI_OIDC_ISSUER=.*}{FVOCI_OIDC_ISSUER=http://auth.localhost:18081/realms/fvoci}; s{^KEYCLOAK_URL=.*}{KEYCLOAK_URL=http://auth.localhost:18081}; s{^KEYCLOAK_LISTEN=.*}{KEYCLOAK_LISTEN=127.0.0.1:18081}' .env && grep -c '^[^#].*change-me' .env
   ```

   마지막 숫자가 `0`이면 비밀 값이 모두 채워졌다.

4. 앱 컨테이너가 브라우저와 같은 주소(`auth.localhost`)로 Keycloak에 닿게 하는 덮어쓰기 파일과, 긴 명령을 줄인 실행 스크립트를 저장소 밖에 만든다.

   ```bash
   printf "services:\n  app:\n    extra_hosts:\n      - 'auth.localhost:host-gateway'\n" > ~/fvoci-try/local.override.yaml
   ```

   ```bash
   printf '#!/bin/sh\ncd "$HOME/fvoci-try/app" && exec docker compose --env-file .env -f deploy/compose.selfhost.yaml -f ../local.override.yaml --profile keycloak "$@"\n' > ~/fvoci-try/fv && chmod +x ~/fvoci-try/fv
   ```

5. 기존 데이터가 없는지 확인한 뒤 띄운다.

   ```bash
   docker volume ls --format '{{.Name}}' | grep '^fvoci_' || echo "기존 fvoci 볼륨 없음"
   ```

   볼륨이 나오면 띄우지 말고 아래 5절의 "기존 볼륨이 있을 때"를 먼저 본다.

   ```bash
   ~/fvoci-try/fv up -d --build --wait
   ```

6. 확인한다. `{"status":"ok","database":"ok","build":{"version":"dev","source":"dev"}}`가 나와야 한다(직접 빌드한 이미지는 `build`가 `dev`, 릴리스 이미지는 제품 버전과 소스 SHA).

   ```bash
   curl -s http://127.0.0.1:18080/healthz
   ```

7. Keycloak에서 사용자를 만들고(2절) 브라우저(Chrome 권장)로 <http://localhost:18080>을 열어 로그인한다.

포트를 8080·8081이 아니라 18080·18081로 두는 이유: 8080은 `cargo xtask dev`의 개발 서버가 쓴다.

## 1. 환경 변수

`.env`(`~/fvoci-try/app/.env`, 소유자만 읽기)의 값이다. 비밀 값은 2단계의 명령이 임의로 채우며 이 문서에도 출력에도 적지 않는다.

| 변수                                                                                                                       | 필수                          | 이 Mac에서 쓰는 값                         | 뜻                                                            |
| -------------------------------------------------------------------------------------------------------------------------- | ----------------------------- | ------------------------------------------ | ------------------------------------------------------------- |
| `POSTGRES_PASSWORD`, `FVOCI_DB_MIGRATOR_PASSWORD`, `FVOCI_DB_APP_PASSWORD`, `MEILI_MASTER_KEY`, `FVOCI_OIDC_CLIENT_SECRET` | 예(비밀)                      | 임의 값(2단계)                             | DB 역할·검색·OIDC 클라이언트 비밀. 서로 달라야 한다           |
| `KEYCLOAK_DB_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`                                                                          | Keycloak을 같이 띄울 때(비밀) | 임의 값(2단계)                             | Keycloak DB와 관리 콘솔 `admin` 비밀번호                      |
| `FVOCI_PUBLIC_URL`                                                                                                         | 예                            | `http://localhost:18080`                   | 사람들이 여는 주소. 쿠키·로그인 되돌아오기·Origin 검사의 기준 |
| `FVOCI_LISTEN`                                                                                                             | 아니요(기본 `127.0.0.1:8080`) | `127.0.0.1:18080`                          | 앱이 듣는 곳(이 Mac에서만)                                    |
| `FVOCI_OIDC_ISSUER`                                                                                                        | 예                            | `http://auth.localhost:18081/realms/fvoci` | OIDC 발급자. `KEYCLOAK_URL` + `/realms/fvoci`                 |
| `FVOCI_OIDC_CLIENT_ID`                                                                                                     | 아니요(기본 `fvoci-web`)      | `fvoci-web`                                | 렐름 틀의 클라이언트                                          |
| `KEYCLOAK_URL`                                                                                                             | Keycloak을 같이 띄울 때       | `http://auth.localhost:18081`              | 브라우저와 앱 컨테이너가 같은 주소로 닿는 Keycloak 주소       |
| `KEYCLOAK_LISTEN`                                                                                                          | 아니요(기본 `127.0.0.1:8081`) | `127.0.0.1:18081`                          | Keycloak이 듣는 곳                                            |
| `FVOCI_INITIAL_ADMIN_SUBJECT`                                                                                              | 아니요                        | 처음에는 빈칸, 2절에서 채움(선택)          | 초기 관리자로 정할 사람의 OIDC subject                        |
| `FVOCI_INITIAL_WORKSPACE`                                                                                                  | 아니요(기본 `fvoci`)          | `fvoci`                                    | 초기 관리자에게 만들어 줄 첫 워크스페이스 이름                |

파일 저장소는 기본 로컬 볼륨이다. S3 호환(R2 등)과 로그 설정은 [셀프호스트 3절·8절](self-hosting.md)을 본다.

`auth.localhost`: 브라우저는 `*.localhost`를 이 Mac(127.0.0.1)으로 보낸다. 앱 컨테이너 안에서는 `localhost`가 컨테이너 자신이므로, 4단계의 덮어쓰기 파일이 컨테이너 안의 `auth.localhost`를 이 Mac으로 보낸다. 실제 도메인으로 운영하면 이 파일은 필요 없다.

## 2. 로그인 준비 (Keycloak, OIDC 전용)

fvoci는 자체 비밀번호 로그인이 없다. 사람은 Keycloak(같이 띄운 로그인 제공자)에서 만들고, fvoci는 Keycloak이 확인한 신원(발급자 + subject)으로 로그인한다. 처음 로그인한 사람을 자동으로 관리자로 만들지 않는다.

1. Keycloak 관리 콘솔을 연다: <http://auth.localhost:18081/admin>. 계정은 `admin`, 비밀번호는 `.env`의 `KEYCLOAK_ADMIN_PASSWORD`다. 화면에 띄우지 않고 클립보드로 옮기려면:

   ```bash
   grep '^KEYCLOAK_ADMIN_PASSWORD=' ~/fvoci-try/app/.env | cut -d= -f2- | tr -d '\n' | pbcopy
   ```

2. 왼쪽 위 렐름 선택에서 **fvoci**를 고른다(`master`가 아니다).
3. **Users → Create new user**: Username(예: `minji`), Email, First name·Last name을 넣고 **Email verified**를 켠 뒤 Create. 초대를 이메일로 받으려면 이메일 확인이 켜져 있어야 한다.
4. 그 사용자의 **Credentials → Set password**에서 비밀번호를 정하고 **Temporary**를 끈다.
5. 실제로 쓸 개인 계정으로 만든다. 저장소의 `deploy/keycloak/realm-test.json`에 든 테스트 계정은 자동 테스트 전용이며 이 설치에는 들어 있지 않다.

**초기 관리자(선택)**: 정해 두면 그 사람이 처음 로그인할 때 `FVOCI_INITIAL_WORKSPACE` 이름(기본 `fvoci`)의 워크스페이스가 만들어지고 그 사람이 소유자가 된다(인스턴스에서 한 번). 정하지 않아도 누구나 로그인 뒤 자기 워크스페이스를 만들 수 있다.

1. Keycloak에서 그 사용자의 **Details**에 있는 **ID**(UUID)를 복사한다. 이것이 OIDC subject다.
2. `.env`에 넣고 앱만 다시 만든다(`<ID>`를 복사한 값으로 바꾼다).

   ```bash
   cd ~/fvoci-try/app && perl -pi -e 's{^FVOCI_INITIAL_ADMIN_SUBJECT=.*}{FVOCI_INITIAL_ADMIN_SUBJECT=<ID>}' .env && ~/fvoci-try/fv up -d --wait app
   ```

3. 그 사람으로 로그인하면 사이드바의 워크스페이스에 `fvoci`가 생긴다. 다른 사람은 이후 초대로 들어온다(멤버 화면).

**앱 로그인**: <http://localhost:18080>에서 **로그인** → Keycloak 로그인 화면에서 위 계정 → fvoci 홈으로 돌아온다. 로그아웃은 사이드바 아래 계정 메뉴 → **로그아웃**(Keycloak 세션도 끝난다).

## 3. 15분 체험

메뉴 이름은 한국어 화면 기준이다. 계정 메뉴 → **표시 설정**에서 언어·시간대를 바꿀 수 있다.

| 순서 | 위치와 동작                                                                                                                                                          | 입력 예시                                | 정상 결과                                                                                                                     |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 1    | 워크스페이스 만들기: **홈**의 워크스페이스 영역 **새 워크스페이스 이름** → **만들기**. 이미 있으면 사이드바 위 워크스페이스 선택 → **워크스페이스 만들기·초대 확인** | `내 연구실`                              | 사이드바에 워크스페이스가 생기고 **위키·모듈·멤버·달력·휴지통·감사 기록**이 보임                                              |
| 2    | 모듈 만들기: 워크스페이스 화면의 **모듈** 영역 → **모듈 만들기**(유형·이름·키)                                                                                       | 유형 **프로젝트**, 이름 `개발`, 키 `DEV` | 사이드바 **모듈**에 `개발`. 키는 영문으로 시작하는 2~10자 영문·숫자이고 `WIKI`는 쓸 수 없음. 같은 키를 다시 쓰면 거절         |
| 3    | 항목 만들기: 모듈 화면 **새 항목 제목** → **항목 만들기**                                                                                                            | `로그인 개선`                            | 번호 `DEV-1`이 붙음. 새 항목은 비공개(만든 사람과 관리자만, 항목 멤버로 넓힘)                                                 |
| 4    | 위키 문서: 워크스페이스 화면 **새 문서 제목** → **문서 만들기**                                                                                                      | `회의록`                                 | 문서가 열리고 위키 트리에 보임                                                                                                |
| 5    | 편집: 본문에 `/` → 목록에서 **체크리스트**, 항목 입력 후 Enter. `@`를 치고 `DEV`나 문서 제목 입력 → 목록에서 고름. 블록 왼쪽 손잡이로 옮기기·복제·지우기             | `/체크` → `자료 정리`, `@DEV-1`          | 체크 항목과 참조 링크가 생김. 상태가 **저장 대기 → 저장됨**(자동 저장, **저장** 단추도 있음). 새로 고침해도 그대로            |
| 6    | 업무: 항목 화면 **새 업무 제목** → **업무 만들기**. 업무를 열어 시작일·마감일을 넣음. 위 보기 전환 **칸반**·**달력**·**간트**                                        | `로그인 화면`, 마감일 오늘부터 3일 뒤    | 칸반 **할 일·진행 중·완료** 열에 보이고 끌어서 옮기면 상태가 바뀜. 달력 마감일 칸에 `DEV-2`. 사이드바 **달력**에도 보임       |
| 7    | 강좌: 2번처럼 유형 **강좌** 모듈(키 `LEC`) → 항목 `러스트 기초` → 항목 화면 **회차**에서 **새 회차 제목**·길이 → **회차 추가**. 회차마다 **내 계획일**과 **완료**    | 회차 `소개`, 길이 `10`(분)               | 회차 `1회`가 생김. 계획일을 넣으면 달력에 **수강** 일정, **완료**를 누르면 상태가 완료                                        |
| 8    | 스톱워치: 위 막대의 **타이머 시작** → 잠시 뒤 **종료**. 사이드바 **개인 → 내 시간**                                                                                  | 종류 **작업**, 대상 `DEV-2`(선택)        | "타이머를 끝냈습니다(…)" 안내, **내 시간**의 **기록**에 방금 시간이 보이고 종류·대상을 고칠 수 있음. 타이머는 한 번에 하나    |
| 9    | 검색: 사이드바 **검색**(Ctrl/⌘ K)                                                                                                                                    | `회의록`, `DEV-1`                        | 문서·업무·항목·일정·파일이 나옴. 방금 만든 내용은 색인에 몇 초 걸릴 수 있음                                                   |
| 10   | 알림: 사이드바 **개인 → 알림**                                                                                                                                       | -                                        | 혼자 쓰면 "알림이 없습니다." 알림은 다른 멤버가 나를 `@사람`으로 언급하거나 내 문서에 댓글을 달 때 생김(멤버를 초대해야 확인) |
| 11   | 휴지통·되살리기: 지울 문서를 열고 **문서 지우기** → **휴지통으로**. 사이드바 **휴지통**(워크스페이스) → 그 줄의 **되살리기**                                         | 연습용 문서 `지울 문서`                  | 지우면 목록에서 사라지고 휴지통에 보임. 되살리면 원래 자리로 돌아옴. 개인 노트는 **내 노트 휴지통**                           |

## 4. Mac과 iPhone에서 접속하기

- **이 Mac의 브라우저**: 위 설정 그대로 <http://localhost:18080>. `localhost`와 `auth.localhost`는 언제나 "지금 이 기기"를 뜻하므로, 다른 기기에서 이 주소를 열면 그 기기 자신을 찾아 실패한다.
- **다른 Mac이나 iPhone**: 지금 설정으로는 안 된다. 앱과 Keycloak이 이 Mac의 `127.0.0.1`에서만 듣고(`FVOCI_LISTEN`, `KEYCLOAK_LISTEN`), 주소가 `localhost` 기준이기 때문이다. 원격 접속에 필요한 것:
  1. 두 기기가 모두 닿는 실제 호스트 이름(예: 사내 DNS 이름, 공개 도메인)과 그 이름으로 받는 HTTPS 역방향 프록시. 이름과 인증서는 운영자가 정한다.
  2. `FVOCI_PUBLIC_URL`을 그 앱 주소(https)로, `KEYCLOAK_URL`·`FVOCI_OIDC_ISSUER`를 그 Keycloak 주소(https)로 바꾼다. Keycloak의 되돌아올 주소(`<FVOCI_PUBLIC_URL>/auth/callback`)와 로그아웃 뒤 주소는 렐름 틀이 `FVOCI_PUBLIC_URL`로 채우는데, 이 틀은 Keycloak DB가 비어 있을 때 한 번만 가져오므로 이미 띄운 설치에서 주소를 바꾸면 Keycloak 관리 콘솔(렐름 fvoci → Clients → fvoci-web)에서도 같이 바꿔야 한다.
  3. https 주소일 때만 앱 세션 쿠키가 `Secure`(`__Host-` 이름)로 붙는다. 렐름은 `sslRequired: external`이라 사설 IP가 아닌 곳에서는 Keycloak이 https를 요구한다.
  4. 프록시는 웹소켓(실시간 편집)과 머리글을 그대로 넘겨야 한다: [셀프호스트 7절·9절](self-hosting.md).
- 이 안내는 방화벽·공유기·외부 공개 설정을 바꾸지 않는다. 같은 와이파이의 iPhone에서 `http://<Mac IP>:포트`로 여는 방식은 확인하지 않았고, `localhost` 기준 쿠키·로그인 되돌아오기 주소와 맞지 않아 그대로는 로그인되지 않는다.

## 5. 종료·재실행·데이터·백업

- **멈추기(데이터 유지)**: `~/fvoci-try/fv stop` 또는 `~/fvoci-try/fv down`. 둘 다 볼륨을 지우지 않는다. `down -v`는 모든 데이터를 지우므로 쓰지 않는다.
- **다시 띄우기**: `~/fvoci-try/fv up -d --wait`(코드를 새로 받은 뒤에는 `--build`를 붙인다. 새 마이그레이션은 앱이 뜨기 전에 적용된다).
- **데이터 위치**(Docker 볼륨, 프로젝트 이름 `fvoci`):

  | 볼륨              | 내용                                                        |
  | ----------------- | ----------------------------------------------------------- |
  | `fvoci_pgdata`    | PostgreSQL: 모든 문서·업무·권한·감사 기록과 Keycloak 사용자 |
  | `fvoci_meilidata` | 검색 색인(지워지면 앱이 다시 만든다)                        |
  | `fvoci_files`     | 첨부 파일 내용                                              |
  | `fvoci_backups`   | `backup create`로 만든 백업                                 |

  그 밖에 `~/fvoci-try/app/.env`(비밀 값)와 `~/fvoci-try/local.override.yaml`, `~/fvoci-try/fv`가 저장소 밖 설정이다. `.env`를 잃으면 같은 볼륨을 다시 열 수 없으므로 따로 안전하게 둔다.

- **백업 만들기와 꺼내기**:

  ```bash
  ~/fvoci-try/fv exec app fvoci-server backup create /backups/$(date +%F)
  ```

  ```bash
  mkdir -p ~/fvoci-try/backups && ~/fvoci-try/fv cp app:/backups/$(date +%F) ~/fvoci-try/backups/$(date +%F)
  ```

  백업에는 모든 개인 노트와 첨부가 들어 있으므로 안전한 곳에 둔다. 세션은 넣지 않는다(복구하면 모두 다시 로그인).

- **복구**: 새로 설치한 빈 DB에, 백업을 만든 것과 같은 릴리스로 한다. 기존 설치 위에 덮어쓰지 않는다. 절차(앱 멈춤 → `cp` → `chown` → `backup restore` → 다시 띄움)는 [셀프호스트 5절](self-hosting.md)을 그대로 따른다. 복구는 비어 있지 않은 DB나 다른 마이그레이션 판을 거부한다.
- **기존 볼륨이 있을 때**: `fvoci_` 볼륨이 이미 있으면 그 설치의 `.env` 없이 새 비밀 값으로 띄우지 않는다(DB가 이전 비밀 값으로 만들어져 있다). 예전 이미지가 PostgreSQL Alpine 판(2026-10-18 이전 main)이었다면 그 데이터 폴더를 지금의 Debian 판으로 그대로 올리지 않는다: 예전 판으로 백업 → 새 설치 → 복구([셀프호스트 6절](self-hosting.md), `docs/development.md` 75절).

## 6. 문제가 생겼을 때

| 증상                          | 확인                                                                                                                                                                                                                                 |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 접속이 안 됨                  | `~/fvoci-try/fv ps --all`(app·postgres·meilisearch는 healthy, bootstrap·migrate는 `Exited (0)`), `curl -s http://127.0.0.1:18080/healthz`, `~/fvoci-try/fv logs --since 10m bootstrap migrate app`                                   |
| 로그인 화면이 반복되거나 오류 | 주소를 `http://localhost:18080`으로 열었는지(127.0.0.1이 아니라), `.env`의 `FVOCI_PUBLIC_URL`·`KEYCLOAK_URL`·`FVOCI_OIDC_ISSUER`, `~/fvoci-try/fv logs --since 10m app keycloak`, 덮어쓰기 파일(`local.override.yaml`)을 함께 썼는지 |
| 저장 실패                     | 화면의 상태 문구(저장 실패·재시도 대기·충돌)와 요청 ID, `~/fvoci-try/fv logs --no-log-prefix app \| jq -c 'select(.level == "ERROR" or .level == "WARN")'`                                                                           |

로그는 컨테이너 표준 출력에만 있고 Docker가 서비스마다 10 MB × 5개까지 돌린다([셀프호스트 8절](self-hosting.md)). 요청 ID로 찾기: `~/fvoci-try/fv logs --no-log-prefix app | jq -c 'select(.request_id == "<화면의 요청 ID>")'`.

`bootstrap`·`migrate`에서 멈추면 [공통 DB 준비·마이그레이션 안내](self-hosting.md#db-setup)를 따른다.
이 안내의 Compose 명령은 같은 설정을 쓰는 `~/fvoci-try/fv`로 실행한다. 기존 DB의 checksum 불일치를 볼륨 삭제로 해결하지 않는다.

문제를 알릴 때는 다음만 전달한다: 쓰는 커밋(`git -C ~/fvoci-try/app log --oneline -1`), 화면 캡처, 일어난 시각, 화면에 나온 요청 ID. `.env`, 비밀번호, 쿠키, 백업 파일은 보내지 않는다.

## 7. 확인 범위

**실제로 실행해 확인함**(2026-10-18~19, main `2d424f2`를 새로 clone한 디렉터리, Chromium 기반 브라우저):

- 2~6단계와 같은 내용의 `.env`·덮어쓰기 파일로 `up --build --wait`, bootstrap·migrate 종료 0, `/healthz` ok, 웹 200. 단 확인 때는 Compose 프로젝트 이름을 따로 주었고(`-p`), 이 문서의 `.env` 채우기 명령은 설치 없이 `docker compose config`로만 검사했다.
- Keycloak 사용자 생성(관리 CLI `kcadm`), 브라우저 OIDC 로그인·로그아웃·다시 로그인.
- 3절의 각 동작(워크스페이스·모듈 키 거절·항목 번호·문서 편집과 `/`·체크리스트·`@` 참조·업무·칸반·간트·달력·반복 일정·강좌 회차와 진도·타이머·내 시간·검색·알림(두 사용자)·휴지통·되살리기). 화면 메뉴를 하나하나 누르기보다 같은 화면이 쓰는 API와 화면 내용 확인으로 했다.
- 멈춤·다시 띄우기 뒤 데이터 유지, 백업 → 새 설치에 복구 → 다시 로그인.

**문서·코드만 검토함**:

- 이 문서의 `~/fvoci-try` 경로와 `fv` 스크립트로 실제로 띄우는 것. 설치 없이 검사한 것: 모든 명령 블록의 문법(bash·zsh), `.env` 채우기 결과(빈 비밀 값 0개), 덮어쓰기 파일·`fv` 스크립트 생성, 합친 Compose 설정(`config --quiet` 통과, 프로젝트 이름 `fvoci`, 볼륨 `fvoci_pgdata`·`fvoci_meilidata`·`fvoci_files`·`fvoci_backups`).
- Keycloak 관리 콘솔 화면의 버튼 이름(Keycloak 26 기준. 확인 때는 같은 작업을 CLI로 했다).
- 초기 관리자 지정(코드와 통합 시험 `the_designated_initial_admin_gets_the_first_workspace_once`로 확인, 브라우저로는 하지 않음).
- 3절의 메뉴 위치와 문구(화면 코드·번역 파일 기준).

**확인하지 않음(NOT RUN)과 영향**: 추가 계정·자격 증명·기기가 필요하거나 지금 환경에서 볼 수 없는 것들이다. 지원·검증 완료로 보지 않는다.

| 항목                                  | 영향                                                                                                                    | 그동안 할 일                                                                                      |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Safari에서 `auth.localhost` 주소      | Safari가 `*.localhost`를 이 Mac으로 보내지 않으면 Keycloak 로그인 화면이 열리지 않을 수 있다                            | 이 문서의 체험은 Chrome으로 한다                                                                  |
| 다른 기기(다른 Mac·iPhone) 접속·HTTPS | 4절의 조건(실제 호스트 이름, HTTPS 프록시, 주소 바꾸기)대로 설정해도 실제로 로그인·실시간 편집이 되는지 확인하지 않았다 | 이 Mac에서만 쓴다. 원격 공개는 [셀프호스트](self-hosting.md)대로 운영자가 따로 정한다             |
| 실물 휴대전화                         | 휴대폰 화면 크기 흉내로만 확인해, 실제 터치·키보드·화면 회전에서의 사용감은 모른다                                      | 휴대폰 접속 자체가 원격 접속 조건을 먼저 갖춰야 한다                                              |
| 한국어 OS 입력기                      | 조합 중 Enter·`/`·`@` 처리는 단위 시험으로만 확인했다. 실제 입력기에서 메뉴가 엉뚱하게 반응할 가능성이 남는다           | 이상하면 화면·시각·요청 ID와 함께 알린다(6절)                                                     |
| 화면 낭독기                           | 접근성 트리(이름·역할)만 확인했다. 실제 낭독 순서·안내는 모른다                                                         | -                                                                                                 |
| 실제 화면 배치                        | 검증 브라우저 창이 최소화돼 캡처를 못 했다. 겹침·잘림 같은 모양 문제는 화면 구조·글꼴 요청으로만 확인했다               | 이상하면 화면 캡처와 함께 알린다                                                                  |
| R2(S3 호환 운영 저장소)               | 로컬 S3 대역(SeaweedFS)으로만 시험했다. 실제 R2의 요청 한도·오류 응답 차이는 모른다                                     | 체험은 기본 로컬 파일 저장소(`fvoci_files` 볼륨)로 한다                                           |
| 실제 Zotero                           | 대역 서버로만 시험했다. 실제 계정·라이브러리 읽기는 모른다                                                              | 체험 순서에 넣지 않았다                                                                           |
| 실제 CDN(Cloudflare·CloudFront)       | 예시 설정 파일만 있고 실제 계정에서 캐시·우회가 맞는지 모른다                                                           | 로컬 체험에는 CDN이 없다. 운영에 앞단 CDN을 둘 때 [셀프호스트 7절](self-hosting.md)의 확인을 한다 |
