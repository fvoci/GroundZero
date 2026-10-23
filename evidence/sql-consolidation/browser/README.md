# 새 SQL 기준 개발·브라우저 검증

2026-10-23 UTC. 이번 작업에서 생성한 **새 빈 개발 DB와 합성 데이터만** 사용했다.
기존 데이터 삭제·초기화·복원은 수행하지 않았다. `cargo xtask down`도 실행하지 않았다.

## 대상과 소스

- checkout: `/home/kinesis/orca/workspaces/GroundZero/leatherback`.
- Compose project: `fvoci-leatherback-b57275-dev`.
- DB: `fvoci`, PostgreSQL 18.3, 앱 `fvoci_app`, migrator `fvoci_migrator`.
- 첫 dev 요청 SHA: `2e8b58b235449f461c3d4f7d8a190c5bb576fee3`.
  이미지 pull 중 서버 사전검사와 비교 도구 보완 커밋이 진행됐다. 첫 런타임 시작 뒤 관측한 checkout SHA는
  `078dbbc39265ecd92faefac364ee49cf5452092a`다. SQL 바이트는 그동안 동일했다.
- 반복 시작·반복 로그인·최종 재열기 SHA: `b7d34936d68b6cc2901c020e5cc26a6098314a54`.
  SHA 원문은 `source-sha.txt`, `runtime-source-sha.txt`, `restart-source-sha.txt`에 구분했다.
- SQLx 이력: `[202610230001]`. bootstrap → migrate → dev seed → Rust 앱/Vite 순서를 표준 `cargo xtask dev --oidc`로 실행했다.

## 결과

| 검사 | 결과 | 증거 |
| --- | --- | --- |
| 새 개발 DB bootstrap/migrate/seed와 서버 시작 | PASS | `dev-first-sanitized.log`, `seed-first.json` |
| 새 예제만 생성 | PASS | 사용자 2, 워크스페이스 2, 문서 10, 모듈 5, 항목 5, 업무 22; seed 표시 3개 |
| 실제 OIDC 브라우저 로그인 | PASS | `01-login.json`, HTTP callback 303 뒤 실제 앱 API 200 |
| 새 위키 문서 생성 | PASS | `02-document-created.json`; 새 문서 1개 추가 |
| 제목 저장·실시간 본문 편집 | PASS | `03-document-saved.json`, `document-db-readback.txt`의 `body_saved=t` |
| 문서에서 나간 뒤 재열기 | PASS | `04-document-reopened.json`에 저장한 제목·본문 일치 |
| UI 메뉴 로그아웃·미인증 접근 거부 | PASS | `05-logout.json`, `06-logout-access-denied.json`의 실제 browser fetch `/api/me` 401 |
| Ctrl-C 후 같은 DB 반복 시작 | PASS | `dev-restart-sanitized.log`; 기존 컨테이너·볼륨 유지 |
| 반복 seed 중복·덮어쓰기 없음 | PASS | `seed-before-restart.json`과 `seed-after-restart.json`의 바이트 동일; 사용자 2/공간 2/문서 11/모듈 5/항목 5/업무 22 |
| 반복 로그인·서버 재시작 후 문서 재열기 | PASS | `07-after-restart.json`에 제목·본문 유지; 두 번째 로그인은 reload 없이 홈 도달 |
| 렌더링 스크린숏 | FAIL (캡처 제한) | viewport/full screenshot 모두 빈 단색 PNG; 아래 범위 참고 |
| 구버전 백업 복원·새 SQL 호환 복원 | NOT RUN | 기존 백업/DB를 사용하지 않음 |

새 문서 제목은 `SQL baseline browser verified`, 본문은
`Fresh SQL baseline: browser save and reopen verification.`이다.
첫 로그인의 인증 콜백 직후에는 빈 DOM snapshot을 관측했고 reload 뒤 홈을 확인했다.
재시작 후 두 번째 로그인은 reload 없이 `Workspaces`를 확인했다. 최초 관측만으로 제품 결함이나 SQL 회귀로 판단하지 않았다.

## 스크린숏 한계

`document-reopened.png`(1280×770), `document-after-restart.png`(1265×1122)는 실제 Orca 캡처 결과이지만
모두 빈 단색 이미지다. 두 이미지를 열어 확인했으며 **성공한 화면 증거로 사용하지 않는다**.
현재 탭 전환 후 full-screenshot 재시도도 같았다. 동일 시점 접근성 snapshot과 DOM에는 문서 제목·본문이 있고,
브라우저 조작으로 실제 DB 쓰기·재열기·401 응답을 확인했다. `screenshot-diagnostic.json`은 DOM 상태만 기록하며
육안 렌더링 성공의 대체 증거가 아니다. Orca console 명령은 `method_not_found`였다.
이 검증은 DOM·HTTP·DB에 근거한 흐름 검증이며 픽셀/육안 UI 검증은 NOT RUN으로 남긴다.

## 남겨 둔 자원

- 컨테이너: `fvoci-leatherback-b57275-dev-postgres-1` (`970fd5ce5e1d`),
  `fvoci-leatherback-b57275-dev-keycloak-1` (`fab820a594f7`),
  `fvoci-leatherback-b57275-dev-meilisearch-1` (`86e4f717bb9a`).
- 볼륨: `fvoci-leatherback-b57275-dev_pgdata`, `fvoci-leatherback-b57275-dev_kcdata`,
  `fvoci-leatherback-b57275-dev_meilidata`.
- DB와 새 테스트 Keycloak 계정, 검색 색인, 예제 문서 11개를 보존했다.
  다른 checkout/Compose 프로젝트·기존 소유 불명 볼륨은 건드리지 않았다.
- 정확한 mount/owner/working-dir 관측은 `dev-resources.json`.
  현재 앱·Vite를 실행하는 Orca 터미널은 `term_5b6e087f-0530-4d48-bdf3-096facf3b26f`,
  검증 브라우저 페이지는 `81e2059c-ab6a-41ad-81b0-f994390e70af`다.

## 재현

```sh
cargo xtask dev --oidc
docker compose --project-name fvoci-leatherback-b57275-dev --file deploy/compose.dev.yaml exec -T postgres \
  psql -X -qAt -U fvoci -d fvoci < target/sql-consolidation/browser/seed-state.sql
# 브라우저: test-user 로그인 → 개발 wiki에서 새 문서 → 제목/본문 입력 → 재열기 → 계정 메뉴 로그아웃.
# Ctrl-C 뒤 같은 cargo xtask dev --oidc 반복. 데이터/볼륨 삭제 명령은 사용하지 않는다.
cmp target/sql-consolidation/browser/seed-before-restart.json target/sql-consolidation/browser/seed-after-restart.json
```

브라우저 저장 증거는 snapshot/refs와 안전한 경로·상태만 추출했다. OIDC query, 쿠키, 비밀번호를 기록하지 않았다.
31개 텍스트 증거에서 테스트 계정 비밀번호/자격증명 포함 PostgreSQL URL/OAuth query 패턴을 검사해 0건이었다.
공개 보존에는 `dev-first-sanitized.log`/`dev-restart-sanitized.log`를 사용한다. 원래 로그의 login 안내는 실제
비밀번호 값 없이 fixture 경로만 안내했지만, 보존본에서는 그 안내도 테스트 계정 목록으로 줄였다.
원본 `dev-first.log`/`dev-restart.log`는 로컬 ignored `target/`에만 유지하고 원격 증거 커밋에서는 제외한다.
