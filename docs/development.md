# fvoci 개발 지침

현재 개발·검사·유지보수에 쓰는 지침이다. 제품 정책과 승인 범위는 [결정 문서](decisions.md),
파일 위치는 [저장소 구조](repo-structure.md), 운영 설치는 [셀프호스트 안내](self-hosting.md)를 따른다.
과거 실행 결과는 [고정 SHA의 기록](#history)에 남겨 두며 현재 동작이나 검증 완료의 근거로 자동 재사용하지 않는다.

## 1. Rust

- rustfmt와 Clippy를 필수 검사로 사용한다. [고정 툴체인](../rust-toolchain.toml)에서 경고를 오류로 처리한다.
- 제품 코드의 무분별한 `unwrap`·`expect`·`panic`을 금지한다. 예상 가능한 입력·네트워크·DB 오류는 `Result`로 전달하고 경계에서 처리한다.
- 오류를 버리거나 성공으로 바꾸지 않는다. 정리 작업의 실패도 이유와 처리 정책을 명시한다.
- 자체 제품 코드의 `unsafe`는 기본 금지한다. 필요한 예외는 범위·안전성 근거·검증을 별도로 검토한다. 의존성 내부까지 금지한 것으로 해석하지 않는다.
- ID·권한·상태·단위를 의미 있는 타입으로 표현한다. 모든 값을 `String`이나 JSON으로 넘기지 않는다.
- 비동기 작업의 취소·종료·오류 회수 책임을 정한다. 무제한 spawn, 블로킹 작업의 무분별한 비동기 실행, 잠금을 잡은 채 불필요하게 기다리는 구조를 피한다.
- Clippy restriction·pedantic 전체를 일괄 활성화하지 않는다. 선택한 규칙은 [Cargo.toml의 workspace.lints](../Cargo.toml), 테스트 예외는 [clippy.toml](../clippy.toml)이 정본이다(O-23 해소).

## 2. TypeScript·Svelte

- `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, `noImplicitOverride`를 사용한다.
- `.ts`와 `.svelte`를 svelte-check와 타입 정보를 사용하는 ESLint로 검사한다. 설정은 [tsconfig.base.json](../tsconfig.base.json)과 [eslint.config.mjs](../eslint.config.mjs)에 있다.
- 외부 데이터는 `unknown`에서 시작해 실제로 검증한다. 타입 단언만으로 API·스토리지·메시지 입력을 신뢰하지 않는다.
- `any`, 이중 단언 `as unknown as T`, 근거 없는 non-null assertion, `@ts-ignore`·`@ts-nocheck`로 오류를 숨기지 않는다.
- 비동기 작업의 오류·취소·오래된 응답 처리를 명시한다. `void`만 붙여 Promise의 실패를 처리했다고 보지 않는다.
- 이벤트·구독·타이머·편집기 인스턴스는 화면 수명주기에 맞춰 해제한다.

<a id="api-contract"></a>

## 3. Rust ↔ TypeScript 계약

- Rust 요청·응답 타입에서 utoipa로 [OpenAPI](../contracts/openapi.json)를 만들고, openapi-typescript로 [클라이언트 타입](../packages/api-client/src/generated)을 생성한다(O-22 해소). DTO를 양쪽에서 독립적으로 손으로 유지하지 않는다.
- API를 바꾸면 `cargo xtask openapi` 후 `pnpm run generate:api`를 실행한다. `cargo xtask check`가 생성물의 최신 여부를 검사한다.
- 생성된 타입은 런타임 검증을 대신하지 않는다. 클라이언트는 응답을 `unknown`으로 받아 검사한다.
- UUID·날짜·시각은 문자열로 전송한다. 64비트 정수를 JavaScript `number`로 무조건 전달하지 않는다. 각 필드의 표현은 Rust 전송 타입과 OpenAPI에 함께 반영한다.
- 문서·체크리스트·업무 상태의 정본을 명시하고 화면마다 별도 사본을 관리하지 않는다.
- 과거 개발 버전만을 위한 별칭·fallback은 D-92를 따른다. 지원 환경 간 차이 처리와 정상적인 실패 복구·재시도까지 제거하지 않는다.

## 4. 검사와 예외

- 테스트·xtask·개발 도구도 검사한다. 테스트의 의도적인 panic·expect 등은 필요한 범위만 예외로 둔다.
- 예외에는 최소 범위와 이유를 적는다. 파일·프로젝트 전체 규칙 해제로 통과시키지 않는다. 생성물·외부 코드의 제외에 자체 코드를 섞지 않는다.
- 실제 지원하는 feature 조합을 검사한다. 현재는 기본 feature를 검사하며, 서로 배타적인 feature를 억지로 `--all-features`로 합치지 않는다.
- 로컬과 CI는 같은 검사 진입점을 쓴다. 검사를 생략해 성공으로 표시하지 않는다. `--skip-licenses`를 썼다면 라이선스 검사는 별도로 수행·보고한다.
- 검사 설정을 추가·변경할 때는 대표 위반이 실제로 실패하는지 확인한 뒤 제거한다. 실행 명령·대상 SHA·PASS/FAIL/NOT RUN·남은 제한을 PR이나 작업 보고에 적는다(D-42). 별도 증거 프레임워크를 만들지 않는다.
- 설정 존재, 구현 완료, 실행 성공, 지원 보장을 구분한다. CodeRabbit의 전체 검토·증분 검토·파일 생략·전체 skipped·지적 해소도 따로 적는다. 새 지적이 없다는 것만으로 검토 완료를 선언하지 않는다.
- 실행별 TXT·JSON·JSONL 로그, 환경·카탈로그 스냅숏, 스크린숏, 중간 소스 사본은 현재 지침과 분리한다. 역사 기록임을 표시하고 관련 결함·SHA를 확인할 때만 좁게 읽는다.
- 새 원시 출력은 기존 `target/` 경로에 둔다. 장기 증거는 접근 가능한 원격 커밋에 보존하고 요약에 SHA·범위·결과·대체 관계·재현 명령·SHA-256·permalink를 남긴다. 추적 파일을 분리하기 전 원격 바이트를 확인한다.
- `.gitignore`만으로 추적 파일이나 모든 도구가 제외되지는 않는다. 검색·색인·포맷·lint·시험 수집 대상을 확인한다. 제품 코드·fixture·비교 도구·생성 계약·클라이언트·고지·자산은 실행 로그로 취급하지 않는다. [SQL 비교와 증거 위치](../crates/server/migrations/README.md#comparison)를 참고한다.

## 5. 복잡도 원칙

타입과 검사는 엄격하게, 구조는 단순하게 유지한다. 규칙을 맞추기 위한 범용 프레임워크·중복 래퍼·대규모 형식 변환 계층을 만들지 않는다.
타입 검사 통과를 권한·동시성·데이터 정합성 검증으로 대신하지 않는다. 서버가 권한·업무 규칙·정합성을 최종 판단한다.

## 6. 개발·통합 테스트 환경: Docker Compose

- PostgreSQL 최소 버전은 17이며 검증한 메이저만 [지원 목록](self-hosting.md#postgres-support)에 둔다(D-100). 기본 시험은 PG17, 기존 개발·셀프호스트 이미지는 PG18이다. PG18 데이터 볼륨을 PG17 이미지에 연결하지 않는다. 환경별 제품 SQL은 분기하지 않는다.
- PostgreSQL·Meilisearch·Keycloak·S3 시험용 SeaweedFS는 고정한 컨테이너로 실행한다. 운영 서비스나 개인 개발 DB를 테스트에 쓰지 않는다.
- OIDC 시험은 격리된 Keycloak으로 로그인·세션 만료·로그아웃·권한 거부를 확인한다. 제품 코드는 Keycloak 전용 API에 의존하지 않는다.
- Dockerfile·Compose·의존성 버전을 저장소에서 관리하고 로컬과 CI가 같은 이미지·검사 명령을 쓴다. 제품 실행 이미지와 빌드 이미지를 분리한다.
- 병렬 실행은 worktree·Compose 프로젝트·네트워크·볼륨·DB·스토리지 경로를 분리한다. 같은 빌드 출력 디렉터리에 동시에 쓰지 않는다. 같은 입력의 이미지는 재사용한다.
- macOS·Linux의 UID/GID 차이를 고려해 생성 파일의 소유권을 확인한다. `1000:1000`을 고정하지 않는다.
- 시크릿을 이미지·Git·로그에 넣지 않는다. 테스트 자격 증명은 `deploy/`의 명백한 테스트 값만 쓴다.
- 불필요한 privileged 모드·Docker 소켓·호스트 전체 디렉터리 마운트를 금지한다.
- 실제 CPU·메모리·디스크 사용량에 맞춰 무거운 작업의 동시 실행 수를 제한한다. 메모리 한도가 있는 서비스가 호스트 코어 수로 작업자를 만들면 CPU도 함께 제한한다. 시험 Meilisearch가 CPU 2개를 요구하므로 `cargo xtask test` 실행 호스트에도 CPU 2개 이상이 필요하다.
- 자신이 만든 컨테이너·네트워크·시험 볼륨만 정리한다. 전역 prune이나 다른 작업의 자원 삭제를 하지 않는다. 기존 데이터의 삭제·전환은 D-88·D-92의 별도 확인 범위를 따른다.
- 빠른 편집·포맷·타입 검사는 고정 버전의 호스트 도구를 쓸 수 있다. 최종 통합 검증은 표준 컨테이너 환경에서 한다. 진입점은 xtask·pnpm이며 별도 실행 프레임워크를 만들지 않는다.

## 7. 의존성 라이선스

정책과 예외의 승인 범위는 [결정 문서의 D-60·D-68·D-70·D-72~D-76·D-84·D-86](decisions.md)을 따른다.
승인된 특정 패키지·버전·용도의 예외를 라이선스 전체의 허용으로 넓히지 않는다. 자체 공개 라이선스는 O-27로 남아 있다.

- 직접·전이 의존성, 복사한 코드, WASM·폰트·아이콘, 제품 이미지에 포함된 구성요소를 확인한다.
- 허용 목록 밖의 라이선스나 이용 조건은 대안과 함께 보고한다. 조용히 대체하거나 검사 예외를 추가하지 않는다.
- 이중 라이선스의 선택, 오픈소스 코어와 유료 기능, 별도 서비스와 앱에 포함되는 코드의 조건을 구분한다. Docker 분리만으로 의무가 없어지지 않는다.
- 설치 버전을 고정하고 의존성 변경 시 `cargo xtask licenses`를 실행한다. cargo-deny와 `pnpm licenses list --json`을 사용하는 기존 xtask가 검사 도구다(O-28 해소).
- 필요한 저작권·라이선스 원문·NOTICE를 배포물에 보존한다. 라이선스·보안 권고 예외의 실제 설정은 다음 절의 파일에서 확인한다.

<a id="checks"></a>

## 8. 검사 명령과 설정 위치

도구 버전과 공통 실행 명령은 [README](../README.md)가 정본이다. `check`는 포맷·lint·타입·단위 시험·Storybook 빌드·라이선스·생성 계약을,
`test`는 DB 등 서비스가 필요한 통합 시험을 담당한다. API 변경 시 계약과 클라이언트를 함께 생성하고,
의존성 변경 시 라이선스를 확인한다. rhwp는 WASM 생성·고지 확인과 최신 보안 권고 재검사를 구분한다.

`check`는 외부 서비스를 띄우지 않는다. 통합 시험은 `DATABASE_URL`이 없다고 조용히 건너뛰지 않고 실패한다.
`test`에서 선택하지 않은 OIDC·Supabase 시험과 외부 서비스 모드의 S3 미설정은 NOT RUN이다.
Supabase session 실행에서도 별도 빈 DB를 만드는 시험은 직접 연결을 사용한다. 실제 Cloud 검증과 같지 않다.

| 설정                     | 정본                                                                                                            |
| ------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Rust 버전·lint·시험 예외 | [rust-toolchain.toml](../rust-toolchain.toml), [Cargo.toml](../Cargo.toml), [clippy.toml](../clippy.toml)       |
| TypeScript·Svelte·ESLint | [tsconfig.base.json](../tsconfig.base.json), [eslint.config.mjs](../eslint.config.mjs)                          |
| 포맷과 생성물 제외       | [rustfmt.toml](../rustfmt.toml), [.prettierrc.json](../.prettierrc.json), [.prettierignore](../.prettierignore) |
| 기본 통합·개발 Compose   | [compose.test.yaml](../deploy/compose.test.yaml), [compose.dev.yaml](../deploy/compose.dev.yaml)                |
| Supabase 고정 구성       | [연결·시험 안내](self-hosting-supabase.md), [compose.test.yaml](../deploy/supabase/compose.test.yaml)           |
| 검사 순서와 대상         | [xtask/src/check.rs](../xtask/src/check.rs), [xtask/src/test.rs](../xtask/src/test.rs)                          |

### DB 비교와 사전 검사 fixture

통합 SQL 비교 결과와 고정 검증 SHA의 재현은 [마이그레이션 안내](../crates/server/migrations/README.md#comparison)를 따른다.
이번 통합 전용 비교 도구는 현재 트리에서 제거했다. 제품 사전 검사 fixture는
[crates/server/tests/fixtures/postgres-preflight/](../crates/server/tests/fixtures/postgres-preflight)에 둔다.
PG16은 지원 대상이 아니라 `db bootstrap`·`db migrate`의 변경 전 거부를 확인하는 fixture다.
새 전용 Compose 프로젝트에 `deploy/compose.test.yaml`과 `crates/server/tests/fixtures/postgres-preflight/compose.pg16.yaml`을 겹쳐 실행한다.
거부 전후에 `crates/server/tests/fixtures/postgres-preflight/catalog.sql`을 조회해 역할·스키마·원장·권한이 그대로인지 비교한다.
PG17 필수 언어·UUID 함수 권한의 거부 검사는 PG16 override 없이 같은 격리 방식으로 실행하고,
시험 fixture에서만 권한을 하나씩 철회·원복한다. 비밀번호 보존은 잘못된 비밀번호의 거부와 원래 비밀번호의 새 연결을 함께 확인한다.

구체적인 거부 조건·명령은 [고정 SHA의 재현 절차](https://github.com/fvoci/GroundZero/blob/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation/verification/fixtures/README.md)에 있다.
그 절차의 `compose.pg16.yaml`·`preflight-catalog.sql` 경로는 위 정식 경로를 사용하고, 성공 후 이력은 현재 SQLx 기준과 대조한다.
기존 개발·Cloud DB에 이 fixture를 실행하지 않는다. 출력은 새 `target/` 경로에 남기고 생성한 프로젝트만 `stop`한다.

<a id="license-records"></a>

## 9. 라이선스·고지 유지보수

Rust는 [deny.toml](../deny.toml), npm은 [xtask/src/licenses.rs](../xtask/src/licenses.rs),
HWP·HWPX 전용 빌드는 [deploy/rhwp/deny.toml](../deploy/rhwp/deny.toml)에서 승인 범위를 검사한다.
고지는 [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt)와 [rhwp 고지](../deploy/rhwp/THIRD_PARTY_NOTICES.txt)에 있다.
구성요소·폰트 출처는 [UI README](../apps/web/src/lib/ui/README.md)와 고지를 함께 본다.

자체 크레이트에는 임의의 공개 라이선스를 넣지 않는다. `publish = false`와 cargo-deny의 private 제외는 내부 검사 설정이며,
공개 라이선스 결정이 아니다. 새 보안 권고는 해당 의존성과 승인된 예외 범위로 판단하고 과거 PASS를 현재 결과로 대신하지 않는다.
과거 도입 조사·거절·승인 전 목록은 [당시 원문](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#9-라이선스-승인-요청-2026-10-17-초기화-o-30)이다.

<a id="ci"></a>

## 10. CI 구조 (D-64)

[base-image.yml](../.github/workflows/base-image.yml)은 [개발 Dockerfile](../deploy/docker/dev.Dockerfile)로
공통 검사 이미지 `ghcr.io/fvoci/groundzero-ci-base`를 만든다. Dockerfile·워크플로 변경, 주간 스케줄, 수동 실행 때 갱신하며
제품 커밋마다 만들지 않는다. 새 digest를 `CI_BASE_IMAGE_DIGEST`에 반영한다. 변수가 없으면 검사 job은 실패한다.

[ci.yml](../.github/workflows/ci.yml)의 구성은 다음과 같다.

| job           | 명령·환경                                                                          |
| ------------- | ---------------------------------------------------------------------------------- |
| `check`       | 의존성 설치 후 `cargo xtask check --skip-licenses`                                 |
| `licenses`    | 의존성 설치 후 `cargo xtask licenses`                                              |
| `integration` | PG17·Meilisearch, `cargo xtask test --external-services`                           |
| `oidc`        | PG18·Meilisearch·SeaweedFS·Keycloak, `cargo xtask test --external-services --oidc` |
| `supabase`    | 공식 PG17 구성, `cargo xtask test --supabase --session-pooler --oidc`              |
| `rhwp`        | `cargo xtask rhwp`, 고지 차이 검사, `cargo xtask rhwp --audit`                     |

`check`·`licenses`·`integration`은 공통 digest의 컨테이너 job이다. `oidc`의 시험과 `rhwp --audit`도 같은 이미지를 쓴다.
OIDC 서비스는 호스트의 격리 Compose에서 띄우고 시험 컨테이너에 Docker 소켓을 넘기지 않는다.
Supabase는 기동·실패 진단에 호스트 Docker가 필요하므로 D-88에 따라 호스트 러너에서 같은 고정 툴체인으로 실행한다.

외부 서비스 모드는 `FVOCI_TEST_PG_HOST_PORT`와 `FVOCI_TEST_SEARCH_URL`을 요구하며 `--oidc`에는
`FVOCI_TEST_OIDC_ISSUER`도 필요하다. xtask가 테스트 자격 증명으로 세 DB 역할 주소를 만든다.
`FVOCI_TEST_S3_ENDPOINT`가 없으면 S3 시험을 NOT RUN으로 표시한다. 임의의 운영 DB 주소를 넘기는 방식이 아니다.

실행마다 프로젝트·DB·볼륨을 분리한다. 캐시는 속도용이며 없어도 같은 검사가 가능해야 한다.
현재 통합 시험은 in-process라 제품 바이너리를 소비하지 않는다. 별도 e2e 산출물 공유나 job 분할은 실제 필요와 병목을 확인한 뒤 정한다.

### 제품 이미지 릴리스

[release-image.yml](../.github/workflows/release-image.yml)은 D-87의 `ghcr.io/fvoci/groundzero:X.Y.Z`를 게시한다.
`latest`는 없고 linux/amd64·linux/arm64를 각 네이티브 러너에서 빌드·확인한다.

- 원격 `vX.Y.Z` 태그 push 또는 기존 태그를 지정한 수동 실행이 진입점이다. 태그의 형식·원격 존재·main 포함 여부를 먼저 확인한다.
- 같은 이미지 태그가 있으면 소스 SHA가 같을 때만 그대로 두고 다르면 실패한다. 두 플랫폼 검증을 통과해야 manifest에 태그를 붙인다.
- 제품 버전·전체 소스 SHA를 빌드 인자와 OCI 라벨에 같이 넣는다. 라벨·`fvoci-server version`·시작 로그·`/healthz`의 `build`가 같아야 한다. 값 없는 개발 빌드는 `dev`다. 주입 전 태그 `v0.0.1`의 라벨만 확인하는 예외는 D-87 그대로다.
- PR에서는 게시 권한 없이 빌드·검증한다. 권한·네트워크 오류를 이미지 없음으로 처리하지 않는다. 공개 pull을 위한 GHCR 패키지 공개 설정은 소유자가 한다.
- 태그 생성·이동·main 병합·배포 승인과 D-92의 `v1.0.0` 원격 게시 경계는 [결정 문서](decisions.md)를 따른다.

<a id="ui"></a>

## 11. 프론트엔드 UI

Svelte·Vite·TypeScript 구성에서 shadcn-svelte/Bits UI와 Tailwind를 사용한다(D-66).
공통 컴포넌트의 출처·도입 명령·수정 범위는 [UI README](../apps/web/src/lib/ui/README.md),
디자인 토큰·서체는 [app.css](../apps/web/src/styles/app.css), 승인 범위는 D-86이 정본이다.
앱 고유 코드는 `apps/web/src/lib/features/<기능>/`에 둔다.

기본 동작·접근성·키보드 처리는 제공 구현을 우선 쓴다. 폰트·색·간격·포커스는 공통 토큰으로 관리하고 별도 종합 UI 시스템이나
범용 폼·모달·포커스 관리자를 만들지 않는다. 라이브러리 사용을 실제 접근성 검증으로 대신하지 않는다.
Safari 키보드 접근성은 D-98의 의미에 맞는 요소와 최소 범위 lint 예외를 따른다.

<a id="storybook"></a>

### 11.1 Storybook

실제 제품 컴포넌트의 상태(비활성·오류·저장 중·충돌 확인·긴 한국어 등)를 합성 데이터로 보고 다듬는 곳이다(D-108).
Storybook 10.6.3(`@storybook/svelte-vite`)과 기본 기능만 쓰며 addon을 더하지 않았다.

```sh
pnpm --filter @fvoci/web run storybook        # http://127.0.0.1:6006 (로컬 접속만)
pnpm --filter @fvoci/web run build-storybook  # target/storybook-static
```

- story는 `apps/web/stories/<기능>/*.stories.ts`, 합성 데이터·조립 예는 `stories/fixtures/`에 둔다. 제품 컴포넌트를 그대로
  import하고 Storybook용 복제·별도 CSS·제품 코드의 Storybook 분기를 만들지 않는다. 대화상자처럼 조립이 필요한 것만
  제품 화면과 같은 방식으로 조립한다.
- `.storybook/preview.ts`가 제품의 `app.css`·서체·번역을 쓰고, 표시 설정(한국어·Asia/Seoul)을 고정한다. 도구 막대로 언어,
  글자 크기(루트 글꼴 100·150·200%, 브라우저 설정의 흉내), 375·500px·데스크톱 화면 폭을 바꾼다.
- 합성 데이터는 [dev-ui-data.json](../deploy/postgres/dev-ui-data.json) 하나를 개발 seed와 함께 쓴다. story(`stories/fixtures/synthetic.ts`)와
  `cargo xtask dev`의 "UI 상태 예제" 워크스페이스는 문서 제목·본문·트리, 댓글 스레드·답글·해결, 업무 제목·상태·우선순위·담당자가
  같다. id·시각은 DB가 정하므로 story는 고정값을 쓰고, 사람은 개발 Keycloak 테스트 사용자 둘(이름은 `tests/synthetic-data.test.ts`가 대조)뿐이다.
- 서버 데이터가 필요한 story는 `api`의 함수를 story의 `beforeEach`에서 바꿔 끼운다(`spyOn`). 같은 원점의
  `/api`·`/auth`·`/healthz`와 다른 원점 요청은 막고 콘솔 오류로 남긴다. 실제 OIDC·DB·운영 API에 연결하지 않는다.
- `tests/component/stories.test.ts`가 모든 story를 jsdom에서 다시 그리고 play 함수(키보드·실패·입력 보존)를 실행한다.
  jsdom은 배치·초점 가두기·애니메이션을 계산하지 않으므로 표시·줄바꿈·넘침·대화상자 초점 가두기는 Storybook을 실제
  브라우저(창이 보이는 상태)로 열어 확인한다. 자동 검사가 화면 낭독기·키보드 수동 검증을 대신하지 않는다.
- 로그인·API·DB 저장·공동 편집 흐름은 Storybook 범위가 아니다. 실제 브라우저 회귀 시험은 #118에서 다룬다.
- Svelte 전용 story 형식(`@storybook/addon-svelte-csf`)은 Vite 설정에 플러그인을 더해야 해 쓰지 않는다. 접근성 자동 검사
  addon(`@storybook/addon-a11y`)은 axe-core(MPL-2.0)가 허용 목록 밖이라 도입하지 않았다.

공개 배포는 Cloudflare Workers의 정적 자산 Worker `fvoci-storybook`이다([wrangler.jsonc](../apps/web/wrangler.jsonc)).
Storybook에는 합성 데이터만 있으며 workers.dev 주소는 누구나 볼 수 있다. Workers Builds가 main 커밋은 배포하고, 다른 브랜치·PR은
미리보기 주소를 PR에 댓글로 남긴다. 저장소 연결은 소유자가 Cloudflare 대시보드에서 한다.

| Workers Builds 설정 | 값                                                                                                                      |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Root directory      | `/` (pnpm 작업 공간 전체 설치)                                                                                          |
| Build command       | `pnpm --filter @fvoci/web run build-storybook`                                                                          |
| Deploy command      | `pnpm dlx wrangler@4.151.3 deploy --config apps/web/wrangler.jsonc`                                                     |
| Preview command     | `pnpm dlx wrangler@4.151.3 preview --config apps/web/wrangler.jsonc`                                                    |
| Build variable      | `NODE_VERSION=24.22.1` (pnpm 판은 `packageManager`를 따른다)                                                            |
| Build watch paths   | `apps/web/*`, `packages/*`, `deploy/postgres/dev-ui-data.json`, `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml` |

Wrangler는 빌드 환경에서만 받아 쓰고 작업 공간 의존성·라이선스 검사 대상에 넣지 않는다.

<a id="development"></a>

## 12. 개발 실행

### 12.1 실행 구성

```sh
pnpm install --frozen-lockfile
cargo xtask dev --oidc
```

서비스·Rust 서버·Vite가 함께 시작한다. 브라우저에서 `http://127.0.0.1:5173`을 연다(`localhost`와 구분).
`--no-web`이면 웹만 다음 명령으로 따로 시작한다.

```sh
VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev
```

- DB는 `db bootstrap`(관리자 URL) → `db migrate`(migrator URL) → 서버(`fvoci_app`) 순서로 준비한다. 환경 변수는 [.env.example](../.env.example), 운영 문제 해결은 [DB 준비 안내](self-hosting.md#db-setup)를 본다.
- 서버 8080·웹 5173·개발 Keycloak 8180이 고정이므로 한 컴퓨터에서 개발 환경은 한 번에 하나만 띄운다. Keycloak 포트는 `FVOCI_DEV_KEYCLOAK_PORT`로 바꿀 수 있지만 발급자가 바뀌면 기존 개발 사용자와 다른 신원이 된다.
- `test-user`는 개발 워크스페이스 소유자, `test-user-2`는 작성자다. 비밀번호는 [realm-test.json](../deploy/keycloak/realm-test.json)의 `credentials`에 있다. Keycloak 관리자 비밀번호는 [compose.dev.yaml](../deploy/compose.dev.yaml)에만 둔다.
- 공동 편집은 두 사용자를 서로 다른 브라우저 프로필로 열어 확인한다. 같은 브라우저의 두 탭은 쿠키를 공유하므로 서로 다른 사용자가 아니다.
- 화면 캡처는 데스크톱 창이 있는 브라우저에서 앞에 띄운 탭으로 만든다. 창 없는 헤드리스·원격 브라우저 런타임은 페이지가 정상이어도 단색 이미지를 돌려줄 수 있다([#112](https://github.com/fvoci/GroundZero/issues/112)). 만든 이미지 파일을 직접 열어 확인하고 DOM·HTTP·DB 결과로 픽셀 확인을 대신하지 않는다. 좁은 창은 실제 기기 시험과 구분해 적는다.
- [dev-seed.sql](../deploy/postgres/dev-seed.sql)·[dev-module-seed.sql](../deploy/postgres/dev-module-seed.sql)·[dev-ui-seed.sql](../deploy/postgres/dev-ui-seed.sql)은 기존 앱 함수를 사용하고 새 ID는 xtask에서 받는다. 개발 워크스페이스·모듈 예제·UI 상태 예제는 합성 데이터이며 운영 설치에는 들어가지 않는다. UI 상태 예제는 Storybook과 같은 [dev-ui-data.json](../deploy/postgres/dev-ui-data.json)으로 만든다([Storybook](#storybook)).
- seed 표시는 반복 실행의 중복과 사용자 편집 덮어쓰기를 막는다. 예제만 지우려면 앱에서 예제 워크스페이스를 삭제한다. 다음 개발 실행이 다시 만들지 않는다. 발급자·고정 사용자 ID가 기존 DB와 다르면 준비를 중단한다.
- Ctrl-C는 데이터를 남긴다. `cargo xtask down`은 이 체크아웃의 개발·시험 컨테이너·네트워크·DB·검색·Keycloak 볼륨을 지운다. `target/dev-files`의 첨부는 남는다. 기존 데이터 삭제는 별도 확인 범위다.
- `dev`의 Compose 실행은 정의가 바뀐 컨테이너를 다시 만들 수 있다. 예전 개발 상태를 보존해야 하면 먼저 [당시 Keycloak 저장 위치와 보존 절차](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#84-개발-keycloak-컨테이너가-실수로-다시-만들어진-일-2026-10-20)를 확인한다. 기존 DB의 checksum 불일치는 볼륨 삭제로 자동 해결하지 않는다.

개발용 rhwp 파일이 필요하면 `cargo xtask rhwp`로 제품과 같은 WASM·고지 생성 단계를 실행한다.
운영 설치·업그레이드·백업은 [셀프호스트 안내](self-hosting.md), 별도 DB 구성은 [Supabase 안내](self-hosting-supabase.md)를 따른다.

<a id="contracts"></a>

## 서버·클라이언트 유지보수 계약

아래는 확정 결정을 구현하는 현재 경계다. API의 전체 필드는 생성된 OpenAPI, 권한은 실제 호출 경로와 관련 시험을 함께 확인한다.
여기 적힌 구현·제한이 새 정책 승인이나 모든 환경의 검증 완료를 뜻하지 않는다.

<a id="sql-ids"></a>

### SQL·UUID

새 행의 UUIDv7은 Rust 앱이 만들고 SQL은 받은 값을 소비한다(D-88). 기존 대상 ID·요청 ID와 새 행 ID를 구분한다.
개수 산정·바인딩 순서·FV001/002·HTTP와 작업 프로세스의 재시도 차이는 [Rust→SQL UUID 인자 계약](sql-id-contract.md)을 따른다.
[통합 초기 마이그레이션](../crates/server/migrations/README.md)이 제품 적용 경로다(D-107).
기존 누적 이력 DB는 자동 이전하지 않는다. 정확한 대상·손실 영향·백업 방안을 확인하고 승인받은 뒤 재생성한다.
통합 당시 비교는 [고정 검증 SHA의 별도 checkout](../crates/server/migrations/README.md#comparison)에서 재현한다.
빈 DB 설치 동등성을 기존 데이터 보존·구버전 백업 복구 검증으로 보지 않는다.

<a id="create-retry"></a>

### 만들기 요청의 재시도

일반 만들기의 `RetryKey`는 같은 입력을 다시 보낼 때 같은 요청 ID를 쓰고 입력을 고치면 새 ID를 쓴다.
서버의 멱등 처리는 요청 내용 비교가 아니라 기존 결과 반환이다. [RetryKey 구현](../apps/web/src/lib/api/retry-key.ts)을 따르되,
행별 적용 상태와 기존 업무의 내용 차이를 다루는 [CSV 업무 만들기](#task-create-contract)의 별도 계약을 우선한다.

<a id="document-access"></a>

### 문서 접근·보관·참조

읽기·쓰기와 공유 관리·복제·내려받기를 구분한다. 항목·모듈을 보관하면 문서 접근은 `read`까지로 낮아지지만,
기존 관리자는 노출을 줄이는 비공개 전환·복제 금지·공유 및 공개 링크 회수를 할 수 있다.
설정으로 공개 범위나 복제 허용을 넓히면 409이며, 새 공유는 기존 쓰기 권한 검사에서 403이다.
설정 요청에서 빠진 칸은 잠근 행의 현재 값을 쓴다. 오래된 폼의 다른 칸을 함께 덮어쓰지 않는다.

본문의 내부 참조는 종류와 ID만 저장한다. 제목·표시 이름은 조회 때 권한을 확인해 해석하며,
볼 수 없는 대상과 없는 대상은 같은 `visible: false` 결과다. 서버가 본문에서 참조를 추출하고,
새로 추가한 사용자 언급만 알린다. 언급은 접근 권한을 주지 않으며 알림도 현재 문서 권한을 다시 따른다.
복제·복원으로 옛 언급을 다시 알리지 않는다. [참조 처리기](../crates/server/src/features/references.rs),
[참조 시험](../crates/server/tests/references.rs), [보관·설정 시험](../crates/server/tests/modules.rs)을 함께 본다.

본문 이미지의 `src`는 표준형 내부 첨부 URL만 허용한다. 링크는 `http`·`https`·`mailto`와 상대 경로·앵커를 허용하며,
저장·복원·실시간 변경에 같은 본문 검사를 적용한다.
실시간 변경은 임시 문서에서 검사한 뒤 방 상태에 적용하고 방송한다. 잘못된 변경은 4400으로 닫으며,
읽기 전용 연결의 변경은 무시한다. 실시간 사용 중 REST 저장은 제목 변경만 허용하고 본문 변경은 409다.
[본문 검사](../crates/server/src/features/documents.rs), [템플릿 시험](../crates/server/tests/templates.rs),
[실시간 시험](../crates/server/tests/collab.rs)을 함께 확인한다.

<a id="document-tree"></a>

### 문서 트리와 자리 이동

트리의 부모는 같은 자리(워크스페이스·항목·개인 소유자)에 있어야 한다. 부모 변경은 권한을 바꾸지 않으며,
옮길 문서의 쓰기 권한과 새 부모·기준 문서의 읽기 권한을 검사한다. 보관 중에는 바꿀 수 없다.
같은 자리의 트리 잠금 안에서 깊이·순환·현재 자리를 확인한다. 볼 수 없는 부모 아래의 볼 수 있는 문서는 조회에서 최상위로 보인다.
문서를 처음 만들 때의 `parent_id`는 같은 생성 요청을 재시도할 때 트리를 다시 옮기는 명령이 아니다.

다른 자리로 문서 한 건을 옮기면 직계 자식은 옛 부모 아래에 순서를 유지해 남고, 옮긴 문서는 새 자리의 최상위 끝에 놓인다.
노트→워크스페이스와 워크스페이스 간 이동을 지원하며, 워크스페이스→개인 노트 이동은 409다(복제는 별도 권한으로 판단).
노출 변화 미리보기의 확인 값을 잠근 뒤 다시 계산해 비교한다. 같은 모듈 안 이동은 번호를 유지하고,
번호 범위를 벗어나면 새 번호를 받으며 옛 번호를 재사용하지 않는다.
첨부 파일·관련 휴지통 묶음·열린 업로드의 자리도 함께 옮긴다. 실시간 편집 보존은 [별도 계약](#live-document-preservation)을 따른다.
[문서 처리기](../crates/server/src/features/documents.rs), [트리 시험](../crates/server/tests/tree.rs),
[모듈 경계 시험](../crates/server/tests/modules.rs)이 근거다.

<a id="revision-restore"></a>

### 리비전 복원과 재시도

복원은 옛 이력을 지우지 않고 `restored_from`을 기록한 새 리비전을 만든다. 기준 리비전 확인 뒤 현재 제목·본문·스키마와
완전히 같으면 쓰지 않고 성공한다. 현재 스키마와 맞지 않는 복원은 409이며, 옛 내용도 현재 제목·본문 크기·참조 수 제한을 검사한다.
외부 업무 상태·개인 이수·진도를 되돌리는 명령은 아니다(D-58).

문서·작성자 범위에서 같은 `request_id`로 같은 원본을 복원한 결과가 아직 최신이면 성공하고,
그 뒤 저장이 있었다면 최신본과 409다. 저장과 복원 사이 또는 다른 원본 복원에 같은 요청 ID를 재사용하면 400이다.
복원은 참조 기록을 갱신하지만 언급 알림을 다시 만들지 않는다. HTTP 권한 검사부터 실시간 체크포인트·DB 함수까지의
전체 경로는 [실시간 편집 보존](#live-document-preservation), [복원 처리기](../crates/server/src/features/revisions.rs),
[복원 시험](../crates/server/tests/revisions.rs)을 함께 확인한다.

<a id="trash-contract"></a>

### 휴지통·복원·영구 삭제

문서는 소유 권한자 또는 쓰기 권한이 있는 작성자가 지운다. 함께 지울 수 있는 하위 문서만 같은 묶음에 넣고,
권한이 없는 하위 문서는 옛 부모 아래에 남긴다. 삭제된 대상은 일반 조회·참조·검색·공개 링크에서 보이지 않는다.
휴지통은 관리자에게 공간의 묶음, 쓰기 역할에는 본인이 지운 묶음만 보인다. 개인 노트 묶음은 소유자만 보며 공간 휴지통과 섞지 않는다.

복원·영구 삭제는 묶음 단위다. 복원할 부모 문서·업무가 따로 삭제됐다면 최상위로 돌아온다.
모듈·항목·첨부의 원본 문서가 삭제됐다면 상위 대상을 먼저 복원해야 하고, 보관 항목으로는 복원하지 않는다.
지운 사람의 복원은 지금도 항목과 첨부의 문서에 쓸 수 있어야 한다. 모듈 복원과 공간의 직접 영구 삭제는 관리자만 할 수 있다.

공간 보존 기간의 기본값은 D-41의 30일이며 설정은 1~3650일이다. 바꾼 기간은 이미 지운 묶음에도 적용한다.
개인 노트는 30일이다. 만료 뒤 복원은 409지만, 살아 있는 기록·문서의 파일 버전 참조 때문에 자동 삭제를 미룬 묶음은
D-69에 따라 복원할 수 있다. 자동 삭제는 참조 버전을 보호하며(D-56), 관리자의 직접 영구 삭제는 별도 동작이다.
대상을 영구 삭제해도 개인 시간 기록 자체는 지우지 않고 대상 이름을 가린다.
[휴지통 시험](../crates/server/tests/trash.rs)과
[통합 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `app_restore_batch`·`app_purge_batch_as`를 함께 확인한다.

<a id="attachment-retention"></a>

### 첨부의 이전 버전 보존

이전 파일 버전의 보존 기간은 그 버전을 처음 올린 때가 아니라 다음 버전으로 교체된 때부터 센다.
문서의 버전 참조는 같은 워크스페이스 안에서만 자동 삭제를 막으며, 개인 파일은 같은 소유자의 노트만 보존 근거가 된다.
다른 공간·다른 사람의 문서에 주소를 적는 것으로 남의 버전을 붙잡을 수 없다. 문서를 옮기면 현재 자리를 기준으로 다시 판단한다.
D-56의 참조 보호와 D-69의 만료 복원은 [휴지통 계약](#trash-contract)을 함께 따른다.
기록을 수정할 때 이미 연결된 휴지통 파일·버전은 유지할 수 있지만 새로 연결할 수는 없다.
영구 삭제된 파일의 이름·버전·SHA-256·크기 연결은 수정 요청에 빠져도 보존한다.
[기록·버전 보존 시험](../crates/server/tests/records.rs)이 교체 시점과 참조 범위를 확인한다.

<a id="upload-completion"></a>

### 업로드 완료와 재시도

완료는 모든 조각을 합쳐 전체 크기·SHA-256을 확인하며 중복 요청도 한 번만 기록한다.
완료 작업은 HTTP 요청과 분리되어 연결이 끊겨도 계속한다. 웹은 `completing`이면 다시 올리지 않고 결과를 기다린다.
일시적 읽기·DB 오류 뒤에는 완료 작업 안에서 자신의 점유만 풀어 다음 완료가 이어받게 한다.
해제 실패 시에는 기존 5분 뒤 승계 경로가 남는다. 점유는 시도마다 구분하여 늦은 이전 시도가 새 점유를 풀거나
업로드를 되돌림·종료·기록하지 않는다.

로컬 저장소는 시도별 준비 파일의 바이트를 검증한 뒤, 점유 확인과 행 잠금을 유지한 채 최종 키에 게시하고 DB에 기록한다.
점유를 잃은 시도는 게시하지 않는다. 게시 뒤 일시적 DB 충돌에는 닫지 않고 다음 완료가 최종 키를 확인해 기록한다.
지속되는 권한·대상·저장량 오류는 닫고 정리한다. 조각 합치기 실패는 받은 조각을 비워 다시 받고 웹은 한 번 더 보낸다.
준비 파일은 게시·취소·만료 때 조각과 함께 정리한다. S3는 완료된 multipart를 다시 완료할 수 없으므로
로컬 준비 파일 방식을 그대로 적용하지 않는다.

[파일 처리기](../crates/server/src/features/files.rs), [통합 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `app_release_upload_claim`,
[저장소](../crates/server/src/storage), [업로드 클라이언트](../apps/web/src/lib/features/files/upload.ts),
[파일 시험](../crates/server/tests/files.rs)을 함께 확인한다.

<a id="item-membership"></a>

### 항목 멤버십과 리더

항목 리더는 표시용이며 권한을 주지 않는다. 비공개 항목 멤버십도 워크스페이스 역할을 넘지 않아 독자에게는 읽기만 허용한다.
항목 멤버 관리는 소유 권한자가 한다. 워크스페이스를 떠나면 직접 항목 멤버십을 지우며 다시 초대돼도 예전 비공개 항목 접근은 돌아오지 않는다.
담당·리더 정리는 휴지통에 있다는 이유만으로 실행하지 않는다. 실제 접근 권한을 잃거나 공간을 떠났을 때는 휴지통 항목에서도 정리한다.
[통합 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `app_item_access_ignoring_trash`·`app_drop_unseen_assignees`,
[항목 권한 시험](../crates/server/tests/item_profile.rs), [휴지통 시험](../crates/server/tests/trash.rs)을 함께 본다.

<a id="item-copy"></a>

### 항목 복제와 재사용

항목 복제는 같은 워크스페이스의 같은 종류이며 보관되지 않은 대상 모듈에만 한다. 원본을 볼 수 있고 대상에 항목을 만들 수 있어야 한다.
같은 요청 ID는 같은 복제본을 돌려준다. 새 항목은 비공개이며 복제한 사람이 작성자다. 원본 권한·멤버·리더·담당자·반복 규칙,
기록·첨부 파일·개인 진도는 복사하지 않는다. 항목 정보·업무와 하위 업무·체크리스트·선행 관계·허용된 문서 트리·강좌 회차·스터디 단위를 복사하고,
업무·문서는 새 번호, 문서는 새 리비전 1로 시작한다. 소유 권한이 없고 복제도 허용되지 않은 문서는 건너뛰고 수를 알린다.
본문의 첨부 주소는 원본 파일을 가리키므로 원본 접근 권한이 없으면 복제본에서도 보이지 않는다.

기본 `reset=true`는 업무 상태를 할 일, 항목 상태를 계획으로 돌리고 날짜·기간·체크 표시와 강좌·스터디 날짜를 비운다.
`reset=false`는 이 값들을 유지한다. 템플릿에서 만들기는 새로 시작하는 복제를 사용한다.
[항목 처리기](../crates/server/src/features/item_profile.rs)와 [복제 시험](../crates/server/tests/item_profile.rs)을 함께 본다.

<a id="time-sharing"></a>

### 시간 집계의 비공개 경계

업무·항목의 시간 합계는 끝난 기록만 센다. 진행 중인 다른 사람의 타이머로 시작·멈춤 시각을 드러내지 않는다.
워크스페이스를 떠나면 시간 공유 선택을 지워 다시 가입해도 확인 없이 공유하지 않는다.
[시간 처리기](../crates/server/src/features/time_tracking.rs), [시간 공유 시험](../crates/server/tests/time_tracking.rs)을 함께 본다.

<a id="user-export"></a>

### 사용자 ZIP 반출과 운영자 백업

사용자 ZIP 반출은 요청자의 권한으로 읽는다. 공간 반출은 관리자만 할 수 있으며 멤버의 개인 노트를 포함하지 않는다.
구조 데이터와 파일 목록은 같은 스냅숏으로 읽고, 전송 중 영구 삭제된 파일은 `missing.json`에 표시한다.
전체 개인 자료를 포함하는 [운영자 백업](self-hosting.md#5-백업과-복구)과 구분한다.
[반출 처리기](../crates/server/src/features/export.rs), [반출 시험](../crates/server/tests/export.rs)을 함께 본다.

<a id="research-import"></a>

### Zotero 문헌 가져오기

Zotero 키는 가져오는 요청 동안만 서버가 API 헤더로 전달하며 URL·저장 데이터·로그에 넣지 않는다. 화면도 입력 칸에만 두고 닫으면 지운다.
같은 Zotero 키의 문헌을 다시 가져오면 서지만 갱신하고 사용자 작성 칸·본문을 유지한다. 요약은 비었을 때만 채우고 태그는 합친다.
가져오기는 연구 항목의 쓰기 권한을 요구하며, 첨부·메모·PDF·주석 가져오기와 Zotero 쓰기는 포함하지 않는다(D-34·X-05).
[Zotero 처리기](../crates/server/src/features/zotero.rs)와 [가져오기 시험](../crates/server/tests/zotero.rs)이 키 수명과 병합 범위를 확인한다.

<a id="recurrence-contract"></a>

### 반복 업무·반복 일정

현재 규칙은 매일·매주(ISO 요일)·매달·매년, 간격 1~99, 끝 날짜 또는 첫 회차를 포함한 횟수 1~1000이다.
기준일은 규칙 요일에 없어도 첫 회차이며 끝 날짜보다 늦을 수 없다. 없는 날짜는 건너뛴다.
업무·일정은 같은 `app_recurrence_dates`로 회차를 계산한다. 날짜 범위는 1900~2200년이고 회차 단위로 펼친다.
이는 현재 구현 범위이며 RFC 5545 전체 지원을 뜻하지 않는다.

- **업무**: 회차마다 실제 행과 완료 상태를 둔다. 첫 회차가 틀이며 `PUT /api/tasks/{id}/recurrence`는 버전 없이 마지막 요청을 따른다. 미완료 회차를 남기고 다음 회차는 원래 일정대로 만든다(D-31). 새 회차는 틀의 현재 제목·설명·우선순위·태그·예상 시간·마일스톤과 날짜 간격을 쓰고, 체크리스트는 체크를 푼 채 복제한다. 담당자는 지금 항목을 볼 수 있는 멤버만 옮기며 하위 업무·선행 관계는 복제하지 않는다.
- 규칙·틀을 바꾸거나 반복을 멈춰도 이미 만든 회차는 바꾸지 않는다. 규칙 변경·재개는 규칙 시간대의 오늘부터(`effective_from`) 적용하며 횟수는 원래 기준일부터 센다. 반복이 켜진 틀은 하위 업무가 되거나 시작·마감 날짜를 모두 잃을 수 없다.
- 규칙 저장과 업무 수정 뒤의 회차 동기화는 별도 트랜잭션이다. 동기화가 실패해도 앞선 저장을 취소하지 않고 경고를 남겨 다음 동기화가 이어 한다. 쓰기 사용자가 항목을 열 때의 동기화 요청과 worker도 회차를 맞춘다. 오늘까지와 다음 한 회차를 반복 하나당 한 번에 최대 50개 만들며, 나머지는 다음 동기화에서 잇는다. 뿌리 업무 잠금과 `(series_id, occurrence_date)` 고유 제약으로 중복을 막는다. 독자의 동기화는 403, 보관 항목은 409다.
- **달력 읽기**: 아직 만들지 않은 업무 회차는 계산한 예정 표시일 뿐 행을 만들지 않는다. 누르면 틀을 열며 목록·칸반·간트에는 실제 회차만 나온다. 보관 항목의 예정 회차는 숨긴다.
- **일정**: 회차 행을 만들지 않고 달력에서 펼친다. 일정 시간대의 현지 시각을 유지하며 DST로 없는 시각은 건너뛰고 겹친 시각은 앞선 한 번만 쓴다(D-81). 전체 수정과 회차 제외를 지원하며 회차 하나의 독립 수정은 지원하지 않는다. 일정 날짜를 옮기면 제외 날짜도 함께 옮긴다.
- 일정의 규칙 저장에는 편집을 시작한 규칙 `base`를 보내며, 규칙 없음은 NULL이다. 규칙이 달라졌으면 쓰지 않고 409 `rule_conflict`와 최신 일정이다. 제목만 바뀐 것은 이 충돌이 아니고, 요청 규칙이 이미 현재 값이면 쓰지 않고 성공한다. 일정 시작·시간대를 바꿔 첫 회차가 `until` 뒤로 가는 요청은 400이다.

[반복 처리기](../crates/server/src/features/recurrence.rs),
[통합 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `app_sync_task_series`·`app_set_event_recurrence`,
[반복 시험](../crates/server/tests/recurrence.rs)을 함께 본다.

<a id="worker-contract"></a>

### 백그라운드 작업과 실패 경계

실행 역할·간격·작업 목록은 [설치·운영 안내](self-hosting.md#9-알아-둘-것), 구현은 [worker.rs](../crates/server/src/worker.rs)에 있다.
`scheduled_runs`와 `app_claim_schedule`이 이름별 실행 기회를 간격마다 하나만 배정한다. 배정은 작업과 별도 트랜잭션이므로
실패한 작업의 완료를 보장하거나 즉시 다시 배정하는 체계가 아니다. UUID 부족의 트랜잭션 재시도는 [SQL 인자 계약](sql-id-contract.md)을 따른다.
관리자의 만료 비우기와 쓰기 사용자의 항목 회차 동기화는 남아 있지만 알림·색인·저장소 정리를 대신하지 않는다.

작업은 사용자 문맥 없이 같은 `fvoci_app` 계정으로 정책에 묶인 `app_system_*` 함수를 부른다.
시스템 감사의 `actor`는 NULL, detail은 `system: true`이며 자동 휴지통 정리는 `reason: retention`이다.
worker가 만든 반복 회차의 작성자는 규칙 작성자다. HTTP 경로가 이 시스템 함수를 사용자 권한 우회용으로 호출하지 않는다.

반복 tick은 여전히 한 트랜잭션이며, 항목 순서로 각 항목의 열을 `done` → `in_progress` → `todo` 순으로 잠그고 끝까지 보유한다.
한 항목의 열 잠금 실패는 그 항목을 건너뛰고, 개별 반복 실패는 경고 후 나머지를 계속한다(`app_system_recurrence_tick`).
이를 항목별 부분 커밋으로 해석하지 않는다. P-35의 항목별 트랜잭션 제안은 보류 상태다.
[작업 시험](../crates/server/tests/worker.rs), [통합 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `app_system_recurrence_tick`,
[반복 시험](../crates/server/tests/recurrence.rs)이 이 경계를 확인한다.

<a id="live-document-preservation"></a>

### 실시간 문서 이동·복원의 편집 보존

D-95의 사용자 결정은 [결정 문서](decisions.md)에 있다. 다음은 그 구현 해석과 현재 남는 제한이다.

실시간의 연결됨·동기화됨은 DB 저장 완료가 아니다. 서버가 확인한 저장 스냅숏에 자신의 삽입·삭제가 모두 포함됐을 때만
그 편집의 미확인 상태와 초안을 해제한다([저장 확인 추적](../apps/web/src/lib/features/documents/live.ts)).

- 리비전 `author`는 저장 트랜잭션의 사용자다(`author = app_user_id()`). 그 사용자가 지난 리비전 뒤의 편집자에 없으면 실제 편집자를 `edited_by`에 남긴다. 편집자를 모르는 옛 저장 대기 상태는 빈 목록이다. 실시간을 끄는 관리자의 대신 저장에도 같다.
- 방 안 복원의 체크포인트 작성자는 고친 사람, 연결된 작성자 순으로 고른다. 연결된 복원 요청자도 대신 저장할 수 있으며 그때 실제 편집자를 구분한다.
- 방 없는 복원은 저장 대기 상태에 기록된 편집자 중 지금 쓸 수 있는 사람만 작성자로 삼는다. 복원 요청자가 대신 저장하지 않는다. 적합한 사람이 없으면 409 `invalid_state`로 거절하고 상태를 보존한다. 먼저 리비전을 남겼다면 기준 판이 지나 복원은 최신본을 포함한 409다. 방 없는 이동은 쓸 수 있는 편집자, 없으면 이동 요청자가 작성자다.
- 복원 처리기는 쓰기 권한을 먼저 확인해 기존 404·403으로 거절한다. 저장 없이 방을 닫는 `Rooms::reset` 경로는 사용하지 않는다. 방 없는 복원 중에는 `Rooms::hold`가 새 연결을 4503으로 막는다.
- 이동·복원의 `documents::LOCK_LIMIT` 10초는 잠금 문장마다 적용하는 `lock_timeout`이다. 넘으면 409 `invalid_state`다. 요청 전체의 제한 시간이 아니다.
- 이동이 잠금을 기다리는 동안에도 `FVOCI_COLLAB_RECHECK_SECS`(기본 30초)마다 세션과 옛 자리 권한을 다시 확인한다. 권한을 잃은 연결은 4403으로 닫고 그 연결의 대기 편집은 넣지 않는다.

제한을 다음과 같이 구분한다.

- 권한 철회 뒤 옛 권한으로 편집이 들어올 수 있는 창은 정상 재확인 때도 “재확인 간격 + 재확인 1회의 소요”다. 풀 연결 얻기 시간 초과(5초)나 DB 오류로 판단하지 못한 `Unknown` 회차에는 닫지 않고 다음 회차에 확인하므로 이 상한도 성립하지 않는다.
- 방 안 이동은 앞선 방 명령 대기, 체크포인트(최대 5초), 연결별 권한 확인과 트랜잭션 연결 얻기(각 풀 대기 최대 5초), 잠금들, SQL 실행, 커밋 뒤 리비전(최대 5초), `persist`, 다시 읽기를 거친다. 방 대기열·잠금 밖 문장 실행·`persist`에는 전체 시간 상한이 없다. 잠금 부분은 `10초 × (7 + L)`이며 기본 7개는 자리 2·문서 1·모듈/항목 4, `L`은 옮길 하위 문서·파일·업로드·휴지통 행 등에 따라 늘어난다. 정해진 부분의 `5 + 10 × (7 + L) + 5초`만 합쳐 전체 최대 시간이라고 보고하지 않는다.
- 0094 전의 편집자 미상 저장 대기 상태는 방 없는 복원이 409다. `author`가 여러 편집자 중 하나면 `edited_by`는 NULL이며 다른 편집자를 모두 기록하지 않는다.
- 이동 커밋과 `Rooms::moved` 사이에 새 자리의 실시간 끄기가 겹치는 틈은 완전히 닫히지 않았다. 이때 확인받지 못한 편집만 브라우저 초안으로 남는다.

유지보수할 때 [문서 처리기](../crates/server/src/features/documents.rs), [collab](../crates/server/src/collab),
[복원 시험](../crates/server/tests/revisions.rs), [이동 시험](../crates/server/tests/tree.rs), [실시간 시험](../crates/server/tests/collab.rs)을 함께 본다.
[원래 결정·제한 기록](https://github.com/fvoci/GroundZero/blob/0a26654db6c96ccfb232ecbf0a6a02ff8220a5dd/docs/decisions.md#L215-L225)도 보존되어 있다.

<a id="expected-user"></a>

### 저장 요청의 기대 사용자

D-96의 기대 사용자는 편집·초안을 시작한 사용자이며 인증·권한의 근거가 아니다. 실제 사용자는 세션으로 정한다.
클라이언트의 `/api/me` 사전 확인과 서버의 요청 검사를 함께 유지한다.

| 경로                                                                                                      | 기대 사용자와 거절                                                                                                                         |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 문서 저장 `PUT /api/documents/{id}`, 내 노트 만들기 `POST /api/me/notes`, 업로드 시작 `POST /api/uploads` | `x-fvoci-expected-user` 필수. 세션 없음 401, 머리 없음·중복·UUID 오류 400, 인증 사용자 불일치 409 `account_changed`. 처리기 쓰기 전에 판단 |
| 실시간 `GET /api/documents/{id}/collab`                                                                   | `expected_user` 질의 필수. 없음·형식 오류는 업그레이드 전 400. 불일치는 방에 들어가기 전 업그레이드 후 4412로 닫아 브라우저에 원인을 전달  |
| 업로드 조각·완료                                                                                          | 시작 때 비교하고 이후에는 업로드 소유자 권한으로 제한(다른 계정 404)                                                                       |
| 칸반 이동·리비전 복원·`RetryKey`를 쓰는 일반 만들기                                                       | 사용자 동작으로 보내며 브라우저 초안의 자동 재전송 경로가 아니므로 이 머리를 적용하지 않음. 기존 권한·판·중복 처리 계약 유지               |

HTTP 비교는 `ExpectedUser`와 `accounts::check_expected_user`가 맡는다. 웹은 열 때 고정한 사용자를 보내고,
409·4412에는 자동 전송·재연결을 멈춘다. 일반·실시간 문서 편집기는 입력을 원래 사용자의 초안 키로 보존하고 새 사용자 화면에서 읽지 않는다.
초안을 저장하지 못하면 다시 읽어 잃지 않도록 화면에 남겨 알린다. 떠날 때도 미확인 편집·제목을 같은 원칙으로 보호한다.
직접 내 노트 만들기의 홈 빠른 메모·새 노트 제목은 편집기 초안이 아니므로 계정 전환 뒤 다시 읽을 때 남지 않는다.

이미 열린 실시간 연결은 처음 세션에 묶여 있으며 다른 탭의 로그인으로 사용자가 바뀌지 않는다. 그 세션이 살아 있으면 계속 쓸 수 있고,
세션 종료는 주기 재확인 때 반영된다. 다시 붙기는 처음의 `expected_user`를 유지한다.
[expected_user.rs](../crates/server/tests/expected_user.rs)와 실시간·웹 계정 전환 시험이 적용 경계를 확인한다.

<a id="search-contract"></a>

### 검색 날짜·작성자

D-101·D-102를 따른다. 시각이 있는 대상은 UTC 초 `at`, 날짜만 있는 대상은 `date`로 색인하고 작성자 `created_by`도 필터 속성으로 둔다.
서버가 사용자의 시간대로 시작일 자정부터 끝 다음 날 자정 미만의 범위를 계산해 색인 검색과 대체 SQL에 같이 넘긴다.
날짜 전용 값은 날짜 그대로 비교한다. DST의 없는·겹친 자정은 기존 `app_local_instant` 규칙을 쓴다.

`tz`를 생략하거나 비우면 저장한 시간대, 그것도 없으면 UTC를 쓴다. 날짜 범위가 있을 때 시간대를 검사하며 알 수 없는 이름은 400이다.
시각 일정은 시작 시각 기준이다. 웹은 날짜를 고른 경우 설정 시간대를 보내며 경계를 직접 계산하지 않는다.

작성자는 색인 단계에서 거르지만 결과는 현재 호출자의 접근 권한으로 다시 검사한다(D-102).
0095는 전체 재색인 대기열을 한 번 채운다. 전환 중 아직 재색인되지 않은 대상은 새 필터에서 빠질 수 있고,
새 필터 설정 전의 검색 오류는 기존 제목 검색 대체 경로를 따른다. 이를 기존 데이터의 재색인 실행 검증 완료로 보지 않는다.

<a id="version-contract"></a>

### 저장한 보기·항목 정보의 판과 보관 댓글

저장한 보기는 개인 소유여서 관리자에게도 공유되지 않으며, 해당 항목에 접근할 수 없으면 읽거나 수정할 수 없다.

- **보기 만들기(D-103)**: `POST /api/items/{item_id}/views`는 새 보기만 만든다. 같은 생성 요청 ID의 재시도는 처음 만든 보기의 현재 값을 201로 돌려준다. 다른 생성 요청이 같은 이름으로 겹치면 기존 보기를 덮지 않고 409 `exists`와 현재 보기다.
- **보기 수정(D-103)**: `PUT /api/views/{id}`는 `base_version`이 필수다. 잠근 행에서 요청 ID와 판을 검사한다. 같은 마지막 수정 요청은 쓰지 않고 현재 보기를 200으로 돌려준다. 다른 보기의 수정이나 생성에 이미 쓴 요청 ID는 400이다. 그 뒤 다른 수정이 있었다면 옛 판의 재시도는 409와 최신본이다. 모든 수정 요청 ID의 이력을 저장하는 범용 멱등 체계는 두지 않는다.
- **항목(D-104)**: `PUT /api/items/{id}`의 제목·설명·공개 범위는 `items.version`, `PUT /api/items/{id}/profile`의 리더·목표·범위·기간·상태·종류별 정보는 `items.profile_version`이다. 두 요청에 `base_version`이 필수이며 실제 해당 값이 바뀌면 트리거가 판을 올린다. 한 폼의 변경이 다른 폼의 판을 올리지 않는다.
- 항목 저장은 접근 404·403, 보관 409 `invalid_state`를 먼저 확인한다. 옛 판은 쓰지 않고 409 `conflict`와 최신본을 돌려준다. 단, 보낸 값이 이미 현재 값과 같으면 쓰지 않고 성공한다. 프로필의 성공 응답은 204다. 일반 오류 409와 최신본이 있는 충돌 409를 구분한다.
- API에서 판이 필수인 것과 별개로 DB 함수도 `IS DISTINCT FROM`으로 NULL 판을 불일치로 보고 쓰기 없이 `conflict`를 돌려준다. 항목의 동일값 재시도와 보기의 동일 요청 재시도처럼 쓰지 않는 성공 예외는 유지한다.
- 웹은 충돌 뒤 입력을 보존하고 최신본 보기·다시 저장을 제공한다. 재저장에도 최신 판을 검사한다. D-94의 업무·일정 칸별 확인을 항목에 자동 확대하거나 범용 3-way 병합을 만들지 않는다.
- 프로필 저장·멤버 저장을 시작할 때 앞선 성공 안내를 지운다. 프로필 저장 뒤에는 최신 값을 다시 읽은 후 편집을 닫으며, 기다리는 동안 새로 연 편집을 이전 응답이 닫지 않는다. 다시 읽기에 실패하면 옛 판으로 편집을 다시 열지 못하게 한다.
- **보관 댓글(D-106)**: 전체 접근 경로가 보관된 항목·모듈 아래 권한을 `read`로 낮춘다. 자기 댓글 삭제는 204, 남의 댓글 삭제는 소유 권한자도 기존 403이며 목록의 `can_delete`도 이에 맞춘다. 고치기·새 댓글·스레드 해결의 보관 제한은 유지한다. 403을 409로 바꾸거나 권한을 넓힌 결정이 아니다.

관련 시험은 [saved_views.rs](../crates/server/tests/saved_views.rs), [modules.rs](../crates/server/tests/modules.rs),
[item_profile.rs](../crates/server/tests/item_profile.rs), [comments.rs](../crates/server/tests/comments.rs)와 웹의 저장·충돌 시험이다.

<a id="task-create-contract"></a>

### CSV 업무 만들기

D-105의 원자성 단위는 업무 한 건이다. CSV 각 줄은 모든 칸을 실은 `createTask` 한 번으로 보내며 줄별 결과 안내는 유지한다.
만들기와 고치기는 같은 필드 검증을 사용하고, 실패하면 업무·담당·감사·번호가 일부만 남지 않는다.
제목만 보내는 일반 만들기는 해당 기본값을 쓰는 정상 경로다.

같은 항목·작성자·`request_id`는 처음 만든 업무의 현재 상태를 돌려주며 칸을 다시 쓰지 않는다.
같은 요청 ID에 다른 내용을 보내도 기존 멱등 계약은 내용 비교로 바뀌지 않는다. 처리기의 형식 검사는 먼저 수행한다.
그 업무가 휴지통에 있으면 409 `task_deleted`이며 웹은 이때만 새 요청 ID로 다시 만들 수 있다.
접근 권한의 404에서는 요청 ID를 유지해 중복 생성을 막는다. CSV 전체 트랜잭션이나 D-96의 기대 사용자 적용으로 확대하지 않는다.

응답을 잃은 줄을 고친 뒤 같은 요청 ID로 다시 보내면, 서버가 돌려준 기존 업무가 지금 입력과 다를 수 있다.
웹은 이를 알리고 그 실행을 멈추며 기존 업무를 덮거나 중복 생성하지 않는다. 그 줄은 처리한 것으로 표시해(`line.applied`),
사용자가 다시 누를 때에만 다음 줄부터 진행한다. 일반 실패도 첫 실패에서 멈추고 앞줄 결과와 이어서 넣기 안내를 유지한다.

<a id="history"></a>

## 과거 구현·검증 기록

다음은 당시의 소스·행동·제한·검증 결과를 보존한 원문이다. 그 안의 “현재”, PASS, 미구현·미승인 목록은 해당 SHA와 날짜에 한정한다.
새 작업은 현재 결정·코드·시험을 확인하고, 특정 주장에 필요한 과거 부분만 읽는다.

- [SQL 준비 기준까지의 개발 기록 전체](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md): 초기화·라이선스 조사, 기능별 구현, 감사·실행 결과와 당시 절 번호.
- [최신 정책 구현 검증의 개발 기록 전체](https://github.com/fvoci/GroundZero/blob/0a26654db6c96ccfb232ecbf0a6a02ff8220a5dd/docs/development.md): 기대 사용자와 검색·판 검사·CSV 원자성, 통합 뒤 수정·시험 범위. 옛 96~99절은 이 원문에서 확인한다.
- [SQL 통합 준비 기록](https://github.com/fvoci/GroundZero/tree/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation), [준비 검증 요약](https://github.com/fvoci/GroundZero/blob/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation/verification/results.md): 해당 SHA의 누적 SQL·후보 검증과 NOT RUN을 구분한다. 통합 검증 재현과 원시 증거 위치는 [마이그레이션 안내](../crates/server/migrations/README.md#comparison)에 있다.

고정 SHA에 보존된 과거 SQL 주석 등에서 옛 절 번호를 만나면 다음 원문 위치를 쓴다. 새 절 번호로 재해석하지 않는다.

| 옛 참조                      | 고정 원문                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 27절 작업 / 59절 실시간 끄기 | [27절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L521), [59절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L902)                                                                                                                        |
| 80·81·82절 무결성 감사       | [80절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L1358), [81절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L1446), [82절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L1686) |
| 94·95절 후속·전반 감사       | [94절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L2524), [95절](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/development.md#L2551)                                                                                                                      |
| 97절 검색 / 99절 CSV         | [97절](https://github.com/fvoci/GroundZero/blob/0a26654db6c96ccfb232ecbf0a6a02ff8220a5dd/docs/development.md#L3128), [99절](https://github.com/fvoci/GroundZero/blob/0a26654db6c96ccfb232ecbf0a6a02ff8220a5dd/docs/development.md#L3319)                                                                                                                      |

새 결과는 관련 PR·작업 보고에 기준 SHA와 함께 남긴다. 이 문서에 날짜별 실행 장부를 다시 누적하지 않는다.
