# fvoci 개발 지침

이전 파일명 `development-guidelines.md`. 저장소 구조안(`docs/repo-structure.md`)에 맞춰 이름을 바꿨다.

작성일: 2026-10-17
상태: 저장소 초기화 완료(2026-10-17). 검사 설정과 명령이 존재하며, 대표 위반이 실제로 실패하는지 확인한 기록은 8절에 있다. 실행 명령은 `README.md`와 같다.

관련 결정: `docs/decisions.md`의 D-09(품질 기준), D-10(Docker Compose 환경). 검증 기록 방식은 D-42. 기술 설계 항목은 O-22, O-23, O-25, O-28.

## 1. Rust

- rustfmt와 Clippy를 필수 검사로 사용한다. 고정한 툴체인에서 CI의 경고를 오류로 처리한다.
- 제품 코드의 무분별한 unwrap·expect·panic을 금지한다. 예상 가능한 입력·네트워크·DB 오류는 Result로 전달하고 경계에서 처리한다.
- 오류를 버리거나 성공으로 바꾸지 않는다. 실패를 무시해야 하는 정리 작업도 이유와 처리 정책을 명시한다.
- 자체 제품 코드의 unsafe는 기본 금지한다. 필요한 예외는 범위·안전성 근거·검증을 별도로 검토한다. 의존성 내부 unsafe까지 금지한 것으로 해석하지 않는다.
- ID·권한·상태·단위를 의미 있는 타입으로 표현한다. 모든 값을 String이나 JSON으로 넘기지 않는다.
- async 작업은 취소·종료·오류 회수 책임을 정한다. 무제한 spawn, 블로킹 작업의 무분별한 async 실행, 잠금을 잡은 채 불필요하게 await하는 구조를 피한다.
- Clippy restriction·pedantic 전체를 무조건 활성화하지 않는다. 유효한 규칙을 선택하고 충돌하거나 목적 없는 규칙은 근거를 남긴다. 선택할 규칙 목록은 미정(O-23).

## 2. TypeScript·Svelte

- strict를 활성화하고 noUncheckedIndexedAccess, exactOptionalPropertyTypes, noImplicitReturns, noFallthroughCasesInSwitch, noImplicitOverride를 명시한다.
- .ts뿐 아니라 .svelte 파일도 svelte-check와 Svelte 대응 ESLint 설정으로 검사한다.
- typescript-eslint의 타입 정보를 사용하는 엄격한 규칙을 적용한다. 특히 no-floating-promises, no-misused-promises, no-explicit-any와 unsafe 계열 검사를 활성화한다.
- 외부 데이터는 unknown에서 시작해 실제로 검증한다. 타입 단언만으로 API·스토리지·메시지 입력을 신뢰하지 않는다.
- any, 이중 단언 `as unknown as T`, 근거 없는 non-null assertion, @ts-ignore·@ts-nocheck로 오류를 숨기지 않는다.
- 비동기 작업의 오류·취소·오래된 응답 처리를 명시한다. void를 붙이는 것만으로 Promise의 실패를 처리했다고 간주하지 않는다.
- 화면에서 등록한 이벤트·구독·타이머·편집기 인스턴스는 수명주기에 맞춰 해제한다.

## 3. Rust ↔ TypeScript 계약

- API 요청·응답 타입은 하나의 계약에서 생성하거나 검증 가능하게 연결한다. 같은 DTO를 양쪽에서 독립적으로 손으로 유지하지 않는다. 생성 방식은 미정(O-22).
- 생성된 타입은 런타임 검증을 대신하지 않는다.
- UUID·날짜·시각·파일 크기·증가 번호의 전송 표현을 정한다. 특히 Rust의 64비트 정수를 JavaScript number로 무조건 전달하지 않는다. 구체 표현은 미정(O-22).
- 문서·체크리스트·업무 상태의 정본을 명시하고 화면별로 별도 사본을 관리하지 않는다.

## 4. 검사와 예외

- 테스트·xtask·개발 도구도 품질 검사 대상이다. 테스트의 의도적인 panic·expect 등은 필요한 범위만 예외로 둔다.
- 예외는 최소 범위에 이유를 명시한다. 파일 전체·프로젝트 전체 규칙 해제로 통과시키지 않는다.
- 생성 코드·외부 코드의 제외 범위를 구분하고 자체 코드를 함께 제외하지 않는다.
- 실제 지원하는 feature 조합을 검사한다. 서로 배타적인 feature를 억지로 all-features로 합치지 않는다.
- 로컬과 CI는 같은 검사 진입점을 사용한다. 경고를 숨기거나 검사를 skip해서 성공시키지 않는다.
- 설정이 존재하는 것만으로 완료 처리하지 않는다. 대표적인 위반 코드가 실제로 실패하는지 확인한 뒤 제거하고 결과를 기록한다. 기록은 실행 명령·대상 커밋 SHA·결과·필요한 실패 로그를 PR이나 작업 보고에 남긴다(D-42). 별도 증거 프레임워크를 만들지 않는다.

## 5. 복잡도 원칙

타입과 검사는 엄격하게, 구조는 단순하게 유지한다. 규칙을 만족시키기 위한 범용 프레임워크·중복 래퍼·대규모 형식 변환 계층을 만들지 않는다. 타입 검사 통과만으로 권한·동시성·데이터 정합성이 검증됐다고 간주하지 않는다.

## 6. 개발·통합 테스트 환경: Docker Compose

- PostgreSQL·Meilisearch·격리된 Keycloak은 컨테이너로 실행한다. 테스트용 S3 호환 저장소는 SeaweedFS 고정 버전을 사용한다(D-61, O-31). 운영 서비스나 개인 개발 DB를 테스트에 사용하지 않는다.
- 통합 테스트는 격리된 Keycloak으로 로그인·세션 만료·로그아웃·권한 거부를 검증한다(결정 D-45). Keycloak 전용 API에 의존하지 않고 표준 OIDC만 사용한다.
- Dockerfile·Compose·의존성 버전을 저장소에서 관리하고, 로컬과 CI가 동일한 이미지와 검사 명령을 사용하게 한다.
- 빌드용 이미지와 제품 실행 이미지를 분리한다. 같은 입력으로 만든 이미지는 재사용하며 워커별 중복 빌드를 피한다.
- 병렬 실행마다 Compose 프로젝트 이름·네트워크·볼륨·DB·스토리지 경로를 구분한다. 소스는 각 작업 브랜치의 독립 worktree를 사용한다.
- 캐시를 재사용하더라도 여러 작업이 같은 빌드 출력 디렉터리에 동시에 쓰지 않게 한다.
- macOS와 Linux의 UID/GID 차이를 고려한다. 1000:1000을 고정하지 않고 생성 파일의 소유권을 검증한다.
- 시크릿은 이미지·Git·로그에 넣지 않는다. 테스트 자격 증명은 운영용과 분리한다.
- 불필요한 privileged 모드, Docker 소켓 마운트, 호스트 전체 디렉터리 마운트를 금지한다.
- CPU·메모리·디스크의 실제 사용량에 맞춰 무거운 작업의 동시 실행 수를 제한한다. 컨테이너를 쓴다고 자원 부족이 없어지는 것은 아니다.
- 작업 종료 시 자신이 생성한 컨테이너·네트워크·테스트 볼륨만 정리한다. 전역 prune이나 다른 작업의 자원 삭제는 하지 않는다.
- 빠른 편집·포맷·타입 검사는 고정 버전의 호스트 도구 사용을 허용하되, 최종 통합 검증은 표준 컨테이너 환경에서 실행한다.
- xtask와 pnpm 명령으로 실행·검사·종료할 수 있게 하고, 별도의 복잡한 실행 프레임워크는 만들지 않는다.

## 7. 의존성 라이선스

결정 D-60. fvoci 자체의 공개 라이선스는 별도 결정(O-27)이다.

- 상업 이용·수정·재배포·SaaS 운영에 적합한 허용적 라이선스를 우선한다. MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, PostgreSQL License를 기본 허용 후보로 두되, 실제 사용할 버전·구성요소의 조건을 확인한다.
- 직접·전이 의존성과 복사해 온 코드, WASM·폰트·아이콘도 확인한다.
- GPL·AGPL·LGPL·MPL 등 카피레프트, SSPL·BUSL·비상업용·사용 목적 제한·상용 계약 필수·라이선스 불명 구성요소는 자동 도입하지 않는다. 조건과 대안을 보고해 별도 승인받는다.
- 이중 라이선스는 허용되는 쪽을 선택할 수 있는지 확인하고 선택 근거를 기록한다.
- 오픈소스 코어와 유료·Enterprise 확장 기능을 구분한다. 저장소에 MIT 파일이 있다는 이유로 전체 기능을 MIT로 간주하지 않는다.
- Docker로 분리했다고 라이선스 의무가 사라진다고 판단하지 않는다. 앱에 포함되는 코드, 별도 서비스, 배포 이미지의 OS 패키지는 구분해 검토한다.
- 필요한 저작권 고지·라이선스 원문·NOTICE를 배포물에 보존한다.
- 설치 버전을 고정하고 의존성 변경 시 라이선스 검사를 수행한다. 기존 Cargo·pnpm 도구를 활용하고 새 검사 프레임워크는 만들지 않는다. 도구 선택은 O-28.
- 현재 선택한 rhwp·Meilisearch·Tiptap 및 Office 뷰어 후보도 실제 사용할 패키지·버전·기능 범위로 확인한다. 조건이 충돌하면 조용히 대체하거나 예외 처리하지 말고 보고한다.

## 8. 현재 구성과 검증 기록 (2026-10-17 초기화)

실행 진입점은 `cargo xtask check`(외부 서비스 없음)와 `cargo xtask test`(격리 Compose + 통합 테스트)다. 전체 명령 표는 `README.md`.

### 8.1 적용된 설정

| 항목             | 위치                                                                                                                                                                                                                                                | 비고                                         |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| 툴체인 고정      | `rust-toolchain.toml` (1.99.0, rustfmt·clippy)                                                                                                                                                                                                      |                                              |
| Rust lint        | `Cargo.toml` `[workspace.lints]`: `unsafe_code=forbid`, clippy `unwrap_used`·`expect_used`·`panic`·`todo`·`unimplemented`·`dbg_macro`·`let_underscore_must_use`·`let_underscore_future`·`allow_attributes_without_reason`·`await_holding_lock` deny | pedantic·restriction 전체 활성화 안 함(O-23) |
| 테스트 예외      | `clippy.toml`: `allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`                                                                                                                                                             | 테스트 함수·`cfg(test)` 범위만               |
| 라이선스         | `deny.toml` allow = D-60의 6개, `xtask/src/licenses.rs`의 pnpm 허용 목록                                                                                                                                                                            | 자체 크레이트는 `private.ignore`             |
| TypeScript       | `tsconfig.base.json`: strict + `noUncheckedIndexedAccess`·`exactOptionalPropertyTypes`·`noImplicitReturns`·`noFallthroughCasesInSwitch`·`noImplicitOverride`                                                                                        |                                              |
| ESLint           | `eslint.config.mjs`: typescript-eslint strictTypeChecked + stylisticTypeChecked, eslint-plugin-svelte, `projectService`, `.svelte` 포함                                                                                                             | `consistent-type-assertions: never`          |
| Svelte 타입 검사 | `svelte-check --fail-on-warnings`                                                                                                                                                                                                                   |                                              |
| 포맷             | `rustfmt.toml`, `.prettierrc.json`(prettier-plugin-svelte)                                                                                                                                                                                          | 생성 파일은 `.prettierignore`                |
| 계약             | `contracts/openapi.json` ← `fvoci-server --print-openapi`(utoipa), `packages/api-client/src/generated` ← openapi-typescript                                                                                                                         | `check`가 최신 여부 검사                     |
| Compose          | `deploy/compose.test.yaml`(tmpfs, 동적 포트, Keycloak은 `oidc` 프로필), `deploy/compose.dev.yaml`(named volume)                                                                                                                                     | 프로젝트명은 체크아웃 경로 해시              |

### 8.2 초기화 중의 구현 선택과 이유

- **TypeScript 6.0.3으로 고정.** pnpm이 7.0.2를 설치했으나 typescript-eslint 8.71이 TS 7을 지원하지 않는다(peer `<6.1.0`). svelte-check 4.7은 `^5 || ^6`. 둘 다 만족하는 최신이 6.0.3.
- **sqlx TLS 기능 미포함.** Compose의 PostgreSQL은 TLS 없이 접속한다. rustls·native-tls는 허용 목록 밖 라이선스를 끌어오므로 지금 넣지 않는다. SaaS에서 DB TLS가 필요해지면 O-09와 함께 다시 본다.
- **`sqlx::query!` 매크로 미사용.** 검사 단계에 DB가 필요해지므로 `sqlx::query`와 명시 타입을 쓴다.
- **Keycloak은 `oidc` 프로필.** 포맷·단위 검사와 기본 통합 테스트는 Keycloak 없이 돈다. `cargo xtask test --oidc`만 기동한다. 현재 `--oidc`는 Keycloak 기동과 realm import, healthy 확인까지만 검증한다. 로그인·세션 만료·로그아웃·권한 거부 검사는 다음 사용자 흐름에서 구현하며 아직 NOT RUN이다.
- **작업 구조(O-19 해소, 27절).** 별도 `jobs/` 크레이트 없이 서버 바이너리의 `worker` 모듈이 맡고 실행 역할로 나눈다. 작업 기록은 PostgreSQL 테이블이다.
- **S3 호환 테스트 저장소 아직 없음.** MinIO는 AGPL-3.0이라 쓰지 않는다. 운영은 Cloudflare R2, 테스트는 SeaweedFS 고정 버전을 우선 검토한다(D-61). 업로드 흐름 구현 시 추가한다(O-31).
- **tokio 런타임을 `Builder`로 직접 생성.** `#[tokio::main]`의 내부 `expect`를 피하고 실패를 종료 코드로 처리한다.

### 8.3 대표 위반 확인 (2026-10-17, 작업 트리, 커밋 전)

위반 코드를 넣고 검사가 실패하는지 확인한 뒤 제거했다. 모두 "FAIL as expected".

| 위반                       | 검사                                                    | 결과                              |
| -------------------------- | ------------------------------------------------------- | --------------------------------- |
| 제품 코드 `unwrap()`       | `cargo clippy --workspace --all-targets -- -D warnings` | FAIL (clippy::unwrap_used)        |
| `unsafe` 블록              | 같음                                                    | FAIL (unsafe_code forbid)         |
| `let _ = <Result>`         | 같음                                                    | FAIL (let_underscore_must_use)    |
| 포맷 깨진 Rust             | `cargo fmt --all --check`                               | FAIL                              |
| `any` 변수                 | `pnpm run lint`                                         | FAIL (no-explicit-any)            |
| `arr[0]`를 `string`에 대입 | `pnpm --filter @fvoci/web run check`                    | FAIL (noUncheckedIndexedAccess)   |
| 처리 안 한 Promise         | `pnpm run lint`                                         | FAIL (no-floating-promises)       |
| `as unknown as T`          | `pnpm run lint`                                         | FAIL (consistent-type-assertions) |
| 포맷 깨진 TS               | `pnpm run format:check`                                 | FAIL                              |

제거 후 `clippy`·`fmt`·`lint`·`format:check` 모두 통과를 재확인했다.

### 8.4 실행 검증 (2026-10-17, 브랜치 `init/repo-scaffold`)

| 검사                                                                                                                                    | 실제 범위                                                                                                                                                                                                                                                                                            | 결과                                                                                                                                                                             |
| --------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo xtask check`                                                                                                                     | fmt·clippy·단위 테스트·deny·openapi 최신·prettier·eslint·svelte-check/tsc·vitest·pnpm 라이선스                                                                                                                                                                                                       | 8 PASS, 2 FAIL(cargo deny, pnpm licenses: 허용 목록 밖, 9절 승인 대기)                                                                                                           |
| `cargo xtask test`                                                                                                                      | 격리 Compose(postgres·meilisearch) 기동, `/healthz` 통합 테스트 2건 포함 전체 테스트, 정리                                                                                                                                                                                                           | PASS                                                                                                                                                                             |
| `cargo xtask test --oidc`                                                                                                               | 위 + Keycloak 26.5.4 realm import·healthy 확인까지만                                                                                                                                                                                                                                                 | PASS (Keycloak 기동 확인). 로그인·세션 만료·로그아웃·권한 거부는 NOT RUN, 다음 흐름에서 구현                                                                                     |
| 체크포인트 커밋 e27d729의 새 worktree에서 `pnpm install --frozen-lockfile`, `cargo xtask check`, `cargo xtask test`, `cargo xtask down` | 문서 명령 그대로                                                                                                                                                                                                                                                                                     | check는 위와 동일(라이선스 2건만 FAIL), test PASS, down PASS                                                                                                                     |
| `docker build -f deploy/docker/app.Dockerfile` 로컬 빌드 후 격리 네트워크에서 실행                                                      | `/healthz` → 200 `{"status":"ok","database":"ok"}`, 비root(uid 10001) 실행 확인, 이미지 약 33MB, 컨테이너·네트워크·볼륨·이미지 정리                                                                                                                                                                  | PASS. 제한: 이미지에 웹 번들(`/srv/fvoci/web`)은 들어 있으나 서버가 정적 파일을 서빙하지 않아 `GET /`는 404. 웹 서빙은 다음 흐름에서 추가. 레지스트리 push·공개 배포는 하지 않음 |
| `cargo xtask dev` + `pnpm --filter @fvoci/web exec vite`                                                                                | 개발 Compose 기동, 서버 `/healthz` 200, Vite 페이지 200(`<title>fvoci</title>`), Vite 프록시 경유 `/healthz` 200, 정상 종료(shutdown complete) 후 `cargo xtask down`                                                                                                                                 | PASS                                                                                                                                                                             |
| `.github/workflows/ci.yml`                                                                                                              | GitHub Actions 실행                                                                                                                                                                                                                                                                                  | NOT RUN                                                                                                                                                                          |
| 대표 위반 9종                                                                                                                           | 8.3절                                                                                                                                                                                                                                                                                                | 모두 FAIL as expected                                                                                                                                                            |
| UI 도입 후 브라우저 확인 (2026-10-17, Vite dev + `cargo xtask dev`)                                                                     | 체크리스트 입력·Enter 추가·체크박스 토글(aria-checked), 대화상자 열림 시 포커스 내부 이동·Escape 종료 후 "정보" 버튼으로 포커스 복귀(focus-visible), 메뉴 ArrowDown·Enter 선택과 포커스 복귀, 사이드바 토글, Tab 포커스 링 표시, 글자 크기 body 16px·입력란/버튼/라벨 15px(rem 토큰), 콘솔 오류 없음 | PASS. 대규모 UI 검증 프레임워크는 만들지 않음                                                                                                                                    |

## 9. 라이선스 승인 요청 (2026-10-17 초기화, O-30)

`cargo xtask licenses` 결과 중 허용 목록(D-60의 6개)에 없는 것 전부다. **승인 전이므로 예외를 넣지 않았고 `cargo xtask check`의 두 라이선스 검사(cargo deny, pnpm licenses)는 FAIL 그대로다.** 어느 것도 "제품에 사용 불가능한" 범주(카피레프트 강제·비상업·사용 제한·불명)는 아니다. MPL-2.0만 파일 단위 카피레프트이며 빌드 도구에만 있다.

배치 확인 방법(2026-10-17):

- **브라우저 번들**: `vite build --sourcemap`의 소스맵에 든 패키지 47개를 확인했다. 아래 npm 9개는 하나도 없다.
- **서버 바이너리**: `cargo tree -e no-proc-macro,no-build,no-dev --target x86_64-unknown-linux-gnu`. 컴파일 때만 실행되는 proc-macro는 바이너리에 없다.
- **제품 이미지**(`deploy/docker/app.Dockerfile`): 최종 단계에는 서버 바이너리와 웹 `dist`만 복사한다. 웹 빌드 단계는 개발 의존성을 설치하지만 그 단계는 이미지에 남지 않는다.
- **공개 CI 이미지**(`ghcr.io/fvoci/groundzero-ci-base`, `deploy/docker/dev.Dockerfile`): Rust·Node·pnpm·cargo-deny 같은 도구만 담고 저장소 의존성은 설치하지 않는다. 의존성은 job 실행 중 작업 디렉터리에 받는다(게시하지 않음).

npm (9개, 모두 개발 의존성):

| 패키지@버전                                     | 라이선스 식     | 도입 경로                                                                                                                       | 용도                                                  | 브라우저 번들 | 서버 바이너리 | 제품 이미지 | 공개 CI 이미지 | 고지 의무                                                                  |
| ----------------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ------------- | ------------- | ----------- | -------------- | -------------------------------------------------------------------------- |
| tslib@2.8.1                                     | `0BSD`          | @fvoci/web → bits-ui@2.19.5·@internationalized/date@3.12.4 → @swc/helpers@0.5.23 → tslib                                        | @swc/helpers 일부 헬퍼의 재내보내기. 번들에 안 들어감 | 아니오        | 아니오        | 아니오      | 아니오         | 없음                                                                       |
| lru-cache@11.5.3                                | `BlueOak-1.0.0` | @fvoci/web → jsdom@30.1.3 → @asamuzakjp/css-color@7.1.5·dom-selector@10.0.3 → lru-cache                                         | vitest DOM 테스트 환경                                | 아니오        | 아니오        | 아니오      | 아니오         | 배포 시 라이선스 원문(또는 링크) 동봉. 지금은 배포하지 않아 발생하지 않음  |
| minimatch@10.2.6                                | `BlueOak-1.0.0` | 루트 → eslint@10.12.0·typescript-eslint@8.71.2 → @eslint/config-array@0.23.5 → minimatch                                        | ESLint 파일 패턴                                      | 아니오        | 아니오        | 아니오      | 아니오         | 위와 같음                                                                  |
| mdn-data@2.27.1                                 | `CC0-1.0`       | @fvoci/web → jsdom@30.1.3 → css-tree@3.2.1 → mdn-data                                                                           | 테스트 DOM의 CSS 정의 데이터                          | 아니오        | 아니오        | 아니오      | 아니오         | 없음(공개 도메인 헌정)                                                     |
| @csstools/color-helpers@6.1.2                   | `MIT-0`         | @fvoci/web → jsdom → @asamuzakjp/css-color → @csstools/css-color-parser@4.2.6 → color-helpers                                   | 테스트 DOM 색 계산                                    | 아니오        | 아니오        | 아니오      | 아니오         | 없음                                                                       |
| @csstools/css-syntax-patches-for-csstree@1.1.15 | `MIT-0`         | @fvoci/web → jsdom@30.1.3 → 이 패키지                                                                                           | 테스트 DOM CSS 문법 보정                              | 아니오        | 아니오        | 아니오      | 아니오         | 없음                                                                       |
| lightningcss@1.32.0, 1.33.0                     | `MPL-2.0`       | 1.32.0: @fvoci/web → @tailwindcss/vite@4.3.3 → @tailwindcss/node → lightningcss. 1.33.0: @fvoci/web → vite@8.3.4 → lightningcss | 빌드 때 CSS 변환·압축. 결과 CSS만 번들에 들어감       | 아니오        | 아니오        | 아니오      | 아니오         | MPL 파일을 배포할 때만 그 파일의 소스 입수 방법 안내. 배포·수정하지 않음   |
| lightningcss-darwin-arm64@1.32.0, 1.33.0        | `MPL-2.0`       | lightningcss의 플랫폼별 선택 의존성. Linux 빌드·CI에서는 `lightningcss-linux-*`(같은 라이선스)로 바뀐다                         | 위 패키지의 네이티브 바이너리                         | 아니오        | 아니오        | 아니오      | 아니오         | 위와 같음                                                                  |
| argparse@2.0.1                                  | `Python-2.0`    | @fvoci/api-client → openapi-typescript@7.13.0 → @redocly/openapi-core@1.34.21 → js-yaml@4.3.2 → argparse                        | 계약 → TS 타입 생성                                   | 아니오        | 아니오        | 아니오      | 아니오         | 배포 시 저작권 고지·라이선스 동봉, 수정 시 변경 요약. 지금은 발생하지 않음 |

Cargo (cargo deny가 거부하는 것):

| 크레이트@버전                                                                                                                                                                                                                                                                                      | 라이선스 식                           | 도입 경로                                                                                           | 브라우저 번들 | 서버 바이너리       | 제품 이미지     | 공개 CI 이미지 | 고지 의무                                                                                                         |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------- | ------------------- | --------------- | -------------- | ----------------------------------------------------------------------------------------------------------------- |
| icu_collections@2.3.0, icu_locale_core@2.3.0, icu_normalizer@2.3.0, icu_normalizer_data@2.3.0, icu_properties@2.3.0, icu_properties_data@2.3.0, icu_provider@2.3.1, litemap@0.8.3, potential_utf@0.1.6, tinystr@0.8.4, writeable@0.6.4, yoke@0.8.3, zerofrom@0.1.8, zerotrie@0.2.5, zerovec@0.11.8 | `Unicode-3.0`                         | url@2.5.8(서버 직접·sqlx-core·reqwest·openidconnect 경유) → idna@1.1.0 → idna_adapter@1.2.2 → icu_* | 아니오        | 예                  | 예(바이너리 안) | 아니오         | 배포물에 저작권 고지와 허가 문구 동봉. 승인되면 제품 이미지에 제3자 고지 파일을 넣는다(지금은 배포 전이라 미포함) |
| yoke-derive@0.8.4, zerofrom-derive@0.1.8, zerovec-derive@0.11.6                                                                                                                                                                                                                                    | `Unicode-3.0`                         | 위 크레이트의 derive 매크로(proc-macro)                                                             | 아니오        | 아니오(컴파일 때만) | 아니오          | 아니오         | 배포하지 않아 발생하지 않음                                                                                       |
| unicode-ident@1.0.27                                                                                                                                                                                                                                                                               | `(MIT OR Apache-2.0) AND Unicode-3.0` | serde·clap·sqlx·thiserror·utoipa의 derive → proc-macro2 → unicode-ident                             | 아니오        | 아니오(컴파일 때만) | 아니오          | 아니오         | 위와 같음                                                                                                         |
| foldhash@0.1.5                                                                                                                                                                                                                                                                                     | `Zlib`                                | sqlx-core@0.8.6 → hashlink@0.10.0 → hashbrown@0.15.5 → foldhash                                     | 아니오        | 예                  | 예(바이너리 안) | 아니오         | 바이너리 배포에는 고지 의무 없음. 소스를 배포하면 고지를 지우지 않는다. 제3자 고지 파일에 함께 적는 것을 권장     |

대안: Cargo 쪽은 sqlx·url이 요구해 피할 수 없다(url의 idna 백엔드는 sqlx 경유로 바꿀 수 없음). npm 쪽은 vite·tailwind·eslint·jsdom·openapi-typescript의 기본 의존이라 바꾸려면 도구 자체를 바꿔야 한다. 권장은 위 목록의 라이선스 식을 허용 목록에 추가하는 것이며, 그 경우 `deny.toml`·`xtask/src/licenses.rs`의 허용 목록과 제품 이미지의 제3자 고지 파일을 함께 바꾼다.

KaTeX 글꼴(47절): `katex@0.18.10` 패키지는 MIT지만 `dist/fonts`의 KaTeX_* 글꼴 60개(ttf·woff·woff2)는 글꼴 자체의 name 표에 `SIL Open Font License 1.1`(OFL-1.1, Reserved Font Name 포함, 저작권 Design Science·Khan Academy)로 적혀 있다. 도입 경로 `@fvoci/web → katex@0.18.10 → dist/fonts`. 허용 목록 밖이라 승인 전에는 브라우저 번들에 넣지 않는다: KaTeX의 MIT 스타일에서 `@font-face`만 뺀 `apps/web/src/styles/katex-layout.css`를 쓰고 수식은 시스템 글꼴로 배치한다(운영 빌드에 글꼴 파일 0개). OFL-1.1을 승인하면 `katex/dist/katex.min.css`를 가져오도록 한 줄 바꾼다. `pnpm licenses`는 package.json의 라이선스만 읽어 이 경우를 잡지 못한다.

PDF 본문 검색(38절 후속) 후보: `lopdf` 0.45.0(MIT, 기본 기능 끔)은 잠금에 약 30개 크레이트를 더하고 그중 `zlib-rs`가 `Zlib`이라 위 `foldhash`와 같은 승인 대상이다. Zlib을 허용 목록에 넣는 결정이 나면 PDF 글자 뽑기를 더한다(지금은 PDF를 이름으로만 찾는다).

OR 결합으로 허용 쪽을 선택해 자동 통과한 것(선택 근거 기록): aho-corasick·byteorder·memchr `Unlicense OR MIT` → MIT. tinyvec `Zlib OR Apache-2.0 OR MIT` → MIT. whoami `Apache-2.0 OR BSL-1.0 OR MIT` → MIT.

### O-30 갱신 (2026-10-17, 리비전 `feat/first-flow` 최신)

다시 뽑은 결과(`cargo deny --format json check licenses`, `pnpm licenses list --json --recursive`, 운영 웹 빌드 확인)는 위 표와 **같다**. 크레이트 20개 = Unicode-3.0 19개(icu_collections·icu_locale_core·icu_normalizer·icu_normalizer_data·icu_properties·icu_properties_data·icu_provider·litemap·potential_utf·tinystr·writeable·yoke·yoke-derive·zerofrom·zerofrom-derive·zerotrie·zerovec·zerovec-derive·unicode-ident) + Zlib 1개(foldhash 0.1.5). npm 9개 = tslib 0BSD, lru-cache·minimatch BlueOak-1.0.0, mdn-data CC0-1.0, @csstools 두 개 MIT-0, lightningcss·lightningcss-darwin-arm64 MPL-2.0, argparse Python-2.0. 운영 웹 번들에는 이 9개도 글꼴 파일도 없다(빌드 결과에서 확인). 서버 바이너리에 들어가는 것은 Unicode-3.0 icu 계열(derive·unicode-ident 제외)과 foldhash다.

**지금 트리에 있는 것과 새로 들일 때만 생기는 것의 구분:**

| 항목                     | 지금 20/9개에 포함?                                                           | 라이선스     | 생기는 조건                                                                                                                                                                       | 배포 범위                                       |
| ------------------------ | ----------------------------------------------------------------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| foldhash 0.1.5           | 포함(크레이트 20개 중 하나)                                                   | Zlib         | 이미 sqlx 경유로 들어와 있음                                                                                                                                                      | 서버 바이너리                                   |
| zlib-rs                  | **포함 안 됨(새로 도입)**                                                     | Zlib         | PDF 본문 검색에 `lopdf`를 쓰면 flate2 기본 기능으로 들어옴. **rhwp WASM(`@rhwp/core` 0.8.7)에도 이미 컴파일되어 있음**(rhwp → image 0.25 → png 0.18 → flate2 1.1 → zlib-rs 0.6.7) | 서버 바이너리(PDF), 브라우저 번들(HWP 미리보기) |
| KaTeX 글꼴(KaTeX_* 60개) | **포함 안 됨(새로 도입)**. `katex` 패키지가 MIT라 `pnpm licenses`가 잡지 못함 | OFL-1.1      | 수식 글꼴을 번들에 넣을 때                                                                                                                                                        | 브라우저 번들                                   |
| pako(jszip 경유)         | **포함 안 됨(새로 도입)**                                                     | MIT AND Zlib | 브라우저 DOCX·PPTX 뷰어를 들일 때(아래 Office 대안)                                                                                                                               | 브라우저 번들                                   |

**승인 범위별 예상(미리 단정하지 않기 위해 저장소 밖 임시 설정으로 시험):** Cargo는 허용 목록에 Unicode-3.0·Zlib을 더한 임시 `deny.toml`로 `cargo deny check licenses`가 `licenses ok`였다. npm 9개는 각각 단일 식이라 0BSD·BlueOak-1.0.0·CC0-1.0·MIT-0·MPL-2.0·Python-2.0을 더하면 판정상 통과한다. 다만 허용 목록 승인만으로 끝나지 않는다: (1) 서버 바이너리·이미지에 들어가는 Unicode-3.0·Zlib 크레이트의 고지를 담은 제3자 고지 파일을 제품 이미지에 넣어야 하고(지금 없음), (2) 의존성이 바뀌면 목록이 다시 달라질 수 있으며, (3) 새로 도입하는 위 세 항목은 각각 따로 승인이 필요하다.

**판정 오류 수정**: `xtask/src/licenses.rs`의 SPDX 판정이 바깥 괄호 한 쌍만 벗기고 " OR "로 먼저 나눠, `(MIT OR GPL-3.0-or-later) AND Zlib` 같은 식을 MIT 하나로 통과시켰다(괄호 뒤의 AND 조건 누락). cargo-deny와 같은 해석기인 `spdx` 크레이트(0.13, Apache-2.0, 기본 기능 없음)로 바꿨다. 괄호·우선순위(AND가 OR보다 먼저)·소문자 연산자를 지키고, 예외 조항(WITH)·이후 판(`+`)·읽을 수 없는 식은 허용하지 않고 보고한다. 회귀 테스트 `compound_expressions_keep_grouping_and_precedence`. 지금 트리의 결과(npm 9개)는 바뀌지 않았다.

### D-68 이후 상태 (2026-10-17)

- **승인됨**: D-60 기본 6개(MIT·Apache-2.0·BSD-2-Clause·BSD-3-Clause·ISC·PostgreSQL)와 D-68의 **Zlib**. foldhash 0.1.5(서버 바이너리)가 이제 통과한다. zlib-rs는 rhwp WASM(0.6.7)과 PDF 본문 검색 크레이트로 들어오며 같은 승인으로 덮인다.
- **미승인(검사 FAIL 그대로, 예외 없음)**: Unicode-3.0 크레이트 19개(서버 바이너리에 드는 icu 계열 15개와 빌드 때만 쓰는 derive 3개·unicode-ident), npm 9개(0BSD·BlueOak-1.0.0·CC0-1.0·MIT-0·MPL-2.0·Python-2.0). KaTeX 글꼴(OFL-1.1)은 번들에 넣지 않았다.
- **npm 9개의 배포 여부(실제 산출물로 확인)**: 운영 웹 빌드를 소스맵과 함께 만들어 소스맵에 든 패키지를 뽑았다(55개, JS 번들). 9개 중 어느 것도 없다. tslib은 bits-ui·@internationalized/date → @swc/helpers → tslib 경로지만 @internationalized/date·@swc/helpers·tslib 모두 번들에 들지 않는다(트리 쉐이킹으로 빠짐). CSS 번들은 `app.css`의 `@import`로 tailwindcss·tw-animate-css(MIT)만 들어온다. 직접·개발 의존성 구분만으로 판단하지 않았다(웹의 모든 패키지는 Vite가 번들하므로 개발 의존성으로 선언되어 있다).
- **제3자 고지**: `cargo xtask notices`가 서버 바이너리(`cargo tree -e normal,no-proc-macro`, x86_64·aarch64 Linux 같은 집합, 크레이트 213개)와 웹 번들(소스맵 55개 + CSS 2개)의 패키지마다 함께 배포되는 LICENSE·NOTICE 원문을 `THIRD_PARTY_NOTICES.txt`에 싣는다(원문이 없는 1개는 SPDX 원문 부록). 제품 이미지의 `/usr/share/doc/fvoci/`와 웹 `/THIRD_PARTY_NOTICES.txt`에 넣고, `cargo xtask check`가 최신 여부를 확인한다. 미승인 라이선스 패키지도 실제로 배포되는 것은 고지에 싣는다(고지와 승인은 별개). rhwp를 들이면 WASM 안의 크레이트 고지를 더한다.

### 보안 권고 예외 (cargo-deny advisories)

| 권고                            | 크레이트@버전 | 유입 경로                 | 판단                                                                                                                                                                                            |
| ------------------------------- | ------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| RUSTSEC-2023-0071 (Marvin 공격) | rsa@0.9.10    | openidconnect@4.0.1 → rsa | 개인키 연산(복호화·서명)의 타이밍 누출이다. fvoci는 제공자 공개키로 ID 토큰 서명을 검증만 하고 RSA 개인키를 갖지 않는다. `deny.toml`에 이 ID 하나만 이유와 함께 무시한다. 패치가 나오면 지운다. |

첫 흐름에서 추가한 크레이트(openidconnect, reqwest native-tls, tower-http, time, uuid, rand, sha2, base64, url, axum-extra)와 npm 패키지(@tiptap/core·starter-kit·pm 3.32.0)는 허용 목록 밖 라이선스를 새로 들여오지 않았다.

### 자체 크레이트의 라이선스 필드

fvoci 자체 라이선스는 미정(O-27)이다. unlicensed 경고를 없애려고 MIT 등으로 정하지 않았다. `Cargo.toml`에 `license` 필드를 두지 않고 `publish = false`로 두며, `deny.toml`의 `[licenses] private = { ignore = true }`로 내부 검사에서만 자체 크레이트를 제외한다. 공개 라이선스 결정과 검사 설정은 이렇게 분리되어 있다.

### S3 호환 테스트 저장소 후보 (O-29, 조사만. 구현 없음)

2026-10-17 각 저장소의 LICENSE 파일을 직접 읽은 결과.

| 후보                               | 라이선스      | 비고                                                                  |
| ---------------------------------- | ------------- | --------------------------------------------------------------------- |
| MinIO (minio/minio)                | AGPL-3.0-only | D-60 허용 목록 밖. 별도 서비스로 분리해도 의무가 사라진다고 보지 않음 |
| RustFS (rustfs/rustfs)             | Apache-2.0    | Rust 구현. multipart·중단·재개 호환성 미검증                          |
| LocalStack (localstack/localstack) | Apache-2.0    | 커뮤니티판 기준. 유료 기능 구분은 별도 확인 필요                      |
| Adobe S3Mock (adobe/S3Mock)        | Apache-2.0    | 테스트 전용 목업. JVM                                                 |
| SeaweedFS (seaweedfs/seaweedfs)    | Apache-2.0    | 범용 스토리지. 테스트 용도로는 무거울 수 있음                         |

첫 사용자 흐름에 파일 업로드가 없으므로 미완료로 남기며 인증·문서 흐름을 막지 않는다. 도입 시 multipart·완료·중단·재개 호환성을 검증한다.

### UI 도입으로 추가된 의존성 (2026-10-17, 브라우저 배포물)

허용 목록 안: bits-ui 2.19.5 MIT, @lucide/svelte 1.58.0 ISC, tailwind-variants 3.3.1 MIT, cn 0.4.0 MIT, tw-animate-css 1.4.0 MIT, @internationalized/date 3.12.4 Apache-2.0, tailwindcss·@tailwindcss/vite·node·oxide 4.3.3 MIT, runed·svelte-toolbelt·@floating-ui/\* MIT. 복사해 온 컴포넌트 코드(shadcn-svelte 레지스트리)는 MIT이며 출처를 `apps/web/src/lib/ui/README.md`에 보존한다.

허용 목록 밖으로 들어온 tslib·lightningcss는 위 O-30 표에 합쳤다(번들 포함 여부를 소스맵으로 다시 확인함).

@ 참조 입력(2026-10-17): @tiptap/suggestion 3.32.0 MIT, @floating-ui/dom 1.8.0 MIT(이미 bits-ui로 들어와 있던 것을 직접 의존성으로 고정). 허용 목록 안이다. @tiptap/extension-mention은 표시 이름(label)을 본문에 저장하므로 쓰지 않았다.

### 미확인 (도입 전 확인 필요)

rhwp, Meilisearch 클라이언트 SDK, Tiptap 유료 확장, OIDC 클라이언트 크레이트, Office 뷰어 후보, Playwright. 웹 폰트는 KaTeX 글꼴(OFL-1.1)을 확인했고 승인 전까지 넣지 않는다(위 O-30 메모). 아직 의존성으로 추가하지 않았다.

## 10. CI 구조 (D-64)

- **베이스 이미지**: `deploy/docker/dev.Dockerfile` → `ghcr.io/fvoci/groundzero-ci-base`(공개 패키지, D-65). `.github/workflows/base-image.yml`이 Dockerfile·워크플로 변경, 주 1회 스케줄, 수동 실행 때만 빌드·게시한다. 제품 커밋마다 만들지 않는다. 게시 권한(`packages: write`)은 그 job에만 있고 pull_request에서는 실행되지 않는다.
- **제품 검사**: `.github/workflows/ci.yml`. `check`와 `integration` 두 job이 같은 `ghcr.io/fvoci/groundzero-ci-base@<digest>` 컨테이너에서 돈다. digest는 저장소 변수 `CI_BASE_IMAGE_DIGEST`이며 base-image 워크플로가 게시한 값을 옮겨 적는다. 변수가 비면 job이 시작 단계에서 실패하며 성공으로 간주하지 않는다. 호출하는 명령은 `pnpm install --frozen-lockfile`, `cargo xtask check`, `cargo xtask test --external-services`뿐이다.
- **서비스 격리**: GitHub의 컨테이너 job 안에서는 Compose를 띄울 수 없어 PostgreSQL·Meilisearch를 job `services`로 둔다. 자격 증명은 `compose.test.yaml`과 같은 테스트 값이며, `cargo xtask test --external-services`가 `DATABASE_URL`이 없으면 실패한다. 로컬은 `cargo xtask test`(Compose)로 같은 테스트를 돌린다.
- **OIDC 로그인 테스트(`oidc` job)**: 컨테이너 job에서는 Compose를 띄울 수 없으므로 GitHub 호스트 러너(`ubuntu-24.04`)에서 `compose.test.yaml`의 `oidc` 프로필(PostgreSQL·Meilisearch·Keycloak)을 띄운다. 테스트는 다른 job과 같은 베이스 이미지 digest 안에서 같은 진입점 `cargo xtask test --external-services --oidc`로 돈다. 테스트 컨테이너는 Compose 네트워크에 붙고 체크아웃만 마운트한다(Docker 소켓은 넘기지 않음). 프로젝트명 `fvoci-ci-<run_id>-<run_attempt>`로 실행마다 컨테이너·네트워크·볼륨을 분리하고, PostgreSQL은 tmpfs라 매번 빈 DB에서 시작하며, 결과와 상관없이 `down --volumes`로 지운다.
- **셀프호스트 Supabase 테스트(`supabase` job, D-88): D-64의 이미지 계약 예외.**
  - **근거**: `cargo xtask test --supabase`는 xtask가 공식 Supabase 구성을 받아 호스트 Docker로 Compose를 띄운다. 기동에 실패하면
    정리하기 전에 컨테이너 상태·healthcheck 기록·로그를 그 Docker에서 읽는다. base 이미지에는 Docker CLI가 없고 Docker 소켓도
    넘기지 않는다(`oidc` job과 같은 원칙). 그래서 이 job만 호스트 러너(`ubuntu-24.04`)에서 xtask를 실행한다.
  - **범위**
    - 툴체인은 base 이미지와 같은 판을 `rust-toolchain.toml`(1.99.0)이 고정한다.
    - 다른 job의 image-digest 계약은 그대로다.
    - 제품 이미지·시크릿은 쓰지 않는다. 서비스 이미지는 `deploy/supabase/compose.test.yaml`에서 digest로 고정한다.
- **OIDC 검증 기록 (2026-10-17, `feat/first-flow` 작업 트리)**:

  | 구분                      | 방법                                                                                 | 결과                                                                                                                                                   |
  | ------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
  | 자동 검사, 로컬 호스트    | `cargo xtask test --oidc` (빈 tmpfs DB + Keycloak)                                   | PASS. 전체 71건 중 OIDC 4건: 발급자+주체 식별과 로그아웃 시 앱·제공자 세션 종료, 두 번째 사용자의 남의 문서 접근 거부(404), 세션 만료, 위조 state 거부 |
  | 자동 검사, CI job 재현    | 새 worktree에서 `oidc` job 단계를 그대로 실행(이미지만 로컬 빌드한 `dev.Dockerfile`) | PASS. 같은 OIDC 4건 포함. 실행 뒤 컨테이너·네트워크 0개                                                                                                |
  | 자동 검사, GitHub Actions | 원격 실행                                                                            | NOT RUN. `feat/first-flow`는 push 승인 범위 밖                                                                                                         |
  | 수동 로그인               | `cargo xtask dev --oidc` + 브라우저에서 Keycloak 테스트 계정 로그인                  | 성공(로그인·작업). 자동 검사 통과와 별개 기록                                                                                                          |

- **캐시**: `Swatinem/rust-cache`와 buildx의 GHA 캐시는 속도용이다. 없어도 같은 명령이 성공해야 한다.
- **산출물 공유**: 현재 통합 테스트는 in-process라 제품 바이너리를 소비하지 않는다. e2e가 생기면 `build` job이 서버 바이너리와 웹 번들을 한 번 만들고 소스 SHA·아키텍처를 키로 하는 artifact로 넘긴다. 병목 측정 전에는 job을 더 쪼개지 않는다.
- **베이스 이미지 갱신 절차**: Dockerfile을 바꾸면 base-image 워크플로가 새 digest를 Step Summary에 적는다. `CI_BASE_IMAGE_DIGEST` 변수를 그 값으로 바꾸면 이후 CI가 새 이미지를 쓴다. 주간 스케줄 빌드도 같은 절차로 반영한다.
- **제품 이미지 릴리스(D-87)**: `.github/workflows/release-image.yml` → `ghcr.io/fvoci/groundzero:X.Y.Z`(linux/amd64·linux/arm64, `latest` 없음).
  - 트리거: `vX.Y.Z` 태그 push, 또는 Actions → release-image → Run workflow에 이미 있는 태그 입력. 명령으로는 `gh workflow run release-image.yml --ref main -f tag=v0.0.1`. 워크플로가 main에 들어간 뒤에만 수동 실행할 수 있다(그 전 태그는 다시 push하지 않는다).
  - 순서: 태그 확인(형식·원격 존재·main 포함) → 같은 이미지 태그가 있으면 소스 SHA 비교(같으면 그대로 끝, 다르면 실패) → 플랫폼마다 그 태그 커밋을 빌드해 digest로만 올림 → 받아서 확인 → 두 플랫폼이 다 통과하면 한 manifest로 합쳐 태그. Step Summary에 태그·소스 SHA·플랫폼·manifest digest·`docker pull` 명령이 남는다.
  - 버전: 태그에서 정한 제품 버전·전체 SHA를 build arg `FVOCI_RELEASE_VERSION`·`FVOCI_SOURCE_SHA`와 OCI 라벨(`org.opencontainers.image.version`·`revision`)에 같이 넣는다. 확인: 라벨, `fvoci-server version`, 시작 로그 `server.starting`의 `version`·`source`, `/healthz`의 `build`가 모두 같아야 한다(서버는 닿지 않는 DB 주소로 띄워 503 본문으로 확인, DB·Keycloak 쓰지 않음). 로컬·개발 빌드는 `dev`. `v0.0.1`은 이 지원 전 소스라 라벨만 붙고 앱 안 버전은 없다(확인이 그렇게 기록).
  - PR에서 Dockerfile·이 워크플로가 바뀌면 게시 권한 없이 두 플랫폼 빌드와 같은 확인만 한다(시험 값 0.0.0, 게시 안 함). 같은 PR에서 `guard-check`가 기존 이미지 확인의 분기(한 번도 게시하지 않은 패키지·없는 태그 → 없음, 같은 소스 → 그대로, 다른 소스 → 실패, 없는 소유자·토큰 없음 → 실패)를 실제 응답으로 확인한다(`groundzero-ci-base` 사용).
  - 기존 이미지 확인: 이미지 소유자의 종류(fvoci는 사용자 계정)에 맞는 패키지 API(`/users`·`/orgs`)를 쓰고, "없음"은 그 경로가 "Package not found."로 답한 404일 때만이다. 다른 404·권한 오류·네트워크 오류는 실패로 멈춘다(이미지 없음으로 보지 않는다).
  - 공개 pull: 처음 게시한 패키지는 GHCR에서 비공개로 생길 수 있다. 공개로 받게 하려면 소유자 fvoci(사용자 계정)의 패키지 `groundzero` 설정에서 공개를 켠다(소유자 작업, 워크플로는 바꾸지 않는다).

## 11. 프론트엔드 UI 도입 기록 (D-66, 2026-10-17)

- 설치: Tailwind CSS 4.3.3 + `@tailwindcss/vite`, shadcn-svelte 1.7.0 수동 설치 방식(Vite, 비 SvelteKit). `$lib` 별칭을 `vite.config.ts`와 `apps/web/tsconfig.json`에 둔다. `components.json`의 `ui` 별칭은 `$lib/ui`.
- 구성요소와 출처·명령: `apps/web/src/lib/ui/README.md`. 디자인 토큰: `apps/web/src/styles/app.css`(shadcn 원문 + fvoci 타이포그래피 토큰 `--font-sans`, `--text-body`, `--leading-body` 등). 글자 크기는 rem, 입력란·버튼은 `font: inherit`. 웹 폰트는 넣지 않고 시스템 글꼴(CJK 폴백 포함)만 쓴다.
- 수정분: dropdown-menu의 RadioGroup·RadioItem 제거(미사용, `exactOptionalPropertyTypes` 충돌). ESLint는 가져온 코드 디렉터리에만 규칙 예외(10개, select 추가 후)(`eslint.config.mjs` 주석). strict 플래그는 완화하지 않았다.
- 앱 고유 컴포넌트는 `apps/web/src/lib/features/<기능>/`. 일반 체크리스트 폼은 `features/checklists/ChecklistForm.svelte`(표시·입력만, 저장은 첫 흐름).
- 검증: `pnpm run lint`·`check`·`test` PASS. 브라우저 상호작용 확인은 8.4절 갱신분.

## 12. 첫 사용자 흐름: 로그인·워크스페이스·위키 문서 (2026-10-17, 브랜치 `feat/first-flow`)

### 12.1 실행 구성

- **DB 역할 (D-08, O-09 해소)**: `fvoci-server db bootstrap`이 관리자 연결(`DATABASE_ADMIN_URL`)로 `fvoci_migrator`(스키마 `fvoci` 소유)와 `fvoci_app`(`NOBYPASSRLS`)을 만들거나 비밀번호를 갱신한다. `fvoci-server db migrate`가 마이그레이션 역할로 `crates/server/migrations/`를 적용한다. 서버는 앱 역할(`DATABASE_URL`)로만 접속한다. 둘 다 반복 실행해도 안전하다. `cargo xtask test`·`dev`와 CI가 같은 두 명령을 쓴다.
- **환경 변수**: `.env.example`. 개발·테스트 값은 xtask가 넣는다.
- **개발 로그인**: `pnpm install --frozen-lockfile` 뒤 `cargo xtask dev --oidc` 하나로 개발 서비스(PostgreSQL·Meilisearch·Keycloak)·서버·웹(Vite)이 뜬다. 접속은 `http://127.0.0.1:5173`(localhost 아님) → **로그인** → 아래 계정. Keycloak 관리 콘솔 조작은 필요 없다. 계정은 `deploy/keycloak/realm-test.json`의 가짜 사용자이고 이 저장소에 적힌 개발·테스트 전용 값이다. 운영·셀프호스트 구성(`realm-selfhost.json`은 사용자 없음, `compose.selfhost.yaml`은 모든 비밀 값을 운영자 `.env`에서 읽음)에는 들어가지 않는다.

  비밀번호는 저장소 규칙대로 `deploy/` 안에만 둔다: 두 사용자는 `deploy/keycloak/realm-test.json`의 `credentials`, Keycloak 관리자는 `deploy/compose.dev.yaml`의 `KC_BOOTSTRAP_ADMIN_PASSWORD`.

  | 계정                     | 역할                                                                                        |
  | ------------------------ | ------------------------------------------------------------------------------------------- |
  | `test-user`              | 초기 관리자(D-51, `FVOCI_INITIAL_ADMIN_SUBJECT`를 xtask가 넣음). '개발' 워크스페이스 소유자 |
  | `test-user-2`            | 일반 사용자. '개발' 워크스페이스 작성자                                                     |
  | `admin`(Keycloak 콘솔만) | `http://127.0.0.1:8180/admin`의 Keycloak 관리자(필요할 때만)                                |
  - 공동 편집: '개발' 워크스페이스는 실시간 공동 편집이 켜져 있고 위키에 **공동 편집 연습** 문서가 있다. 두 계정을 서로 다른 브라우저(또는 일반 창과 시크릿 창)로 열어 같은 문서를 고친다. 한 브라우저의 두 탭은 쿠키를 함께 써서 같은 사람이 된다.
  - 준비는 `deploy/postgres/dev-seed.sql`이 한다: 로그인 때와 같은 함수로 두 사용자를 만들고, 초기 관리자 함수로 워크스페이스를, 초대·수락 함수로 멤버를, 위키 함수로 문서를 만든다(번호·감사·권한 검사 그대로). 새 행 id는 xtask가 만들어 넘긴다(85.1). `instance_state`의 `dev_seed` 표시가 있으면 아무것도 하지 않으므로 다시 실행해도 중복되지 않고 바꾼 비밀번호·데이터를 덮지 않는다. 다만 처음 준비한 발급자와 지금 발급자가 다르면(개발 Keycloak 포트를 바꿈) 테스트 사용자가 다른 계정이 되므로 알리고 멈춘다(처음 포트로 되돌리거나 `cargo xtask down`).
  - **모듈 예제**: 같은 준비 단계에서 이어서 기존 테스트 사용자 둘이 함께 쓰는 별도 **모듈 예제 (합성 데이터)** 워크스페이스를 만든다(`deploy/postgres/dev-module-seed.sql`). 프로젝트·연구·스터디·강의·취미, 전용 기록 18종, 공통 업무·문서·체크리스트·일정·종료된 시간 기록, 서로 다른 개인 학습 진도가 들어간다. 실행 중인 타이머나 외부 파일·계정은 만들지 않는다. 날짜는 처음 실행한 날 기준이며, `dev_module_seed_v1` 표시와 트랜잭션 잠금으로 반복·동시 실행 때 중복이나 사용자 편집 덮어쓰기를 막는다. 예제를 지워도 다시 채우지 않는다. `--oidc` 없는 개발 실행·운영·셀프호스트는 이 파일을 실행하지 않는다.
    - 보강(`dev_module_seed_v2`, 86절): 같은 파일의 둘째 블록이 같은 워크스페이스에 모듈마다 업무를 더해(프로젝트 4, 나머지 2, 할 일·진행 중·완료와 우선순위를 섞음) v1과 합쳐 프로젝트는 칸반 열마다 카드가 둘 이상(같은 열 안 순서 바꾸기), 나머지 모듈은 열마다 한 장이 되게 하고, 위키 **운영 안내 (합성) › 설치 › 백업** 세 단계와 설치·백업을 가리키는 **점검 기록**(백링크 3건), 구분선 바로 아래 체크리스트가 있는 본문을 만든다. 행을 고치거나 지우지 않고 더하기만 한다. 예제 워크스페이스를 지웠으면 아무것도 만들지 않고 표시만 남기며, 휴지통·보관 상태인 항목에는 업무를 더하지 않는다. 만들다 실패하면 이 블록만 되돌리고 `WARNING`을 남긴 뒤 개발 실행은 계속한다(다음 실행에 다시 시도). 이미 v1만 있는 개발 DB에도 다음 `cargo xtask dev --oidc`에서 한 번 더해진다.
    - **예제 정리**: 예제만 치우려면 소유자 `test-user`로 로그인해 **모듈 예제 (합성 데이터)** 워크스페이스를 지운다(29절, 휴지통에 들고 기본 30일 보관, 그 안에서 되살리기 가능). 표시가 남으므로 다음 개발 실행이 다시 만들지 않는다. 처음 상태로 다시 받으려면 `cargo xtask down`(이 체크아웃의 개발 DB·검색·Keycloak 볼륨을 모두 지운다, 다른 개발 데이터도 사라진다)을 쓴다. 예제만 지우는 별도 명령은 두지 않는다.
  - 사용자 식별(발급자 + subject)이 바뀌지 않도록 개발 Keycloak은 고정 포트 `127.0.0.1:8180`(`FVOCI_DEV_KEYCLOAK_PORT`로 변경)에 열고, 테스트 사용자는 고정 id를 쓴다. Keycloak은 realm이 이미 있으면 파일을 다시 들이지 않으므로, 고정 id 전의 볼륨이 남아 테스트 사용자 id가 realm 파일과 다르면 `cargo xtask dev --oidc`가 준비 전에 알리고 멈춘다(`cargo xtask down` 뒤 다시 실행. 볼륨을 스스로 지우지 않는다). 서버 8080·웹 5173·Keycloak 8180이 고정이라 한 컴퓨터에서 개발 환경은 한 번에 하나만 띄운다.
  - 데이터: Ctrl-C로 멈춰도 이 체크아웃의 Compose 볼륨(DB·검색·Keycloak)과 `target/dev-files`(첨부)가 남아 다음 실행에 그대로 쓰인다. 처음부터 다시 하려면 `cargo xtask down`(이 체크아웃의 개발·테스트 컨테이너·네트워크·볼륨을 지운다. 호스트의 `target/dev-files` 첨부는 지우지 않으므로 첨부까지 비우려면 그 폴더를 따로 지운다).
  - 볼륨에 없는 상태는 컨테이너와 함께 사라진다: `cargo xtask dev`(`compose up`)는 서비스 정의(포트·볼륨·이미지·명령)가 바뀐 컨테이너를 다시 만든다. Keycloak 데이터 볼륨(`kcdata`)은 `6697625`(2026-10-19)부터라 그 전에 만든 개발 Keycloak은 realm·사용자·세션을 컨테이너 안에만 두었고, 새 정의로 처음 띄울 때 그것이 지워진다. 오래된 개발 환경을 지켜야 하면 그 체크아웃에서 다시 띄우기 전에 컨테이너를 `docker commit`으로 이미지로 남기거나 `docker cp <컨테이너>:/opt/keycloak/data <보관 위치>`로 떠 두고, 새 코드의 개발은 별도 체크아웃(다른 Compose 프로젝트·볼륨)에서 한다(84절).
  - `--no-web`이면 웹을 띄우지 않는다(웹은 `VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev`로 따로). 이 realm의 클라이언트는 리다이렉트 URI를 `*`로 열어 둔 개발·테스트 전용이다.

### 12.2 설계 요점

- **저장 경계 (D-62)**: 본문은 Tiptap JSON(`schema_version` 1)이고 저장마다 리비전이 1 오른다. 이것은 JSON 리비전 저장이며 공동 편집이 아니다. CRDT 도입 시 상태 열을 `body` 옆에 두고 문서 id·번호·리비전을 이어 쓴다. 실시간 OFF의 저장 방식은 이 경로를 그대로 쓴다.
- **저장 순서**: 권한 확인 → 행 잠금(`FOR UPDATE`) → 같은 사용자의 같은 `request_id` 재시도 판단(그 리비전이 최신이 아니면 409) → 내용 동일이면 리비전 없이 성공 → `base_revision` 불일치면 409와 서버 최신본 → 갱신·리비전 기록. 클라이언트는 409에서 초안을 버리지 않고 서버 버전과 비교해 고른다(D-53).
- **DB 불변식**: 번호 할당·문서 생성·감사 기록은 정의자 함수와 트리거만 한다. 앱 역할은 세션·로그인 상태·번호열·감사 기록 테이블을 직접 읽거나 쓰지 못한다. 리비전은 1씩만 오르고, 리비전 행은 문서의 현재 리비전과 같아야 한다. 제목 변경 감사 기록은 트리거가 남기며 본문은 남기지 않는다.
- **로그인 (D-45)**: Authorization Code + PKCE + nonce. 쿠키에는 무작위 비밀값만 두고, 로그인 상태 행은 그 해시로 찾으며 OIDC `state`는 행의 값과 비교한다. 세션 토큰은 해시만 저장하고 로그인 시 기존 세션을 회전한다. https에서는 `__Host-` 쿠키, 같은 이름 쿠키가 둘 이상이면 거부한다. 상태를 바꾸는 요청은 `Origin`이 공개 원점과 같아야 한다. 로그아웃은 서버 세션을 지우고 제공자 로그아웃(`id_token_hint`)으로 보낸다.
- **권한 응답**: 멤버가 아니면 404(존재를 숨김), 역할 부족이면 403, 세션 없음·만료·중복 쿠키는 401.
- **웹**: 경로 `/`, `/w/{ws}`, `/w/{ws}/wiki/{doc}`. 서버는 `FVOCI_WEB_DIR`의 `index.html`로 새로고침을 지원한다. 편집기 인스턴스는 마운트마다 하나이며 내용을 통째로 바꿀 때 다시 만든다. 자동 저장(입력 후 1.5초), 미저장 초안의 로컬 보관과 이탈 경고, 네트워크 실패 시 같은 `request_id`로 재시도한다.

### 12.3 검증 (2026-10-17)

| 검사                                            | 실제 범위                                                                                                                                                                                                                                                                                        | 결과                 |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------- |
| `cargo xtask test --oidc`                       | 단위 10, 첫 흐름 11(교차 사용자 404·DB 수준 RLS 반례·충돌·재시도·재시도 후 다른 저장 시 409·내부 테이블 직접 접근 거부·리비전 되감기 거부·중복 쿠키·만료·새 상태 재열기), health 2, 실제 Keycloak OIDC 4(issuer+subject 식별·로그아웃 후 재로그인 폼·다른 사용자 거부·세션 만료·위조 state 거부) | PASS                 |
| `cargo xtask test`                              | 위에서 OIDC 4건 제외(NOT RUN 출력)                                                                                                                                                                                                                                                               | PASS                 |
| 실제 브라우저 (`cargo xtask dev --oidc` + Vite) | Keycloak 로그인 → 워크스페이스 생성 → 위키 문서 작성 → 자동 저장(정확히 리비전 1개) → 서버 재시작 후 재열기(세션·내용 유지) → 두 탭 동시 편집 충돌·비교·확인 후 재저장 → 로그아웃(제공자 세션 종료) → 재로그인 폼 → 재열기 → 두 번째 사용자 화면·API 모두 거부(404)                              | PASS                 |
| 로컬 app 이미지                                 | 이미지 안의 `db bootstrap`·`db migrate` 반복 실행, 앱 역할로 `/healthz` 200, `GET /` 웹, 깊은 경로 새로고침, 알 수 없는 API 404                                                                                                                                                                  | PASS. push·배포 없음 |
| 원격 CI (이 브랜치)                             | 원격 push 승인 전                                                                                                                                                                                                                                                                                | NOT RUN              |
| CI의 OIDC 테스트                                | GitHub job services는 Keycloak `start-dev` 인자를 받지 못한다                                                                                                                                                                                                                                    | NOT RUN (12.4)       |

브라우저 검증 중 발견해 고친 결함: `jsonb`가 객체 키 순서를 바꿔 돌려줘 클라이언트가 계속 "변경됨"으로 판단하고 저장을 반복했다. 클라이언트 비교를 키 정렬 정규형으로 바꾸고, 서버는 내용이 같으면 새 리비전을 만들지 않는다(회귀 테스트 포함).

### 12.4 남은 제한

- CI에서 OIDC 통합 테스트를 돌리려면 Keycloak을 인자와 함께 띄울 방법이 필요하다(호스트 runner의 Compose 또는 테스트용 Keycloak 이미지 게시). 새 이미지 게시는 승인 대상이라 보류했다.
- `/auth/login` 요청 빈도 제한은 없다. 만료 행은 요청마다 최대 100개씩 지운다.
- 여러 서버 인스턴스 실행은 검증하지 않았다(디스커버리 캐시는 인스턴스별).

## 13. 멤버·초대·역할·그룹·소유권 (2026-10-17, D-20·D-25·D-51·D-57)

- **소유자**: 워크스페이스마다 한 명이며 항상 관리자 역할이다. 소유자는 탈퇴·제거·강등될 수 없다. 인계는 소유자가 제안하고 수신자가 수락해야 완료된다. 제안은 취소할 수 있다.
- **초대**: 관리자가 이메일과 역할을 정한다. 초대는 IdP가 검증한 같은 이메일(`email_verified`)을 가진 로그인 사용자에게만 보이고 그 사용자만 수락할 수 있다. 링크만으로는 가입할 수 없다(D-51). 이메일로 계정을 병합하지는 않는다(D-45). 앱 안의 "받은 초대"로 전달하므로 SMTP 없이 동작한다(D-55). 유효 기간은 14일이고, 같은 이메일에 다시 초대하면 열린 초대를 갱신한다.
- **멤버 관리**: 역할 변경·제거·그룹 관리·초대는 관리자만, 탈퇴는 본인이 한다. 제거·탈퇴해도 그 사람이 만든 팀 문서는 워크스페이스에 남는다(D-25). 멤버 목록의 이메일은 관리자에게만 보인다.
- **구현**: 모든 변경은 `migrations/0003_members.sql`의 정의자 함수가 권한·불변식·감사 기록을 함께 처리한다. 함수 오류 코드는 `AppError::from_db_function`이 응답으로 바꾼다(P0002 404, 42501 403, 22023 400, 55000 409).
- **같은 마이그레이션의 0002 재검토 보강**: 저장 재시도 키를 작성자 단위로 바꿨다. 로그인 상태는 `state`가 맞을 때만 꺼낸다(위조 콜백이 진행 중 로그인을 지우지 못함). 리비전 일치 트리거는 호출자 권한으로 돈다. 로그아웃은 중복 쿠키에서도 제거 쿠키를 보낸다.
- **검증**: `cargo xtask test --oidc`에 멤버 통합 테스트 7건을 추가했다(초대 대상 확인, 거절·취소, 역할 집행, 소유자 보호·인계 수락, 제거 후 접근 상실·팀 문서 보존, 그룹, 감사 기록). 실제 브라우저에서는 Keycloak 두 사용자로 초대 → 받은 초대 수락 → 독자 화면(이메일 숨김·관리 동작 없음·역할 이름 표시) → 확인 대화상자로 탈퇴까지 확인했다. 역할 선택은 키보드만으로 열고 고르며 포커스가 트리거로 돌아온다.

## 14. 개인 노트·공유·비공개·복제·이동 (2026-10-17, D-20·D-22·D-23·D-24·D-25·D-40·D-49·D-57·D-59)

- **한 문서 엔진**: 개인 노트도 `documents`와 같은 저장·리비전·충돌 경로를 쓴다. 문서는 워크스페이스 소유(위키) 또는 사용자 소유(개인 노트) 중 정확히 하나이며 DB 제약으로 강제한다. 개인 노트에는 번호가 없다.
- **권한 함수 하나**: `app_document_access(doc)`가 `owner·write·read·없음`을 돌려준다. RLS(문서·리비전)와 서버 코드가 같은 함수를 쓴다. 개인 노트 소유자와 워크스페이스 관리자가 `owner`다. 작성자·독자는 `visibility='private'`이면 자기가 만든 문서만 본다. 명시적 공유는 그 권한을 준다. 개인 노트는 어느 워크스페이스에도 속하지 않으므로 받는 사람의 워크스페이스 관리자에게 권한이 가지 않는다(D-49).
- **공유 대상**: 이메일로 지정하고, 제공자가 검증한 같은 이메일의 로그인 사용자에게만 적용한다. 사용자 목록은 노출하지 않는다. 철회는 다음 요청부터 반영된다. "공유받은 문서"에는 공유한 사람 이름만 보여주고 워크스페이스 이름은 보여주지 않는다.
- **설정·공유·복제·이동**은 정의자 함수가 행을 잠근 뒤 권한을 확인하고 감사 기록을 남긴다. 앱 역할은 공개 범위·복제 허용·소유 열을 직접 바꿀 수 없다. 리비전은 내용이 바뀔 때만 정확히 1 오른다.
- **복제**: 소유 권한자는 항상, 그 밖의 사람은 원본이 복제를 허용할 때만 한다. 사본은 리비전 1의 독립 문서이고 원본과 동기화하지 않는다. 비공개 문서·개인 노트의 사본은 비공개로 만든다.
- **이동**: 개인 노트 → 워크스페이스, 워크스페이스 → 워크스페이스만 지원한다. 팀 문서를 개인 노트로 빼는 이동은 409이며 복제를 안내한다(D-25). 서버가 접근 변화 미리보기를 계산하고, 이동 요청은 사용자가 확인한 미리보기를 담아야 한다. 잠근 뒤 다시 계산한 결과와 다르면 이동하지 않고 새 미리보기를 돌려준다. 리비전 기록도 함께 옮겨진다는 점을 미리보기에 표시한다. 대상 번호가 새로 매겨지며 링크는 id 기준이라 유지된다(D-18). 옮긴 사람이 볼 수 없게 되면 응답에 문서 없이 새 위치만 준다.
- **감사 기록**: 개인 영역 기록은 `owner_user_id`로 소유자에게 보인다(D-57). 다른 영역의 ID는 감사 기록에 남기지 않는다.
- **검토**: 별도 검토자가 반례를 찾았다. 비공개 문서 이동 후 404·롤백, 잠금 전 권한 확인 경합, 공유받은 사람에게 워크스페이스 이름 노출, 내용 없는 리비전 증가, 복제로 공개 범위 확대, 감사 기록의 다른 영역 ID, 이력 동반 이동 미표시, 미리보기와 이동 확인의 분리였다. 모두 고치고 회귀 테스트를 넣었다.
- **검증**: `tests/sharing.rs` 10건(비공개 문서 열람 범위, 공유 권한·철회·미검증 이메일, 받는 사람의 워크스페이스 관리자 비노출(HTTP·DB), 복제 권한·독립성, 이동 미리보기 확인·오래된 확인 거부·접근 변화, 비공개 문서 이동 완료, 개인 노트의 비공개 이동, 사본 비공개 유지, 리비전 위조 거부, 직접 갱신 거부·개인 감사 기록). 실제 브라우저에서 개인 노트 작성 → 공유 → 받는 사람의 "공유받은 문서"·읽기 전용 → 복제 허용 후 내 노트로 복제 → 새 워크스페이스로 이동 미리보기·확인을 확인했다.
- **비로그인 공개 링크 (D-20, 마이그레이션 0005)**: 소유 권한자가 만들고, 만료(1·7·30일 또는 없음)를 정하며, 언제든 철회한다. 링크 토큰 원문은 만들 때 한 번만 보여주고 DB에는 해시만 둔다. 공개 보기(`/p/{token}`, `GET /api/public/{token}`)는 로그인 없이 읽기 전용이며 내부 ID를 내보내지 않는다. 철회·만료 즉시 404이고, 응답은 `Cache-Control: no-store`라 캐시에 남지 않는다. 생성·철회는 감사 기록에 남는다. 통합 테스트 2건과 브라우저(로그아웃 상태로 열기)로 확인했다.
- **남은 것**: 그룹 대상 공유는 열 추가로 확장할 수 있게 테이블을 두었다(그룹 공유가 필요해지면 추가).

## 15. 모듈 인스턴스·키·공통 번호·항목 (2026-10-17, D-14·D-15·D-16·D-17·D-18·D-20·D-48·D-57·D-59, 마이그레이션 0006)

- **모듈 인스턴스**: 관리자가 기능 유형(프로젝트·연구·스터디·강좌·취미)과 이름·키로 만든다. 같은 유형을 여러 개 둘 수 있고(예: DEV·OPS 둘 다 프로젝트), 유형은 만든 뒤 바꾸지 않는다. 보관하면 그 아래 항목·문서가 읽기 전용이 되고 목록에는 남는다. 보관은 휴지통(D-41)과 다르며 번호·키를 그대로 둔다.
- **키 대장(`module_keys`)**: 입력은 대문자로 바꿔 받는다(`dev` → `DEV`). 2~10자 영문·숫자, 영문으로 시작, `WIKI` 예약. 워크스페이스 안에서 현재 키와 예전 키를 통틀어 유일하므로 다른 모듈이 예전 키를 가져갈 수 없다. 같은 모듈은 자기 예전 키를 되찾을 수 있다. 키 변경은 감사 기록에 남는다.
- **번호 대장(`numbered_targets`)**: 범위(위키 = 워크스페이스 id, 모듈 = 모듈 id)마다 번호열 하나를 항목·문서(이후 작업)가 함께 쓴다. `(범위, 번호)`와 `(대상 종류, 대상 id)`가 각각 유일하다. 할당은 정의자 함수 `app_allocate_number`만 하며 앱 역할은 대장과 카운터에 쓸 수 없다. 참조 문자열(`DEV-3`)은 저장하지 않고 읽을 때 현재 키로 만든다(`app_document_reference`, `app_item_reference`). 키를 바꿔도 번호·id·링크는 그대로다. 이동한 문서는 새 범위에서 새 번호를 받고 옛 번호는 재사용하지 않는다.
- **항목(`items`)**: 개별 프로젝트·연구·스터디·강좌·취미 활동이다. 새 항목은 비공개다(D-20). 권한은 `app_item_access` 하나로 판정한다. 관리자 → owner, 만든 사람 → owner(독자로 내려가면 read), 공개 항목이면 작성자 → write, 독자 → read, 그 밖에는 없음(404). 같은 `request_id`의 재시도는 같은 항목을 돌려준다.
- **항목 아래 문서(`placement='module'`)**: 같은 문서 엔진을 쓴다. `app_document_access`가 항목 권한과 문서 규칙 중 낮은 쪽을 쓰고, 명시적 공유는 그대로 적용하며, 보관된 항목·모듈 아래에서는 read로 제한한다. 번호 대장은 볼 수 있는 대상만 보인다(비공개 항목의 존재를 번호로 드러내지 않는다).
- **이동 일반화**: 대상은 `target_workspace_id` + 선택적 `target_item_id`다. 같은 워크스페이스 안 위키↔항목 이동을 지원하고, 미리보기는 항목 공개 범위까지 반영한다. 항목은 대상 워크스페이스에 속하고, 볼 수 있고, 쓸 수 있고, 보관되지 않아야 한다.
- **웹**: 워크스페이스 화면의 모듈 목록과 만들기(관리자), 모듈 화면(항목 목록·만들기, 이름·키 변경, 보관), 항목 화면(제목·설명·공개 범위, 보관, 문서 목록·만들기), 항목 문서 경로 `/w/{ws}/i/{item}/d/{doc}`, 이동 대화상자의 위키·항목 대상.
- **검토 (마이그레이션 0007)**: 별도 검토자가 높음 등급 없이 10건을 찾았고 모두 반영했다.
  - 보관 아래에서도 소유 권한자는 노출을 줄이는 동작(공유 철회, 공개 링크 철회, 비공개 전환, 복제 허용 끄기)을 할 수 있다. 보관 상한을 뺀 `app_document_base_access`로 판정하며, 넓히는 변경은 409다. 응답의 `can_manage`가 이를 알린다.
  - 비공개 항목 아래 문서의 사본은 비공개다.
  - 문서 응답의 `item_id`는 그 항목을 볼 수 있는 사람에게만 준다.
  - 저장·이동·항목 문서 생성·항목 생성은 항목과 모듈을 공유 잠금한 뒤 판정한다. 그사이 보관되어 갱신 정책이 행을 거르면 500이 아니라 403이다.
  - 같은 모듈의 다른 항목으로 옮기면 번호를 유지한다.
  - 다른 위치에서 쓴 `request_id`를 재사용하면 409다.
  - 항목·키·항목 문서의 워크스페이스 일치를 복합 외래 키로 강제한다.
  - 이동 미리보기에 번호 변경 여부와 유지되는 활성 공개 링크 수를 보여 준다.
  - 회귀 테스트 2건을 더했다.
- **받아들인 한계**: 항목과 문서가 모듈 번호열을 함께 쓰므로 번호의 빈자리(DEV-1, DEV-3)로 볼 수 없는 대상이 있다는 사실은 드러난다. 제목·종류·id는 드러나지 않는다. 공통 번호(D-48)의 결과로 받아들인다.
- **다음 조각으로 미룬 것**: `@` 링크·사용자 언급·역링크, 워크스페이스 키(`@A/`)는 키 대장과 같은 방식으로 다음 조각에서 둔다. 항목 멤버·리더는 40절에서 했다.

## 16. @ 링크·사용자 언급·역방향 링크·알림 (2026-10-17, D-18·D-27·D-39, 마이그레이션 0008)

- **본문 형식**: 참조는 인라인 노드 `{"type":"reference","attrs":{"kind":"item|document|user","id":"<uuid>"}}`이다. 표시 이름(번호·제목)은 저장하지 않는다. 대상이 나중에 비공개가 되어도 본문을 읽는 사람에게 제목이 남지 않게 하기 위해서다. 서버는 다른 속성·글자가 붙은 참조 노드, 표준 형식이 아닌 id, 상한(`FVOCI_MAX_REFERENCES_PER_DOCUMENT`, 기본 500)을 넘는 개수를 400으로 거부한다.
- **표시**: 화면에 보일 때 `POST /api/references/resolve`로 묻는다(한 번에 100개, 같은 틱의 요청을 모아서). 볼 수 없는 대상과 없는 대상은 같은 모양(`visible: false`)이며 화면에는 "볼 수 없는 대상"으로 보인다. 링크는 내부 id 기준이라 키·제목이 바뀌어도 이어진다. 비로그인 공개 보기는 참조 노드를 자리표시 글자로 바꿔 내부 id를 내보내지 않는다.
- **기록**: 저장 트랜잭션 안에서 정의자 함수 `app_record_document_references`가 저장된 본문에서 링크·언급을 뽑는다(클라이언트가 보낸 목록을 믿지 않는다). 앱 역할은 링크·언급·알림 표에 쓸 수 없다. 사본은 링크만 기록하고 알림은 만들지 않는다.
- **언급 알림**: 이번 저장에서 새로 넣은 언급(직전 리비전 본문에 없던 사람)에만, 저장한 사람 이름으로 만든다. 문서마다 사람마다 한 번(기본 키로 보장)이다. 언급된 사람이 그 시점에 문서를 볼 수 없으면 알리지 않고, 언급이 권한을 주지도 않는다. 알림 목록은 지금 문서를 볼 수 있는 것만 보여 주므로 접근이 철회되면 사라진다. 언급한 사람 이름은 워크스페이스를 함께 쓰거나 그 개인 노트 소유자일 때만 보인다.
- **역방향 링크**: 문서·항목 화면 아래에 "이곳을 가리키는 문서"를 보여 준다. 출발 문서를 볼 수 있는 것만 나오고, 대상을 볼 수 없으면 404다.
- **@ 입력**: 편집기에서 `@`를 치면 사람·항목·문서 후보가 나온다(제목·이름 일부, 또는 `DEV-3`처럼 번호, 예전 키도 별칭으로). 범위는 문서의 워크스페이스이고, 개인 노트에서는 내가 속한 모든 워크스페이스와 내 노트다. 같은 이름의 후보는 종류·번호·워크스페이스로 구분한다. 키보드(위아래·Enter·Tab·Esc)로 고르고, 편집기에 `aria-activedescendant`로 현재 후보를 알린다. 편집 중 참조 클릭은 선택이고 Ctrl·Cmd 클릭이나 읽기 화면에서 이동한다.
- **검색 범위 제한**: 후보 검색은 내가 속한 워크스페이스 id 목록과 인덱스로 먼저 좁혀 RLS 판정 함수가 다른 테넌트의 행에 돌지 않게 한다. 제목 부분 검색 인덱스(trigram)와 요청 빈도 제한은 사용량을 보고 정한다(O-26). 본격 검색은 Meilisearch(D-39) 조각에서 한다.
- **다른 사용자의 권한 판정**: 언급 알림 판정은 같은 권한 함수를 쓰려고 정의자 함수 안에서 `app.user_id`를 잠시 바꾸고 되돌린다(앱 역할에는 권한 없음). 예외가 나면 트랜잭션 되돌림이 설정도 되돌린다.
- **검토**: 별도 검토자가 높음 등급 없이 6건을 찾았고 모두 반영했다. 사본 편집이 원본 언급을 다시 알리던 문제(새 언급만 알림), 후보 검색의 전 테넌트 스캔(범위 선제한·인덱스, 해석 일괄 질의·상한 100), Rust와 DB의 UUID 형식 차이, 참조 노드의 추가 속성, 알림의 언급자 이름 노출 범위, 사용자 전환 함수의 복원 방식 확인이다.
- **검증**: 통합 테스트 5건(볼 수 없음 = 없음, 라벨 거부, 역링크와 출발 문서 권한, 언급 한 번·볼 수 없는 사람 제외·권한 미부여·철회 시 사라짐·읽음 표시, 후보 검색의 비공개·예전 키·사람 범위, 사본 편집 재알림 없음), 단위 테스트(참조 검사, 공개 보기 치환, 번호 입력 해석), 웹 테스트 4건.
- **마이그레이션 제자리 수정 확인**: 0007은 새 파일로 추가했고 고친 적이 없다. 0008은 커밋 `abf111c` 뒤 검토 반영으로 제자리 수정했다(원격에 올린 적 없고, 수정 전 내용이 적용된 영속 DB도 없다). 테스트 DB는 매 실행 tmpfs의 빈 DB이며, 남은 컨테이너·볼륨이 없음을 확인한 뒤 `cargo xtask test --oidc --keep`으로 만든 새 DB에서 `_sqlx_migrations`의 8개 체크섬이 현재 파일의 SHA-384와 모두 같고, 설치된 함수가 최종본(언급 비교 로직 포함, `SET app.user_id` 없음)임을 확인했다. sqlx는 적용된 마이그레이션의 체크섬이 다르면 실행을 거부하므로 옛 SQL이 남은 DB에서 조용히 PASS가 나지 않는다.
- **워크스페이스 구분(O-33)**: `이름/질의`로 다른 워크스페이스 대상을 찾는다(예: `@연구실/DEV-1`). 같은 이름의 워크스페이스는 모두 범위에 들고, 후보에는 자동 배지 색과 같은 이름일 때 UUID 끝 네 글자가 붙는다. 사이드바·홈·이동·복제 대상도 같은 표시를 쓴다. 통합 테스트 1건(같은 이름 두 곳 모두 검색·워크스페이스 id로 구분, 남의 같은 이름 공간 제외, 키·제목 변경 뒤 링크·역링크 유지)과 웹 테스트 2건.
- **남은 것**: 알림 이메일은 선택 기능으로 나중에(D-39). 리비전 diff에서 참조 노드 표시는 리비전·diff 조각에서 같은 자리표시 규칙(`@[종류:id]`)을 쓴다.

## 17. 리비전 기록·비교·복원 (2026-10-17, D-27·D-29·D-57·D-58, 마이그레이션 0009)

- **기록**: 문서를 볼 수 있는 사람이 리비전 목록(최신순 100개씩, `before`로 이어 보기)과 리비전 하나의 내용을 본다. 저장한 사람 이름은 users RLS 범위(나 자신·워크스페이스를 함께 쓰는 사람)에서만 보이고 아니면 "알 수 없음"이다. 이동한 문서는 기록도 함께 옮겨진다(이동 미리보기에 이미 표시).
- **비교**: 웹에서 본문을 블록마다 한 줄(제목 `#`, 목록 `-`, 인용 `>`, 코드 들여쓰기, 참조 `@[종류:id]`)로 펴고 jsdiff(`diff` 8.0.4, BSD-3-Clause, 허용 목록 안)로 줄 단위 차이를 보여 준다. "지금과 비교"는 복원하면 생기는 변화, "직전과 비교"는 그 리비전에서 바뀐 것이다. 추가·삭제는 색과 함께 화면 낭독기용 "추가:"·"삭제:" 글자로도 알린다.
- **복원**: 쓰기 권한자가 정의자 함수 `app_restore_document_revision`으로 한다. 옛 내용으로 새 리비전을 만들고(`restored_from` 기록) 이전 기록은 지우지 않는다. 저장과 같은 규칙을 따른다. 보고 있던 리비전이 지났으면 409와 서버 최신본, 같은 사람의 같은 `request_id` 재시도는 그 복원이 아직 최신일 때만 성공, 내용이 이미 같으면 리비전을 늘리지 않는다. 감사 기록(`restore_revision`, 원본·새 리비전 번호)을 남기고 링크·언급을 다시 기록한다. 연결된 업무 완료 상태·강의 진도는 되돌리지 않는다(D-58, 업무·진도가 생기면 같은 규칙).
- **복원과 언급·한도·형식**: 복원은 링크를 다시 기록하지만 언급 알림은 만들지 않는다(새 언급을 쓰는 것이 아니며, 사본이나 옛 리비전의 언급을 복원한 사람 이름으로 알리지 않는다). 복원할 내용도 지금의 한도(제목·본문 크기·참조 수)를 지켜야 하고, 저장 형식(`schema_version`)이 다른 리비전은 409로 거부한다(형식 변환이 생기면 변환 뒤 복원). 저장과 복원은 같은 `request_id`를 서로 재사용할 수 없고(400), 같은 `request_id`로 다른 리비전을 복원하는 것도 재시도로 보지 않는다.
- **검토**: 별도 검토자가 지적한 잠금 전 권한 확인 누락(볼 수 없는 문서를 잠가 남의 저장을 막을 수 있음), 저장·복원 간 `request_id` 재사용의 엉뚱한 성공, 복원 언급 알림, 이동과 겹칠 때의 항목 잠금, 형식·한도 검사 누락을 고쳤다. 권한을 먼저 확인한 뒤 문서 → 항목·모듈 순서(이동과 같음)로 잠그고 다시 확인한다. 회귀 테스트 1건(남의 노트·다른 워크스페이스 404, 읽기 공유 403, `request_id` 교차 400, 복원 언급 알림 없음, 보관 항목 403).
- **편집 중 복원**: 화면은 복원 결과로 편집기를 다시 만든다. 저장하지 않은 편집은 편집기의 초안 보존(D-53)이 남겨 두고 복구를 제안하며, 그 초안을 저장하면 충돌 화면으로 비교한다.
- **검증**: 통합 테스트 3건(목록·이어 보기·내용·비멤버 404·없는 리비전 404, 복원이 새 리비전을 만들고 기록 보존·재시도·같은 내용·감사 기록, 독자 403·오래된 기준 409·복원 뒤 다른 저장이 있으면 재시도 409), 웹 테스트 3건(블록 펴기, 줄 차이, 응답 해석).

## 18. 문서 트리 (2026-10-17, D-27, 마이그레이션 0010)

- **범위**: 위키, 항목 아래 문서, 개인 노트에서 부모·자식과 순서를 둔다. 부모는 같은 자리(워크스페이스·항목·소유자)에 있어야 하며 제약 트리거로 강제한다(NULL 열과 부모 행 참조 때문에 CHECK·복합 외래 키로는 안 됨). 트리 변경은 권한을 바꾸지 않는다.
- **권한(D-47 구현 선택)**: 순서·부모 바꾸기는 그 문서의 쓰기 권한과 새 부모·앞 형제를 볼 수 있는 권한이 필요하다. 보관 아래에서는 읽기 전용이라 할 수 없다. 부모가 바뀌면 감사 기록(`reparent`)을 남기며 리비전은 늘지 않는다. 앱 역할은 `parent_id`·`position`을 직접 바꿀 수 없고 정의자 함수만 바꾼다.
- **숨은 부모**: 부모를 볼 수 없는 사람에게는 `parent_id`를 주지 않아 그 자식이 최상위로 보인다. 형제 판단도 호출한 사람이 보는 부모 기준이다. 받아들인 한계: 깊이 한도 오류와 "자기 하위 아래로는 옮길 수 없음" 오류는 볼 수 없는 조상·후손을 포함해 계산하므로 하위 트리 깊이나 경로 존재를 짐작할 단서가 될 수 있다. 문서 제목·id는 드러나지 않는다.
- **순서**: numeric 위치. 끼워 넣을 때 옮기는 문서 한 행만 바꾸고 가운데 값은 정확한 곱셈(`* 0.5`)으로 구해 반복해도 겹치지 않는다(같은 틈에 70번 끼워 넣는 회귀 테스트).
- **동시성**: 잠금 순서를 모든 경로에서 자리 잠금(advisory) → 문서 행 → 항목·모듈 → 번호 카운터로 통일했다. 자리 잠금은 같은 자리의 트리 변경을 직렬화해, 서로를 부모로 삼는 동시 변경이 순환을 만들지 못하게 한다. 부모를 지정한 생성은 번호 할당 전에 자리 잠금을 잡고, 이동은 행을 잠그기 전에 원래·새 자리를 키 순서로 잠근다.
- **생성**: `parent_id`를 함께 보내면 새로 만들 때만 그 아래 맨 끝에 놓는다. 같은 `request_id` 재시도는 기존 문서를 그대로 돌려준다(그사이 사용자가 옮긴 위치를 되돌리지 않음).
- **다른 자리로 이동**: 하위 문서는 원래 자리에 남아 이동한 문서가 있던 위치에 원래 순서대로, 그 원래 부모 아래로 붙는다(각각 감사 기록). 이동 미리보기에 남는 하위 문서 수(볼 수 있는 것)를 보여 준다.
- **깊이 한도**: `FVOCI_MAX_DOCUMENT_DEPTH`(기본 10, 2~100만 허용).
- **웹**: 위키·항목 문서·내 노트를 같은 트리 구성요소로 보여 준다(WAI-ARIA tree, 위·아래·오른쪽·왼쪽·Home·End·Enter). 쓰기 권한이 있으면 Alt+위·아래로 순서, Alt+오른쪽·왼쪽으로 들여쓰기·내어쓰기, Alt+Enter로 하위 문서, 같은 동작을 항목 메뉴로도 한다.
- **검토**: 별도 검토자가 찾은 교착 두 경우(자식 배치와 부모 이동, 부모 지정 생성과 이동), 재시도 생성의 위치 되돌림, 전체 테이블 탐색(인덱스를 쓰도록 자리별로 나눔), 가운데 값 정밀도 소진, 깊이 설정 범위, 남는 자식 순서·감사, 노트 정렬을 모두 고쳤다.
- **검증**: 통합 테스트 6건(생성·재시도·순서·순환·형제 조건·독자 403·감사, 같은 자리·숨은 부모·보이는 기준 형제, 이동 시 남는 자식, 깊이 한도·동시 반대 변경 순환 없음, 재시도·정밀도·남는 자식 순서, 이동·배치·생성 동시 실행 무교착), 설정 단위 테스트, 웹 테스트 2건.

## 19. Markdown 편집·읽기 미리보기·본문 체크리스트 (2026-10-17, D-27·D-30)

- **모드**: 편집기 위에서 서식 편집(기본), Markdown 원문, 미리보기를 고른다. Markdown은 추가 입력 방식이며 서식 편집 없이 Markdown을 알아야만 쓸 수 있게 만들지 않았다. 미리보기는 읽기 전용 렌더링이다.
- **손실 방지(D-27)**: Markdown 원문을 고치지 않고 돌아가면 원래 내용을 그대로 둔다. 문서가 Markdown으로 그대로 오가지 못하면(왕복 결과가 원래 JSON과 다르면) 들어갈 때 알리고, 원문을 고친 경우에만 원문에서 다시 만든 내용으로 저장한다. 자동 저장은 고친 원문에서 만든 내용으로 한다.
- **참조 왕복**: 참조 노드는 Markdown에서 `[reference kind="item" id="…"]`로 쓰고 다시 읽는다. 왕복해도 링크·언급이 사라지지 않는다(웹 테스트로 확인).
- **본문 체크리스트(D-30의 편집기 쪽)**: 도구 막대의 "체크리스트"로 완료·미완료 항목을 만든다. Markdown에 `- [x] 자료 수집`, `- [ ] 초안 작성`을 붙이거나 쓰면 상태와 내용이 그대로 항목이 된다. 리비전 비교에서도 `- [x]`·`- [ ]`로 보여 체크 변경이 차이로 드러난다. 편집기가 없는 화면의 체크리스트 입력과 저장 위치·본문 연결(O-04)은 업무 조각에서 함께 한다(같은 항목을 두 곳에 독립 저장하지 않는다는 D-30 원칙 유지).
- **의존성**: `@tiptap/markdown` 3.32.0(MIT, `marked` 17.0.6 MIT 포함), `@tiptap/extension-list` 3.32.0(MIT). 허용 목록 안이다.
- **검증**: 웹 테스트 3건(참조·체크리스트 왕복 무손실, 붙여넣은 Markdown 체크리스트의 상태 추출, 체크 변경의 리비전 차이). 브라우저에서 체크리스트 작성 → Markdown 모드 → 원문 수정 → 서식 편집 복귀 → 저장 확인.

## 20. 업무·하위 업무·담당자·업무 체크리스트·칸반 (2026-10-17, D-14·D-18·D-30·D-31·D-48·D-57, 마이그레이션 0011)

- **구조**: 업무는 항목에 속하고 모듈의 공통 번호(`DEV-12`)를 함께 쓴다. 하위 업무는 같은 항목 안 한 단계만이다(DB 함수가 순환·깊이를 막는다). 상태 `todo`·`in_progress`·`done`, 우선순위, 시작일·마감일(날짜만, `YYYY-MM-DD` 엄격 검사), 예상 시간(분), 태그.
- **권한**: 업무 권한은 항목 권한을 따른다(`app_task_access`, 보관된 항목은 읽기로 낮춘다). 보관 때문에 쓸 수 없으면 409, 권한이 없으면 403, 볼 수 없으면 404. 목록·상세·체크리스트 수 모두 같은 함수로 거른다.
- **담당자 (D-31)**: 여러 명이 하나의 완료 상태를 공유한다. 새로 넣는 담당자만 확인하며, 그 사람의 멤버 행을 공유 잠금한 채 항목을 볼 수 있는지 본다(제거·역할 변경과 경합하지 않음). 항목 비공개 전환·멤버 제거·역할 강등으로 볼 수 없게 된 담당은 같은 트랜잭션에서 빠지고 `unassign` 감사 기록(이유 `item_private`·`remove`·`leave`·`role_change`)을 남긴다. 고치기 요청의 `assignee_ids`는 전체 목록이라 생략할 수 없다.
- **충돌**: 필드 고치기는 `base_version`으로 보호하고 409에 서버 최신본을 돌려준다(입력은 화면에 남는다). 칸반 이동·상위 바꾸기는 버전 없이 마지막 요청이 이긴다.
- **칸반 순서**: 열(항목·상태) 안 numeric 위치의 정확한 가운데 값. 자릿수가 40을 넘으면 그 열을 1, 2, 3…으로 다시 매긴다. 잠금 순서: 업무 행 → 항목 공유 → 열 → 번호.
- **체크리스트 (D-30, O-04 일부)**: 업무 체크리스트는 DB 행(`checklist_items`)이고 문서 본문 체크리스트는 본문 노드다. 한 체크리스트를 두 곳에 따로 저장하지 않는다. 항목 추가는 `request_id`로 재시도해도 한 번만 들어간다. Markdown 붙여넣기는 상태·내용을 펼쳐 보여 주고 확인 뒤 기존 항목 뒤에 덧붙인다(교체하지 않음). 같은 체크리스트를 본문과 입력 영역에 함께 보여 주는 화면은 아직 없다(O-04의 나머지).
- **참조**: `@`로 업무를 고를 수 있고 링크는 업무 id로 저장된다. 업무 상세에 역방향 링크가 보인다.
- **웹**: 항목 화면의 업무 영역(칸반·목록 전환, 업무 만들기), 상세 대화상자(주소 `/w/{ws}/i/{item}/t/{task}`). 칸반 카드는 제목 버튼에서 Enter로 열고 Alt+왼쪽·오른쪽으로 열, Alt+위·아래로 순서를 바꾸며 결과를 화면 낭독기에 알린다. 목록 보기는 상위 업무 바로 뒤에 하위 업무를 둔다.
- **검증**: 브라우저(`cargo xtask dev --oidc` + Vite, Keycloak 테스트 계정): 칸반 카드 Alt+오른쪽으로 열 이동과 화면 낭독기 알림·초점 유지, Enter로 상세 열기(주소 반영), Markdown 체크리스트 붙여넣기 → 2개 펼침 → 덧붙이기(완료·미완료 유지), 담당자 지정 저장, Escape로 닫으면 같은 카드로 초점 복귀, 목록 보기의 열 순서와 하위 업무 묶음, 서버 재시작 뒤 세션·내용 유지. 대화상자를 닫을 때 항목 화면이 다시 마운트되어 페이지가 잠기던 문제를 이 확인에서 찾아 고쳤다. 통합 테스트 6건(번호 공유·권한·보관 409, 버전 충돌과 이동, 담당자 확인·비공개 전환·멤버 제거·감사, 강등 시 담당 해제와 기존 담당자 유지 저장, 체크리스트 재시도·독자 거부, 참조 대상과 동시 상위 바꾸기의 순환 방지). 웹 테스트(체크리스트 폼, 칸반 열·이동 계획·목록 순서).
- **남은 범위(D-31)**: 마일스톤·선행 관계·반복·일정·달력·간트·저장한 보기는 다음 조각. (저장한 보기·검색·필터·정렬·항목 달력은 46절에서 했다.)

## 21. 마일스톤·선행 관계·후속 재배치 승인·간트 (2026-10-17, D-31·D-33·D-50·D-57, 마이그레이션 0012)

- **마일스톤**: 마감일 하나만 가진 업무(`milestone`, DB 검사로 시작일 금지). 간트에 ◆로 보인다.
- **선행 관계**: 같은 항목 안의 업무 쌍마다 하나(`task_dependencies`). 종류(FS·SS·FF·SF)와 지연일은 D-91·90절. 후속 업무 쓰기 권한으로 추가·삭제하며 순환·자기 참조·다른 항목은 400, 독자는 403, 볼 수 없으면 404. 같은 항목의 관계 변경은 잠금으로 직렬화해 동시에 서로를 선행으로 걸어도 하나만 성공한다. `link_predecessor`·`unlink_predecessor` 감사 기록.
- **후속 재배치 (D-31 "영향과 충돌을 보여준 뒤 사용자가 승인")**: 날짜를 바꾸고 후속 업무가 있으면 화면이 먼저 `POST /api/tasks/{id}/schedule-plan`(D-91 전에는 `schedule-preview`)으로 제안을 받는다(읽기, 아무것도 바꾸지 않음). 제안은 바뀐 선행이 관계 종류·지연일대로 더 요구하는 만큼만 미는 최소 이동이고(90절) 기간은 유지한다(앞당기면 제안 없음, 날짜 없는 업무는 옮기지 않음). 사용자가 "후속 업무도 옮겨 저장"을 고르면 업무 고치기의 `successor_shifts`로 같은 트랜잭션에서 함께 옮긴다. 승인한 업무 중 하나라도 기준 버전이 지났으면 아무것도 바꾸지 않고 409 `shift_conflict`, 화면은 다시 계산해 묻는다. 후속이 아닌 업무를 옮기라는 요청은 400. 옮긴 업무마다 `reschedule` 감사 기록(이유 `predecessor_moved`). "이 업무만 저장"은 후속 업무를 그대로 둔다.
- **간트**: 항목 업무 영역의 세 번째 보기. 상위 업무 바로 뒤에 하위 업무, 막대·마일스톤·오늘 선. 막대는 보조 표시이고 줄마다 단추가 같은 내용(번호·제목·기간·상태·선행)을 글로 말한다. 가로 이동은 "2주 앞·오늘·2주 뒤" 단추로 키보드에서도 된다. 날짜는 UTC 날짜 번호로 계산해 시간대 때문에 하루가 밀리지 않는다(D-50). 달력·간트 라이브러리는 들이지 않았다: 필요한 것은 읽기 표시와 상세 열기이고 끌어서 옮기기는 없으며, 새 의존성은 허용 목록·접근성 확인 부담(O-30 대기 중)만 늘린다.
- **리뷰 반영(0011 동시성)**: 열 잠금을 업무 행보다 먼저 잡는다(다시 매기기와의 교착 방지). 다시 매기기는 그 열에 남은 행만 바꾼다. 볼 수 없게 된 담당 정리는 워크스페이스 잠금으로 직렬화해 비공개 전환과 역할 강등이 동시에 일어나도 놓치지 않는다. 새 담당자 목록은 한 번 정해 그 목록만 넣고(그사이 지워진 담당을 확인 없이 되살리지 않음) 멤버 행은 사용자 id 순으로 잠근다. 실제로 지운 경우에만 `unassign` 감사 기록. 잠금 순서: 열 → 업무 행(id 순) → 항목 공유 → 번호.
- **리뷰 반영(이 조각)**: 업무 행을 잠그는 모든 경로(고치기·옮기기·상위 바꾸기)가 `app_lock_task_rows`로 열 잠금을 (항목, 상태) 순으로 먼저 잡고 행을 id 순으로 잡는다. 잡는 사이 열이 바뀌었거나 교착이 감지되면 409(다시 시도)이며 아무것도 바뀌지 않는다. 승인 재배치는 선행 관계 잠금 아래에서 후속 여부를 확인하고, 같은 업무를 두 번 넣거나 기준 버전이 빠지면 거부한다. 제안은 이번에 옮겨진 선행이 원래보다 더 요구하는 만큼만이며(전부터 겹친 만큼과 완료한 업무는 그대로), 미리보기도 시작일 > 마감일을 거부한다. 화면은 미리보기 패널이 떠 있는 동안 폼을 잠그고 그때의 입력·기준 버전 그대로 보내며, 서버 기준 내용과 날짜가 다르면 언제나 미리 본다(충돌 뒤 다시 저장 포함). 이중 제출을 막고, 패널이 닫히면 저장 단추로 초점을 돌린다. 간트는 최대 400일까지 그리고 그 뒤의 업무는 따로 적는다.
- **검증**: 통합 테스트 3건(마일스톤 날짜 규칙, 관계 규칙·권한·감사·동시 순환 방지, 미리보기·승인 재배치·`shift_conflict` 무변경·후속 아님 400·감사), 단위 테스트 3건(최소 이동·앞당김·두 선행 중 늦은 쪽), 웹 테스트 2건(간트 배치·시간대). 브라우저: 간트 표시와 줄 단추 설명, 선행 마감을 미루고 저장 → 후속 2개 제안(초점이 안내 제목으로) → 함께 옮겨 저장 → 간트 갱신, 대화상자를 닫으면 간트 줄로 초점 복귀.
- **남은 범위(D-31)**: 회의·발표·행사 일정과 달력 보기, 반복 업무·일정(O-19 미결이므로 백그라운드 작업 없이 설계), 저장한 보기·검색은 다음 조각. (46절에서 했다.)

## 22. 회의·발표·행사 일정과 워크스페이스 달력 (2026-10-17, D-19·D-31·D-50·D-57, 마이그레이션 0013)

- **일정**: 항목에 속한 회의·발표·행사(`events`). 권한은 항목을 따르고(보관 아래 읽기, 볼 수 없으면 404) 번호는 붙이지 않는다(D-48은 항목·업무·문서). 시각 있는 일정은 UTC 시각과 일정에 지정한 IANA 시간대를 함께, 종일 일정은 날짜(끝 포함) 그대로 둔다(D-50). 모르는 시간대는 400(`pg_timezone_names`로 확인). 길이는 366일까지. 고치기는 기준 버전으로 막고 409에 서버 최신본, 날짜를 바꾸면 `reschedule` 감사 기록. 지우기는 행을 남기고 목록·달력에서 뺀다(`delete` 감사, 휴지통·복원은 D-39 조각).
- **달력 API**: `GET /api/workspaces/{id}/calendar?from&to&tz`. 기간(최대 62일) 안의 볼 수 있는 업무(시작~마감이 걸친 것)와 일정, 그리고 그 항목 이름. 시각 있는 일정의 날짜 경계는 보는 사람의 시간대(`tz`)로 정한다. 홈의 주간 달력(D-19)도 이 API를 쓴다.
- **웹**: 항목 화면에 일정 목록·추가·상세(고치기·두 번 묻는 지우기). 입력한 시각은 일정의 시간대 벽시계 시각으로 읽어 UTC로 보낸다(브라우저 Intl, 여름시간 경계 처리, 별도 시간대 라이브러리 없음). 보는 사람의 시간대와 다르면 일정 시간대 기준 시각도 함께 적는다. 워크스페이스 메뉴의 "달력": 일요일 시작 6주 달 격자(업무는 마감일 한 칸, 일정은 걸친 날마다), 좁은 화면은 날짜별 목록. 업무는 업무 상세로, 일정은 일정 대화상자로 열고 닫으면 그 칸으로 초점이 돌아온다.
- **검증**: 통합 테스트 2건(권한·시간대·재시도·버전 충돌·감사·지우기, 달력의 기간·보는 사람 시간대 경계·비공개 항목 숨김·기간 한도·모르는 시간대·비멤버 404), 웹 테스트 4건(시간대 변환·여름시간·달 격자·날짜별 배치). 브라우저: 항목에서 일정 만들기 → 달력 표시 → 대화상자에서 날짜 고치기 → 달력 갱신 → 지우기(확인) → 좁은 화면 목록.
- **남은 범위(D-31)**: 반복 업무·반복 일정은 다음 조각. 백그라운드 작업 구조(O-19)가 정해지지 않았으므로 반복 생성은 요청 처리 안의 순수 계산으로 설계하고 정해진 시각에 미리 만드는 일은 O-19 결정 뒤로 둔다.

## 23. 반복 업무·반복 일정 (2026-10-17, D-31·D-50·D-57, 마이그레이션 0014)

- **규칙(구현 선택, RFC 5545 전체가 아님)**: 매일·매주(ISO 요일)·매달·매년, 간격 1~99, 끝은 날짜 또는 횟수(첫 회차 포함, 1~1000). 기준일이 첫 회차다. 매달 31일·매년 2월 29일처럼 없는 날은 건너뛴다. 펼치기는 DB 함수 `app_recurrence_dates` 하나가 하고 업무·일정이 함께 쓴다(새 의존성 없음).
- **반복 업무 (D-31 "미완료는 남기고 다음 회차는 원래 일정대로")**: 회차마다 실제 업무 행이 있고 완료 상태는 회차별이다. 첫 회차가 틀이며 규칙은 거기서 바꾼다(`PUT /api/tasks/{id}/recurrence`, 버전 없이 마지막 요청 우선, `set_recurrence` 감사). 새 회차는 틀의 지금 내용(제목·설명·우선순위·태그·예상 시간·마일스톤)과 기준일과의 날짜 간격을 따르고, 체크리스트는 체크를 푼 채, 담당자는 지금 그 항목을 볼 수 있는 사람만 옮긴다. 하위 업무·선행 관계는 옮기지 않는다. 이미 만든 회차는 규칙·틀을 바꿔도 그대로이고, 멈추면(규칙 없음) 남은 회차는 그대로 둔다. "오늘"은 규칙에 지정한 시간대 기준이다(D-50).
- **회차를 만드는 때**(27절 이후: 작업 프로세스가 15분마다도 맞춘다): 백그라운드 작업 구조가 정해지지 않았으므로 쓰기 요청 안에서만 만든다. 규칙을 정하거나 바꿀 때, 반복 업무를 고친 뒤(별도 트랜잭션), 쓰기 권한이 있는 사람이 항목을 열 때 화면이 보내는 `POST /api/items/{id}/recurrences/sync`. 오늘까지의 회차와 그다음 한 회차를 만들고 한 번에 50개까지(나머지는 다음 요청). 같은 회차는 (반복, 날짜) 고유 제약으로 한 번만 생기고 동시 요청도 첫 회차 행 잠금으로 줄을 선다. 감사 기록의 `create`는 그 요청을 보낸 사람이 주체이고 `reason: recurrence`·반복 id·회차 날짜가 붙는다(정해진 시각에 미리 만드는 일은 O-19 결정 뒤). 독자는 403, 보관 항목은 409.
- **예정 회차(읽기)**: 달력은 아직 만들지 않은 회차를 계산해 "예정"(점선)으로 보여 주며 아무것도 만들지 않는다(독자도 같음, 테스트로 행·감사 수 불변 확인). 누르면 틀(첫 회차)을 연다. 칸반·목록·간트에는 실제 회차만 나온다.
- **반복 일정**: 행을 만들지 않고 달력에서 펼친다. 시각 있는 일정은 일정 시간대의 같은 벽시계 시각을 UTC로 바꾼다(여름시간이 바뀌어도 09:00은 09:00, 뉴욕 2026-11-01 경계 테스트). 고치기는 반복 전체에 적용되고, 회차 하나 빼기만 지원한다(`excluded_dates`, `exclude_occurrence` 감사). 회차 하나만 고치기는 하지 않는다.
- **일정·달력 리뷰 반영**: 업무 행 잠금은 `FOR NO KEY UPDATE`(외래 키 확인과의 교착 방지), 일정 시간대는 브라우저가 아는 이름만(posix/·right/·Factory 등 거부), 화면은 모르는 시간대를 UTC로 보이고 입력은 거부, 없는 시각(여름시간 시작)은 음의 오프셋 지역에서도 뒤로 민다, 409 중 기준 버전 충돌만 서버 최신본으로 다루고 나머지(보관·다시 시도)는 오류로 보인다(문서 저장·복원도 같이), 자정에 끝나는 일정은 다음 날에 걸치지 않는다, 일정 날짜 변경 감사는 시간대 변경도 남기고 UTC ISO로 적는다, 종일↔시각 전환은 그 일정의 날짜를 쓴다, 지운 뒤 초점은 영역 제목으로. 지우기는 표시만 하고 행을 남기는 것은 D-39(휴지통·복원)에 맞춘 구현 선택이다.
- **검증**: 통합 테스트 4건(규칙 펼치기: 31일·2월 29일·격주 요일·횟수·끝 날짜, 반복 업무: 미완료 유지·원래 일정·동시 동기화·독자 403·틀 변경이 기존 회차 불변·읽기는 예정만·멈춤, 따라잡기 50개 상한과 보관 409, 반복 일정의 여름시간·회차 빼기). 웹 테스트(시간대 경계·모르는 시간대·예정·자정 끝). 브라우저: 반복 편집기로 매주 수요일 정하기 → 회차 2개(오늘까지+다음) → 달력의 예정 회차, 일정에 매주 반복 → 달력 3회 → 한 회차 빼기.
- **리뷰 반영(마이그레이션 0016)**: 규칙을 바꾸거나 다시 켜면 그날(규칙 시간대)부터만 만들고 보인다(`effective_from`, 지난 날짜를 한꺼번에 채우지 않음, 횟수는 여전히 기준일부터). 펼치기는 하루씩 훑지 않고 회차 단위로 계산하며 날짜는 1900~2200년만 받는다(먼 기간 요청이 커지지 않게). 기준일은 요일이 규칙에 없어도 첫 회차이고, 끝 날짜는 기준일보다 앞설 수 없다. 항목 전체 동기화는 세 열을 먼저 잠근다(교착 방지). 반복이 켜진 첫 회차는 하위 업무가 되거나 날짜를 모두 잃을 수 없다(DB 트리거). 규칙 저장 뒤 회차 맞추기가 실패해도 규칙 저장은 성공으로 두고 다음 요청에서 이어 한다. 반복 아닌 일정은 저장된 시각 그대로(여름시간 끝의 겹친 시각), 일정 날짜를 옮기면 뺀 회차도 옮긴다, 보관 항목의 예정 회차는 숨긴다, 달력의 보는 사람 시간대도 같은 이름 규칙. 화면은 틀린 간격·횟수를 조용히 바꾸지 않고 알리며, 반복 저장 뒤 일정 입력을 지우지 않는다.

## 24. 스톱워치·개인 시간 기록 (2026-10-17, D-08·D-19·D-32·D-50, 마이그레이션 0015)

- **소유와 비공개**: 시간 기록은 사용자 소유다(RLS: 본인만). 워크스페이스 관리자에게도 보이지 않고 워크스페이스 감사 기록에도 남기지 않는다. 고친 내용은 본인만 보는 수정 이력(`time_entry_revisions`)에 이전 값으로 남는다.
- **타이머**: 시작·일시정지·재개·종료. 진행 중(측정 또는 일시정지)인 타이머는 사용자당 하나이며 모든 워크스페이스를 통틀어 센다(부분 고유 색인 + 사용자별 잠금, 다른 타이머가 있으면 409). 서버 시각으로 재므로 앱을 닫아도 이어지고, 다시 접속하면 머리글에 그대로 보인다. 길이 = 끝 − 시작 − 멈춘 시간.
- **기록**: 수동 추가(같은 요청 id면 한 번만), 고치기(기준 버전, 진행 중이면 시작·종류·메모·대상만), 지우기(표시만, 휴지통은 D-39 조각). 종류는 작업·순공부(강의 시청 제외)·강의 시청·휴식·기타로 나눠 순공부와 강의 시청을 구분한다(D-32). 연결 대상(업무·문서·항목)은 측정 중에도 뒤에도 정할 수 있고(@ 링크와 같은 후보 검색), 연결할 때 볼 수 있어야 하며 나중에 볼 수 없게 되면 기록은 남고 이름만 가린다.
- **집계**: `GET /api/me/time-summary` 일·주(월요일 시작)·월, 보는 사람의 시간대(D-50), 기록은 시작한 날에 센다(자정을 넘는 기록도 나누지 않음). 화면 "내 시간": 한 주의 날짜·종류별 합계, 이번 달 합계, 날짜별 기록, 직접 기록하기.
- **공유(D-32 "선택한 집계만")**: 워크스페이스마다 켜면 그 워크스페이스 업무에 연결한 내 시간의 합계만 함께 보인다(기록·메모·시각은 공유하지 않음). 업무 상세의 "시간": 예상 시간, 내 시간, 공유를 켠 멤버들의 합계와 사람 수, 이 업무로 타이머 시작(다른 타이머가 돌고 있으면 끝낼지 먼저 묻는다).
- **검증**: 통합 테스트 3건(타이머 하나·재시도·동시 시작 하나만·일시정지 멱등·종료 뒤 재개 409·관리자에게 안 보임(API 404, RLS 0행, 감사 0행), 수동 기록·볼 수 없는 대상 404·시간대별 일·주 합계·버전 충돌·수정 이력·지우기, 공유한 사람만 합계에 들어감·끄면 빠짐·비멤버 404). 웹 테스트(표시·진행 중에만 흐름). 브라우저: 업무에서 타이머 시작 → 머리글 표시 → 다른 화면으로 이동해도 계속 → 일시정지에서 멈춤 → 종료 → 내 시간 목록 갱신 → 기록을 1시간 순공부로 고치기 → 수정 이력 → 업무 시간에 반영.
- **리뷰 반영(마이그레이션 0018)**: 업무의 공유 합계와 내 시간은 끝난 기록만 센다(진행 중인 남의 타이머로 시작·멈춤 시각이 드러나지 않게, 화면이 진행 중인 내 타이머를 더한다). 같은 요청 id로 다시 보낸 시작은 같은 기록을 돌려준다. 일시정지 중인 타이머의 시작은 멈춘 시각 뒤로 옮길 수 없다. 워크스페이스에서 빠지면 그 워크스페이스의 합계 공유 선택을 지운다. 기록 고치기는 고치지 않은 칸의 원래 초 단위 값을 그대로 보낸다(입력란이 분 단위라 길이가 바뀌던 문제). 다른 탭·기기에서 바뀐 타이머는 409·404를 받거나 탭으로 돌아올 때 맞춘다.
- **남은 범위(D-32)**: 강의 회차·공부와의 연결과 영상 길이·배속 기준 예상 시간은 강의·스터디 조각에서. 사용자 시간대 설정 저장은 아직 없고 브라우저 시간대를 쓴다(48절에서 설정을 더했다).

## 25. 홈 (2026-10-17, D-19, 마이그레이션 0017)

- **구성**: 오늘 날짜, 빠른 추가(메모, 항목을 고른 업무, 항목을 고른 종일 일정), 실행 중인 스톱워치, 이번 주(월~일) 일정·마감(모든 워크스페이스의 달력 API를 모음), Up Next(기한 지남·오늘·7일 안, 내가 담당이거나 담당 없이 내가 만든 미완료 업무, 보관 항목 제외), 고정·즐겨찾기, 진행 중인 항목(최근 활동 순 8개), 최근 작업(내가 마지막으로 고친 문서·업무), 읽지 않은 알림, 받은 초대와 워크스페이스 목록. 모두 볼 수 있는 것만 나온다(RLS).
- **진행률(D-19 "활동의 의미에 맞게")**: 지금은 업무 완료 비율만 보이고 다른 종류와 합산하지 않는다. 강의 이수율·공부 목표 달성률은 각 모듈 조각에서 더한다.
- **고정·즐겨찾기**: 항목·문서·업무 화면의 단추로 켜고 끈다. 본인만 보는 표시이며 켤 때 볼 수 있어야 하고(아니면 404), 나중에 볼 수 없게 되면 홈에서 사라진다. 한 사람당 종류마다 100개.
- **검증**: 통합 테스트 1건(Up Next 구분과 담당·완료·남의 업무 제외, 최근 작업, 진행률, 볼 수 없는 대상 고정 404, 고정은 본인에게만, 항목을 볼 수 없게 되면 고정·Up Next에서 사라짐, 모르는 시간대 400). 브라우저: 기한 지남·오늘·다가오는 일 구분, 이번 주 마감 표시, 고정 항목, 진행률 1/4, 빠른 추가로 메모 만들기와 최근 작업 갱신.
- **리뷰 반영**: 홈 조회는 내가 속한 워크스페이스로 범위를 좁히고(전체 항목을 훑지 않게) 진행률·최근 활동을 한 번에 센다. 색인(업무 고친 사람·항목별 갱신·열린 업무 마감일, 문서 고친 사람)을 더했다. 빠른 추가는 쓰기 권한이 있는 항목만 고르게 한다(`can_write`). 워크스페이스 하나의 달력이 실패해도 나머지는 보인다. 고정 상한은 지금 볼 수 있는 대상만 세고 이미 있는 표시는 그대로 둔다. 고정 단추는 이름을 그대로 두고 눌림 상태로 알린다. 시간 기록 고치기 대화상자가 열리자마자 효과가 자기 자신을 다시 부르던 문제를 고쳤다.

## 26. 휴지통·복원·영구 삭제 (2026-10-17, D-39·D-41·D-57, 마이그레이션 0019)

- **묶음**: 지우면 휴지통 묶음 하나가 생긴다. 문서는 하위 문서까지, 업무는 하위 업무까지, 항목은 항목 하나(그 안의 업무·문서·일정은 항목이 안 보이면 함께 안 보인다), 일정은 하나. 되살리기·영구 삭제는 묶음 단위라 부모 없이 자식만 돌아오는 상태가 생기지 않는다. 부모가 따로 지워져 있으면 되살린 것은 최상위로 온다.
- **숨김**: 지운 것은 접근 함수(`app_item_access`·`app_document_base_access`·`app_task_access`·`app_event_access`)가 없는 것으로 본다. 그래서 목록·상세·참조 해석·역방향 링크·달력·홈·검색 후보·시간 기록 대상 이름·고정 표시에서 따로 고치지 않고 모두 빠진다. 반복 동기화는 첫 회차가 지워진 반복을 건너뛴다.
- **권한**: 문서는 문서 소유 권한(관리자·개인 노트 소유자)이나 쓰기 권한이 있고 내가 만든 문서, 업무는 업무 쓰기 권한, 항목은 항목 소유 권한. 독자는 지울 수 없다. 휴지통은 워크스페이스 관리자는 전부, 작성자는 자기가 지운 것만 보고 되살린다. 개인 노트 휴지통은 소유자만. 영구 삭제는 관리자(개인 노트는 소유자)만.
- **보존 기간(D-41)**: 워크스페이스마다 관리자가 1~3650일로 정한다(기본 30일, 지금 값이 이미 지운 것에도 적용). 기간이 지나면 되살릴 수 없고(409) "만료"로 남는다. 실제 삭제는 관리자가 "만료된 N개 영구 삭제"나 묶음별 영구 삭제로 한다. 기간이 지나면 작업 프로세스가 매시간 영구 삭제한다(27절).
- **영구 삭제**: 문서는 리비전·링크·언급·알림·공유·공개 링크와 함께, 업무는 담당·체크리스트·선행 관계·반복 정보와 함께, 항목은 그 안의 업무·문서·일정·반복과 함께 지운다. 번호는 다시 쓰지 않는다(번호 기록은 남김, D-17). 시간 기록은 개인 소유라 남고 대상 이름만 사라진다.
- **감사(D-57)**: `delete`·`restore`·`purge`(묶음 id·함께 지운 수), 보존 기간 변경. 개인 노트는 소유자 영역 기록.
- **시험 준비**: 만료를 시험하려고 `cargo xtask test`가 테스트 전용 마이그레이션 역할 주소(`FVOCI_TEST_MIGRATION_URL`, 테스트 비밀번호)를 넣고, 테스트는 그것으로 지운 시각만 옮긴다(`fixture_sql`). 앱 권한 검사를 대신하지 않는다.
- **검증**: 통합 테스트 2건(문서와 하위 문서 숨김·참조 해석 숨김·독자 403·남의 문서 403·휴지통은 지운 사람과 관리자만·다른 작성자 되살리기 404·같은 id와 번호로 되살리기·감사, 업무와 하위 업무·항목 지우기와 안의 업무 숨김·항목 소유자만·보존 기간 관리자만·만료 뒤 되살리기 409·만료 비우기·번호 재사용 없음·`purge` 감사). 브라우저: 위키 문서 지우기(확인) → 워크스페이스로 이동 → 휴지통 표시(지운 사람·보존 기한) → 되살리기 → 문서 다시 열림.
- **리뷰 반영(마이그레이션 0022)**: 공개 링크는 지운 문서·휴지통에 있는 항목의 문서를 보여 주지 않는다. 항목이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없고 받은 공유 목록에서도 빠진다. 문서를 지울 때 지우는 사람이 지울 수 없는 하위 문서는 함께 넣지 않고 지운 문서의 부모 자리로 옮겨 남긴다(남의 문서를 몰래 휴지통에 넣지 않게). 업무를 지울 때 잠그는 사이 생긴 하위 업무도 함께 넣는다. 영구 삭제한 반복 회차는 다시 만들지 않는다(`skipped_dates`). 되살리기는 지우기와 같은 잠금 순서를 따르고, 항목이 휴지통·보관 중이면 409, 지운 사람이 지금 그 항목에 쓸 수 없으면 404. 예전 방식으로 지운 일정은 휴지통 묶음으로 옮겼다. 화면은 지울 수 있을 때만 지우기 단추를 보이고(`can_delete`), 확인 단계로 바뀌면 초점이 확인 단추로 가며, 처리 중에는 다시 누를 수 없다.
- **남은 범위**: 워크스페이스 삭제(D-51, 소유자만, 휴지통으로)는 다음 조각.

## 27. 백그라운드 작업 (2026-10-17, O-19 해소, 마이그레이션 0021)

- **구조(O-19 권장안 채택)**: 한 바이너리가 API와 작업을 함께 갖고 `FVOCI_ROLE`로 나눈다. `all`(기본, 셀프호스트 한 프로세스), `api`(HTTP만), `worker`(작업만, HTTP를 열지 않음). 간격은 `FVOCI_WORKER_INTERVAL_SECS`(기본 60초). 별도 작업 서비스·외부 큐는 들이지 않았다.
- **주기 작업**: 만료된 휴지통 영구 삭제(매시간, D-41), 반복 업무 회차 맞추기(15분마다, D-31). `scheduled_runs` 표와 이름별 잠금(`app_claim_schedule`)으로 여러 프로세스가 떠 있어도 간격마다 한 번만 돈다. 한 묶음·한 반복이 실패해도 나머지는 계속하고 경고만 남긴다.
- **권한 경계**: 작업은 사용자 문맥 없이 `app_system_purge_expired`·`app_system_recurrence_tick`만 부른다. 둘 다 정책이 이미 정한 일(만료분만 삭제, 오늘까지와 다음 회차만 생성)만 하고 같은 일을 두 번 해도 결과가 같다. HTTP 경로는 이 함수를 부르지 않는다. 본문은 사용자 경로와 같은 함수(`app_sync_task_series_as`·`app_purge_batch_as`)를 쓰고 권한 확인만 건너뛴다. 작업용 DB 계정을 앱 계정과 나누는 일은 아직 하지 않았다(같은 `fvoci_app`로 실행). 운영에서 분리가 필요하면 다음 조각에서 `db bootstrap`에 작업 역할을 더한다.
- **감사(D-57)**: 작업이 한 일은 주체가 없으므로 `actor`를 비우고 detail에 `system: true`(휴지통은 `reason: retention`)를 남긴다. 반복 회차의 `created_by`는 반복을 정한 사람이다.
- **사용자 경로 유지**: 관리자의 만료 비우기와 쓰기 권한으로 항목을 열 때의 회차 동기화는 그대로 둔다(작업 프로세스가 없거나 멈췄을 때 따라잡기).
- **검증**: 통합 테스트 1건(만료된 묶음만 영구 삭제하고 system 감사, 지운 다음 회차를 다시 만들고 system 감사, 두 번째 실행은 할 일 없음, 동시에 두 번 돌려도 주기 작업은 한 번).

## 28. 통합 검색 (2026-10-17, D-39, 마이그레이션 0023·0024)

- **구성**: Meilisearch를 기존 `reqwest`로 REST 호출한다(별도 SDK 없음). `FVOCI_SEARCH_URL`·`FVOCI_SEARCH_KEY`가 없으면 검색은 제목만 PostgreSQL에서 찾는다(같은 거르기). `cargo xtask dev`는 개발 Meilisearch와 짧은 작업 간격(5초)을 넣는다.
- **색인 반영**: 문서·업무·항목·일정이 바뀌면 트리거가 `search_queue`에 올린다. 항목을 휴지통에 넣거나 되살리면 그 안의 문서·업무·일정도, 모듈 키가 바뀌면 그 모듈의 항목·업무도 함께 올린다. 백그라운드 작업은 대기열에 "처리 중"(2분) 표시만 하고 바로 끝낸 뒤 지금 내용을 읽어 Meilisearch에 넣거나 지우고, 그 사이 다시 오르지 않은 것만 지운다(Meilisearch를 기다리는 동안 사용자 저장이 대기열 잠금에 걸리지 않음). 묶음 넣기가 실패하면 하나씩 다시 해 문제 있는 한 건만 남기고, 10번 실패한 것은 다시 바뀔 때까지 쉰다. 색인이 없으면(처음 시작, Meilisearch 데이터 삭제) 매 주기에 만들고 살아 있는 것을 모두 다시 올린다. 시스템 함수는 사용자 문맥이 있으면 아무것도 하지 않는다. 검색 반영이 실패해도 휴지통 만료·반복 회차 작업은 따로 돈다.
- **색인 내용**: 종류·id·제목·번호(업무·항목만. 문서 번호는 멤버가 아닌 공유 대상이 번호로 키를 알아내지 않게 넣지 않는다)·본문 글자(ProseMirror에서 뽑은 일반 글자, 10만 자까지)와 거친 범위(워크스페이스, 개인 노트 소유자)만. 공유 대상 이메일·담당자·참조 표시 이름은 넣지 않는다.
- **권한(D-39 "오래된 색인으로 노출 금지")**: `GET /api/search`는 먼저 "내 워크스페이스 또는 내 노트 또는 나에게 공유된 문서"로 Meilisearch를 좁혀 후보를 60개씩(최대 5쪽) 받아 `app_search_visible`(종류별 접근 함수)로 다시 걸러 20개까지 돌려준다. Meilisearch를 기다리는 동안 DB 트랜잭션을 열어 두지 않고, 2초 안에 응답하지 않으면 제목 검색으로 대신한다. 받은 공유 문서는 최근 1000개까지 범위에 넣는다. 그래서 멤버에서 빠지거나 비공개로 바뀌거나 지운 것은 색인 반영 전에도 바로 빠진다. 번호(`WIKI-3` 등)는 호출자 권한으로 다시 읽어 멤버가 아닌 공유 대상에게는 보이지 않는다. 워크스페이스로 좁히면 멤버만(아니면 404).
- **화면**: 사이드바의 검색 단추와 Ctrl/⌘+K로 여는 대화상자. 250ms 뒤 찾고 늦게 온 이전 응답은 버린다. 위·아래 화살표로 검색어 칸과 결과 사이를 옮긴다. 결과는 종류·번호·제목·본문 근처 글자를 보여 주고 놓인 곳(워크스페이스 문서·항목 문서·업무·항목·일정은 항목)으로 연다.
- **파일 이름(0029)**: 첨부 파일 이름도 색인하고 파일 권한으로 거른다. 결과는 붙은 문서(또는 항목)로 연다. 문서를 휴지통에 넣거나 되살리면 붙은 파일도 다시 색인한다.
- **아직 없음**: 첨부 본문 검색(형식별 글자 뽑기 필요), 모듈·담당자·상태·날짜 필터(API는 종류·워크스페이스만).
- **검증**: 통합 테스트 2건(사람마다 볼 수 있는 것만: 남의 워크스페이스·비공개 항목 업무·남의 노트 제외, 관리자에게도 남의 노트 없음, 본문 검색과 근처 글자, 멤버 아닌 워크스페이스 404, 알 수 없는 종류 400, 멤버 제거 즉시 반영, Meilisearch 없을 때 제목 검색; 휴지통으로 보내면 색인에서 빠지고 되살리면 돌아옴, 항목을 지우고 되살리면 그 안의 문서·업무도 함께; 검색 서비스에 닿지 않으면 제목 검색과 다른 작업 계속). 웹 테스트(결과 경로). 브라우저(`cargo xtask dev --oidc` + Vite): Ctrl+K로 열기, 새 노트가 작업 간격 안에 검색됨, Tab·Enter로 결과 열기.

## 29. 워크스페이스 삭제 (2026-10-17, D-51·D-41, 마이그레이션 0025·0026)

- **지우기**: 소유자만, 워크스페이스 이름을 다시 입력해 확인한다(서버가 확인, 앞뒤 공백 무시). 소유권 인계가 진행 중이면 409(먼저 취소). 열린 초대는 `workspace_deleted`로 닫고, 되살리면 기한이 남은 것을 다시 연다. 지운 워크스페이스의 초대는 받은 초대 목록에서 빠지고 수락은 404. 초대·초대 수락·소유권 제안·수락은 먼저 워크스페이스 행을 잠그고 지워지지 않았는지 다시 봐서(지우기와 같은 잠금 순서) 지우는 동안 새 초대나 인계가 끼어들지 않는다. 화면은 멤버 화면 아래 "워크스페이스 삭제".
- **곧바로 닫힘**: 멤버 판정(`app_member_role`·`app_my_workspace_ids`·`app_shares_workspace`)이 지운 워크스페이스를 빼므로 그 위의 모든 접근 함수·RLS·검색 거르기·홈·시간 대상 이름이 함께 닫힌다. 멤버를 거치지 않는 길(받은 공유, 공개 링크)도 따로 닫았다. 반복 회차 작업은 지운 워크스페이스를 건너뛴다. 검색 색인은 그대로 두어도 결과 거르기가 막고, 영구 삭제 때 행 삭제 트리거로 빠진다.
- **되살리기·영구 삭제**: 소유자의 "내 노트 휴지통" 화면 아래 "지운 워크스페이스". 보존 기간은 그 워크스페이스의 휴지통 보존 일수(D-41, 기본 30일). 기간 안에는 모든 내용·멤버·번호가 그대로 돌아오고, 지나면 409이며 백그라운드 작업이 영구 삭제한다. 소유자는 기간 전에도 영구 삭제할 수 있다.
- **잠금 참고**: 영구 삭제는 워크스페이스 행을 먼저 잠근다. 지우기 전에 시작해 아직 끝나지 않은 쓰기와 부딪히면 PostgreSQL이 교착을 끊어 한쪽이 취소된다(사용자는 409 다시 시도, 작업 프로세스는 다음 주기). 데이터는 그대로다.
- **영구 삭제 범위(구현 선택)**: 업무·문서(리비전·공유·공개 링크·링크·언급·알림)·일정·항목·모듈·키·번호·그룹·초대·인계·시간 공유·멤버·휴지통 묶음을 지운다. 각자의 개인 시간 기록은 남기고 워크스페이스 연결만 끊는다(대상 이름은 사라짐). 감사 기록은 지우지 않고 워크스페이스 연결을 끊어 소유자 영역(`owner_user_id`)으로 옮기며, 영구 삭제 자체도 소유자 영역에 남긴다(작업 프로세스가 지우면 system 표시).
- **검증**: 통합 테스트 2건(관리자 403·이름 틀림 400·밖의 사람 404, 지운 뒤 목록·문서·멤버 화면·받은 공유·공개 링크·초대 수락·검색 모두 닫힘, 지운 목록은 소유자만, 되살리면 같은 번호·공유 복구, 인계 중 409; 모듈·항목·하위 업무·일정·항목 문서·휴지통 묶음·그룹까지 있는 워크스페이스 영구 삭제와 개인 시간 기록 유지, 만료 뒤 되살리기 409와 작업 프로세스 영구 삭제). 브라우저(`cargo xtask dev --oidc` + Vite): 이름 입력 전 버튼 비활성, 지운 뒤 사이드바에서 빠지고 휴지통 화면으로 이동, 되살리면 사이드바에 돌아옴.

## 30. 파일 저장소 (2026-10-17, D-06·D-61, O-31 해소)

- **구조**: `crates/server/src/storage/`. 서버 로컬 디렉터리(`FVOCI_STORAGE=local`, 기본, `FVOCI_STORAGE_DIR` 기본 `data/files`)와 S3 호환 저장소(`FVOCI_STORAGE=s3`, `FVOCI_S3_ENDPOINT`·`FVOCI_S3_REGION`(기본 `auto`, R2)·`FVOCI_S3_BUCKET`·`FVOCI_S3_ACCESS_KEY`·`FVOCI_S3_SECRET_KEY`)가 같은 여러 조각 업로드 모양(시작·조각 넣기·받은 조각 보기·마치기·그만두기·읽기·지우기)을 따른다. S3는 기존 `reqwest`로 요청하고 AWS 서명 v4만 직접 만든다(경로 방식 주소, 조각은 본문 SHA-256으로 서명). SDK·자체 S3 서버는 쓰지 않는다. 새 의존성은 이미 잠금 파일에 있던 `hmac`·`hex`·`bytes`·`futures-util`·`tokio-util`와 reqwest `stream` 기능(새로 잠금에 든 것은 `futures-macro`·`wasm-streams`, 둘 다 MIT OR Apache-2.0).
- **무결성**: 조각마다 SHA-256(S3는 서명으로 저장소가 확인), 마친 뒤 저장된 내용 전체를 다시 읽어 SHA-256과 크기를 계산한다(`digest`). 로컬은 조각과 완성본을 임시 파일에 쓴 뒤 이름을 바꿔 넣는다. 키는 서버가 만든 `[a-z0-9/-]`만 받는다.
- **한도(D-52, O-26 제안값)**: `FVOCI_MAX_FILE_BYTES` 1 GiB, `FVOCI_UPLOAD_CHUNK_BYTES` 8 MiB(5~64 MiB, S3 최소 조각과 Cloudflare 요청 한도 사이), `FVOCI_MAX_WORKSPACE_STORAGE_BYTES` 50 GiB, `FVOCI_UPLOAD_TTL_SECS` 24시간.
- **SeaweedFS(O-31 권장안 채택)**: 테스트 Compose에 `chrislusf/seaweedfs:4.48`(태그의 LICENSE: Apache-2.0)을 `mini` 모드로 두고 사용 통계 전송을 끈다(`-master.telemetry=false`, 기본은 켜짐). 자격 증명은 환경 변수의 명백한 테스트 값, 버킷은 시작 때 만든다. 시작 인자가 필요해 GitHub job 서비스로는 띄울 수 없으므로 S3 시험(`s3_*`)은 로컬 `cargo xtask test`와 CI의 Compose job에서만 돌고 컨테이너 job에서는 NOT RUN이다.
- **호환성 확인 결과**: 서명·여러 조각 시작·조각 넣기·같은 번호 다시 넣기·받은 조각 보기·마치기·그만두기·읽기·지우기·틀린 비밀 키 거부를 SeaweedFS와 로컬에서 확인. SeaweedFS는 받은 조각 목록에서 ETag를 숫자 엔티티(`&#34;`)로 쓰고 다시 올린 조각 번호를 두 줄로 보여 주므로, 마치기에는 조각을 넣을 때 받은 ETag를 쓰고 목록은 이어 올리기 확인에만 쓴다. R2 실검증은 승인된 테스트 대상·자격 증명이 없어 NOT RUN.
- **검증**: 통합 테스트 3건(로컬·S3 왕복, 틀린 비밀 키). 단위 테스트(키 검사, AWS 문서 예시와 같은 서명, 목록 XML 읽기).

## 31. 파일 첨부·버전·업로드 (2026-10-17, D-06·D-35·D-39·D-52·D-56, 마이그레이션 0027·0028)

- **모델**: 파일은 문서나 항목에 붙고 그 권한을 그대로 따른다(`app_file_access`: 문서 권한 또는 항목 권한). 문서를 공유받은 사람은 첨부도 읽고, 비공개 항목의 첨부는 항목을 볼 수 있는 사람만, 개인 노트의 첨부는 소유자만(관리자도 아님). 같은 파일에 새 내용을 올리면 v2, v3…으로 쌓이고 이전 버전은 남는다(올린 사람·시각·설명·크기·SHA-256). 구현 선택: 연구 자료(D-35)의 "자료 + 버전" 모델을 모든 첨부에 쓴다(P-06 방향). 다른 범위를 원하면 바꿀 수 있다.
- **업로드(D-06)**: `POST /api/uploads`로 세션을 만들고(같은 요청 id면 같은 세션, 크기·저장량 한도 확인), 조각을 `PUT /api/uploads/{id}/parts/{n}`로 보낸다(날것 본문, `x-content-sha256`, 크기는 조각 크기와 같아야 함, 같은 번호는 바꿈). 끊기면 `GET /api/uploads/{id}`로 받은 조각을 보고 나머지만 보낸다. 화면은 파일·대상별 요청 id를 브라우저에 기억해 새로 고침 뒤에도 같은 파일을 고르면 이어 올린다. `POST …/complete`는 모든 조각이 있어야 하고, 저장소에서 합친 뒤 내용 전체를 다시 읽어 크기를 확인하고 SHA-256을 기록한다(맞지 않으면 업로드를 닫고 409). 마치기는 여러 번 보내도 한 번만 처리한다. 마치기는 요청과 떨어진 작업으로 끝까지 돌아 연결이 끊겨도(프록시 시간 제한) 결과가 남고, 화면은 다시 고른 파일이 마치는 중이면 다시 올리지 않고 결과를 기다린다. 마치는 동안에는 기한을 늘린다. 저장소가 조각을 합치지 못하면(조각 표시 어긋남) 받은 조각을 비우고 한 번 더 받는다. 잠깐의 DB 충돌로는 업로드를 닫지 않는다. 같은 요청의 동시 시작은 하나로 합친다. 올리는 중 취소할 수 있다. 조각 경로만 조각 크기만큼 본문을 받는다(나머지 API는 문서 크기 기준).
- **한도(D-52)**: 파일 하나·조각 크기·워크스페이스(개인 영역은 사람마다) 총 저장량·업로드 기한을 설정으로(30절). 저장량에는 모든 버전(휴지통 포함)과 진행 중인 업로드가 든다. 빈 파일은 올릴 수 없다(S3 여러 조각 업로드의 제약). 업로드 조각은 서버 전체에서 `FVOCI_UPLOAD_CONCURRENCY`(기본 8, O-26 제안값)개까지 동시에 받고(본문을 읽기 전에 자리를 잡음), 나머지는 기다린다.
- **내려받기**: 권한 확인 뒤 서버를 거쳐 흘려보낸다. D-20("복제·다운로드 허용 여부는 소유자가 별도 설정")에 따라 문서 첨부는 문서를 고칠 수 있는 사람이나 소유자가 복제·다운로드를 허용한(`allow_copy`) 문서의 독자만 내려받는다. 독자도 안전한 형식은 브라우저 안에서 볼 수 있다. 항목에는 그런 설정이 없어 항목을 읽을 수 있는 사람이 내려받는다. 기본은 내려받기(`attachment`). 브라우저 안에서 열기는 PNG·JPEG·GIF·WebP·PDF·텍스트만이고 SVG·HTML 등은 늘 내려받기. `nosniff`, 제한적 CSP, `private, no-store`, 형식 문자열 정리, RFC 5987 파일 이름.
- **휴지통·영구 삭제(D-39·D-41)**: 파일 지우기는 휴지통 묶음(`file`)이 되고 모든 버전이 함께 가고 함께 돌아온다(붙은 문서가 휴지통에 있으면 먼저 문서를 되살려야 함). 문서·항목·워크스페이스를 영구 삭제하면 붙은 파일도 지워진다(삭제 트리거). 버전 행이 지워지면 저장소 키를 `storage_deletions`에 올리고 백그라운드 작업이 지운다(실패하면 10번까지 다시). 그만두거나 기한이 지난 업로드의 조각도 같은 길로 지운다. 쓸 수 있는 사람은 현재가 아닌 이전 버전을 하나씩 영구 삭제할 수 있다(D-56 정리). D-56의 "참조 중인 버전 보호"는 분석 기록이 아직 없어 해당 없음(분석 기록 조각에서 더한다). 파일만 담은 휴지통 묶음은 붙은 문서·항목이 영구 삭제되면 함께 끝난다. 보관된 항목(항목 문서 포함)의 파일은 이름 바꾸기·지우기·버전 지우기를 막는다. 저장소 정리는 하나씩 짧은 트랜잭션으로 하고 실패하면 횟수만큼 미뤄 다시 한다(10번째 실패는 오류 로그). S3 연결은 전체 시간 대신 연결 10초·읽기 대기 120초로 제한해 큰 파일 내려받기·확인이 끊기지 않는다.
- **화면**: 문서 화면과 항목 화면의 "첨부 파일": 올리기(여러 개, 진행률), 미리보기(안전한 형식만, 새 탭), 버전 기록과 이전 버전 내려받기, 새 버전, 이름 바꾸기, 휴지통으로.
- **아직 없음**: 파일 이름·본문 검색(D-39 첨부 본문 검색), Office·HWP 미리보기(O-10, 새 서비스가 필요하면 사용자 결정), Zotero 읽기.
- **검증**: 통합 테스트 3건(조각 순서 바꿔 보내기·이어 올리기·해시·크기·번호 오류, 마치기 중복, 읽기 전용 403·밖의 사람 404, 내려받기 머리글, 새 버전과 이전 버전, 이름 바꾸기·휴지통·되살리기·영구 삭제 후 저장소 정리; 파일·저장량 한도와 진행 중 업로드 포함, 그만두기, 기한 지난 업로드 닫기, 개인 노트 첨부는 소유자만; S3(SeaweedFS)로 왕복과 문서 영구 삭제 시 정리). 웹 테스트(조각 계산). 브라우저(`cargo xtask dev --oidc` + Vite): 9 MB 파일 두 조각 업로드와 텍스트 파일, 목록·크기·버전 표시, 텍스트 미리보기(한글), PDF 응답 머리글(inline·CSP). 내장 브라우저에 PDF 보기가 없어 실제 Chrome PDF 보기는 NOT RUN.

## 32. 반출·백업·복구 (2026-10-17, D-39·D-57, 마이그레이션 0030)

- **반출(사용자)**: 워크스페이스 관리자는 멤버 화면의 "데이터 반출"로, 누구나 사용자 메뉴의 "내 자료 내려받기"로 ZIP을 받는다. 안에는 `export.json`(구조 데이터: 모듈·항목·문서 본문(Tiptap/ProseMirror JSON)·업무와 담당·체크리스트·선행 관계·반복·일정·파일 버전 목록·감사 기록, 개인은 노트·첨부·시간 기록·고정/즐겨찾기·개인 감사 기록), `documents/`(문서마다 일반 글자), `files/`(모든 파일 버전 내용)가 있다. 요청한 사람의 권한(RLS)으로 읽으므로 볼 수 없는 것과 멤버의 개인 노트는 들어가지 않는다. 구조 데이터와 파일 목록은 한 문장(한 스냅숏)으로 읽는다. ZIP은 만들면서 바로 응답으로 흘려보낸다(임시 파일 없음, 첫 바이트가 곧 나가 프록시 시간 제한에 걸리지 않음). 그 사이 영구 삭제되어 저장소에 없는 내용은 건너뛰고 `missing.json`에 적는다. 동시에 만드는 반출은 `FVOCI_EXPORT_CONCURRENCY`(기본 2)개까지이고 넘으면 409. 내려받는 이름은 한글도 그대로(RFC 5987). 반출도 감사 기록에 남는다. Markdown은 편집기의 변환기(문서 화면의 Markdown 보기)를 쓰고 서버에 두 번째 변환기를 두지 않는다. ZIP은 압축하지 않고 담는다(`zip` 9.0.4, MIT, 압축 기능 끔. 8.x는 풀기 기능의 보안 권고(RUSTSEC-2026-0367~0369) 대상이라 고친 9.x를 쓴다. 새로 잠금에 든 `crc32fast`·`typed-path`는 MIT OR Apache-2.0, `nix`는 MIT).
- **백업(운영자)**: `fvoci-server backup create <폴더>`(서버와 같은 환경 변수, `DATABASE_MIGRATION_URL` 필요). 한 번의 일관된 읽기 스냅숏으로 모든 표를 PostgreSQL `COPY` 형식으로 쓰고, 그 스냅숏의 파일 버전 내용을 저장소에서 복사하며 SHA-256을 확인한 뒤 `manifest.json`(형식 판·마이그레이션 버전·표별 행 수·파일별 크기와 SHA-256)을 남긴다. 백업하는 동안 백그라운드 작업은 저장소 정리를 쉰다(복사 중인 내용이 지워지지 않게). 세션·로그인 상태·대기열·진행 중 업로드는 넣지 않는다. 폴더는 비어 있어야 한다. 백업 폴더와 파일은 소유자만 읽고 쓴다(0700/0600). 표 덤프마다 SHA-256을 목록에 남긴다. 실패하면 읽기 트랜잭션을 닫고 원래 오류를 보인다. 앱 이미지의 Debian 기본 `pg_dump`(15)는 PostgreSQL 18을 덤프할 수 없어, 별도 도구 없이 서버가 직접 한다.
- **복구(운영자)**: 새로 설치한 빈 DB에 같은 릴리스로 `db bootstrap`·`db migrate`를 한 뒤 `fvoci-server backup restore <폴더>`(`DATABASE_ADMIN_URL` 필요). 마이그레이션 버전이 다르거나 표에 행이 있으면 거부한다. 서버를 멈추고 복구할 것을 권한다(복구 트랜잭션은 표를 잠그고 비어 있는지 다시 본다). 표 덤프의 SHA-256을 먼저 확인해 손상된 백업은 아무것도 건드리기 전에 거부한다. 파일을 올리다 실패하면 저장소의 여러 조각 업로드를 그만둔다. 기록된 모든 파일 버전의 내용이 백업에 있는지 확인하고, 검색 색인 다시 만들기도 같은 트랜잭션에서 올린다. 먼저 파일 내용을 저장소에 올리며 백업의 SHA-256과 저장 뒤 내용을 확인하고, 그다음 한 트랜잭션에서 표를 채운다(관리자 연결, 외래 키·트리거를 그 트랜잭션에서만 끔). 끝으로 검색 색인을 다시 만든다. 세션은 복구하지 않으므로 모두 다시 로그인한다(같은 발급자·subject면 같은 계정).
- **검증**: 통합 테스트(반출: 관리자만·밖의 사람 404, 문서 본문·파일 내용·감사 기록 포함, 멤버 개인 노트 제외, 개인 반출은 내 것만; 백업·복구: 문서 본문·번호·첨부 내용이 새 DB·새 저장소에서 그대로, 같은 계정으로 다시 로그인, 옛 세션 무효, 같은 폴더 재백업·비어 있지 않은 DB 복구 거부).

## 33. 감사 기록 보기 (2026-10-17, D-57)

- 워크스페이스 관리자는 워크스페이스 메뉴의 "감사 기록"에서, 누구나 사이드바의 "내 활동 기록"에서 최신 먼저 100건씩 본다(`GET /api/workspaces/{id}/audit`, `GET /api/me/audit`, `before`로 이어 보기). 범위는 행 보안 정책 그대로: 워크스페이스 기록은 관리자, 개인 영역 기록(개인 노트·개인 파일·반출·영구 삭제한 내 워크스페이스의 옮겨진 기록)은 본인. 동작·대상 이름은 화면에서 한국어로 보이고 모르는 코드는 그대로 보인다. 기록은 각 기능의 정의자 함수만 쓴다(앱 역할은 직접 쓰지 못함).
- **검증**: 통합 테스트(관리자만 403/404, 초대·참여·삭제 기록과 행위자 이름, 최신 먼저와 쪽 넘기기, 개인 노트 기록은 본인의 개인 기록에만). 웹 테스트(경로, 표시 이름).

## 34. 강의 모듈: 강좌·회차·개인별 진도 (2026-10-17, D-37·D-32·D-39, 마이그레이션 0031)

- **강좌 정보**: 과목·강사·플랫폼·링크·교재·수강 시작일·수강권 만료일·목표 완료일. 강의 모듈의 항목(강좌)을 쓸 수 있는 사람이 고친다. 만료 14일 전부터 화면에 안내한다.
- **회차(목차)**: 번호·제목·챕터·영상 길이·링크·자료·공통 계획일. 강좌를 볼 수 있는 사람이 함께 보고 쓸 수 있는 사람이 고친다. 직접 추가와 CSV 일괄 등록(제목·챕터·길이(분, 분:초, 시:분:초)·계획일, 따옴표·머리글 지원, 줄 번호로 오류 안내)이 같은 API를 쓰고 같은 요청 id는 한 번만 들어간다. 회차를 더하면 진행률 변화(예: 50% → 40%)를 알린다.
- **개인별 진도**: 미수강·수강 중·완료(직접 표시), 내 계획일(공통 계획일을 덮어씀, 밀린 회차 다시 배정), 마지막 시청 위치, 개인 메모(나만 봄), 본 횟수. 다시 보아 완료해도 완료 회차 수는 한 번만 센다(완료한 날은 사용자의 오늘, 완료를 풀면 지움, 39절). 화면은 회차 수·영상 길이 기준 진행률, 완료·수강 중·남은 회차, 오늘 예정·완료, 밀린 회차, 남은 분량과 목표일까지 하루 필요 회차·길이를 보인다. 읽기만 하는 사람도 자기 진도는 남긴다.
- **함께 수강**: 진도를 남긴 사람 중 지금 그 강좌를 볼 수 있는 사람마다 완료·수강 중·밀린 회차 수와 완료한 영상 길이만 보인다(개인 메모·위치·시간 기록은 보이지 않음, 진도 행은 본인만 RLS로 읽음).
- **시간 기록(D-32)**: 회차를 시간 기록 대상으로 고를 수 있고, 회차의 "측정" 단추가 강의 종류로 스톱워치를 시작한다(실제 수강 시간과 순공부시간은 시간 종류로 구분).
- **반출·백업**: 워크스페이스 반출에 강좌 정보·회차, 개인 반출에 내 진도를 넣는다. 항목을 영구 삭제하면 강좌 정보·회차·진도도 지워진다.
- **아직 없음**: 외부 플랫폼 자동 시청 추적(H-02 보류). 만료·목표일 알림은 44절에서 알림함으로 보낸다.
- **검증**: 통합 테스트(강좌 정보 쓰기 권한, 밖의 사람 404, 일괄 추가·같은 요청 id·빈 제목·읽기 전용 거부, 진도는 사람마다·다시 보기·메모 비공개, 함께 수강 집계, 회차를 시간 기록 대상으로, 회차 고치기·지우기, 강의가 아닌 항목은 404). 웹 테스트(진행률·밀린 회차·하루 필요 분량, CSV 읽기). 브라우저(`cargo xtask dev --oidc` + Vite): CSV로 3회차 등록, 완료 표시 후 회차 33%·영상 13%, 오늘 예정·완료, 함께 수강 집계.

## 35. 셀프호스트 설치 재현 (2026-10-17, O-25 해소)

- **구성(O-25 권장안 채택)**: `deploy/compose.selfhost.yaml`(PostgreSQL·Meilisearch·한 번 도는 `db bootstrap`·`db migrate`·앱, 비밀 값은 운영자 `.env`에서만, 빠지면 Compose가 이유를 말하며 멈춤), 선택 프로필 `keycloak`(운영 모드, 같은 PostgreSQL의 전용 DB를 첫 설치 때 만듦, 렐름 틀 `deploy/keycloak/realm-selfhost.json`은 사용자 없이 `.env` 값으로 채움). 운영자 안내는 `docs/self-hosting.md`. 이미지는 앱 사용자 소유의 `/data/files`·`/backups`와 `fvoci-server healthcheck`(이미지에 curl 없음)를 갖는다.
- **재현(커밋 `36331a0`의 새 worktree, 임의 생성 테스트 비밀 값, Keycloak 프로필)**: 이미지 빌드와 기동, bootstrap·migrate 완료, 모든 서비스 healthy, `/healthz` ok, 렐름 틀의 되돌아올 주소·출처·클라이언트 비밀이 `.env` 값으로 채워짐, Keycloak 관리 CLI로 사용자 생성, 브라우저에서 OIDC 로그인 → 워크스페이스 만들기 → 위키 문서 작성·저장(리비전 2) → 새로 고침 → 스택 재시작 뒤 세션·내용 유지 → 첨부 올리기 → `backup create`(36개 표, 파일 1개, 폴더·파일 0700/0600) → 앱 멈춤 → **다른 새 설치**(새 DB·검색 비밀 값)에 `backup restore` → 같은 사용자로 다시 로그인(옛 세션 무효) → 문서(번호·리비전·본문)·첨부 내용·검색(문서·파일 이름) 그대로. 확인 뒤 두 설치 모두 `down -v`, 이미지·worktree 삭제.
- **재현에서 찾아 고친 것**: `docker cp`로 넣은 백업은 호스트 사용자 소유라 앱 사용자가 읽지 못한다 → 복구 절차에 소유자 넘기기(`chown`)를 넣었다.
- **로컬 재현만의 장치**: 브라우저와 앱 컨테이너가 같은 주소로 Keycloak에 닿도록 `auth.localhost`와 앱 컨테이너의 `extra_hosts`(host-gateway)를 쓴 덮어쓰기 파일(커밋하지 않음). 실제 설치는 공개 도메인이므로 필요 없다.
- **아직 없음**: HTTPS 역방향 프록시 설정 예시는 운영자가 고른 프록시에 맡긴다. 실시간 협업은 보류 중.

## 36. 댓글과 알림 (2026-10-17, D-27·D-20·D-39, 마이그레이션 0032)

- **댓글**: 문서와 업무에 다는 한 단계 스레드. 대상을 볼 수 있으면 읽고 쓴다(독자·공유받은 사람 포함, D-20). 고치기는 쓴 사람만(고친 표시), 지우기는 쓴 사람이나 대상의 소유 권한(관리자 등, 남의 댓글을 지우면 감사 기록). 답글이 남은 첫 댓글은 "지운 댓글"로 남겨 답글을 보존한다. 스레드 해결 표시는 연 사람이나 대상에 쓸 수 있는 사람, 해결된 스레드는 접어 둔다. 보관된 항목 안의 대상에는 새 댓글을 달 수 없다(409). 같은 요청 id면 같은 댓글. 화면은 문서 화면과 업무 상세의 "댓글"(Ctrl·⌘+Enter로 달기, 답글을 열면 그 칸으로 초점).
- **알림 넓히기**: 언급 외에 댓글(대상을 만든 사람·업무 담당자에게), 답글(스레드에 참여한 사람에게), 기한(업무 담당자, 없으면 만든 사람에게 하루 전과 당일, 백그라운드 작업이 매시간, 같은 날 같은 업무는 한 번). 받는 사람이 지금 대상을 볼 수 있을 때만 만들고, 읽을 때도 다시 확인해 권한이 철회되면 보이지 않는다. 알림 화면은 종류별 문장과 댓글 첫 부분을 보이고 문서·업무로 연다. 기한 날짜는 서버의 오늘(UTC) 기준이다.
- **반출·백업**: 워크스페이스 반출에 댓글을 넣는다(볼 수 있는 대상의 것만). 문서·업무를 영구 삭제하면 댓글과 그 알림도 지워진다.
- **아직 없음**: 일정 시작 알림(일정에 참석자 개념이 없어 받는 사람을 정하지 못함), 알림 메일·푸시.
- **검증**: 통합 테스트(독자 댓글·밖의 사람 404·빈 댓글 400, 댓글 알림과 행위자·첫 부분, 자기 댓글은 알리지 않음, 답글 한 단계·답글 알림, 목록 순서와 권한 표시, 고치기·해결·지우기 권한과 지운 댓글 보존, 업무 댓글의 담당자 알림, 기한 알림 하루 한 번·자동 알림 표시, 보관 항목 409, 멤버 제거 뒤 알림 사라짐). 웹 테스트(스레드 묶기). 브라우저(`cargo xtask dev --oidc` + Vite): 댓글·답글(이 확인에서 답글 칸 초점 문제를 찾아 고침)·해결 단추.

## 37. 업무 CSV 일괄 등록 (2026-10-17, D-39)

- 업무 목록·칸반 아래 "CSV로 여러 업무 등록"(쓸 수 있는 사람). 열: 제목, 설명, 상태(할 일·진행 중·완료 또는 영어), 우선순위(없음·낮음·보통·높음·긴급 또는 영어), 시작일, 기한(YYYY-MM-DD), 예상(분), 담당자(표시 이름, `;`로 여럿, 이 항목에 지정할 수 있는 사람 중 이름이 하나로 맞아야 함), 태그(`;`로 여럿). 머리글 줄은 건너뛰고, 잘못된 줄은 번호와 이유로 알려 고치기 전에는 등록하지 않는다.
- 새 서버 경로 없이 줄마다 기존 업무 만들기(같은 요청 id면 같은 업무)와 고치기 API를 쓴다. 같은 검증·권한·감사 기록을 그대로 지나고, 중간에 실패하면 다시 눌러 나머지를 이어서 넣는다(이미 만든 업무를 또 만들지 않음).
- CSV 줄 나누기는 회차 일괄 등록과 함께 쓰는 `apps/web/src/lib/csv.ts`.
- **검증**: 웹 테스트(필드·한국어 값·담당자 이름 맞추기·따옴표, 줄 번호별 오류). 서버 경로는 기존 업무 테스트가 덮는다.

## 38. 검색 필터와 첨부 본문 검색 (2026-10-17, D-39, 마이그레이션 0033)

- **필터**: `GET /api/search`에 `module_id`(그 모듈 안), `status`(`todo`·`in_progress`·`done`), `assignee`(`me` 또는 사용자 id), `from`·`to`(YYYY-MM-DD, 끝 포함)를 더한다. 상태·담당자를 주면 업무만 나온다. 날짜는 업무는 기한(없으면 시작일), 일정은 시작일, 그 밖은 고친 날로 본다. 잘못된 값은 400. Meilisearch에는 걸러 낼 속성(`module_id`·`status`·`assignee_ids`·`day`)을 두고 권한 범위 필터에 덧붙이며, 결과는 그대로 `app_search_visible`로 다시 거른다. 검색 서비스가 없을 때의 제목 검색도 같은 필터를 쓴다. 담당자가 바뀌면 그 업무를 다시 색인한다.
- **첨부 본문**: 파일의 현재 버전에서 글자를 뽑아 색인한다. 글자 형식(text/\*, JSON·XML·YAML, `.md`·`.csv`·`.log` 등), DOCX(본문·머리글·바닥글), PPTX(슬라이드), XLSX(공유 문자열·시트), HWPX(구역). 이 넷은 ZIP 안의 XML이라 메모리에서 읽고 태그를 걷어 낸다(디스크에 풀지 않음, 항목당 32 MiB까지 읽어 압축 폭탄을 막음). 20 MiB보다 큰 파일과 PDF·HWP(이진)·옛 Office 이진 형식은 아직 이름으로만 찾는다. 뽑은 글자는 색인에만 두고 DB에 저장하지 않는다.
- **권한**: 본문 일치와 요약은 그 파일을 볼 수 있는 사람에게만 보인다. 요약은 읽기에 해당하므로 다운로드 허용(D-20 복제·다운로드)과 따로 묻지 않는다.
- **새 의존성**: DOCX 등의 압축을 풀기 위해 `zip`의 `deflate-flate2` 기능과 `flate2` 1.1(순수 Rust 백엔드). 잠금에 새로 든 것은 `flate2`(MIT OR Apache-2.0), `miniz_oxide`(MIT OR Zlib OR Apache-2.0), `adler2`(0BSD OR MIT OR Apache-2.0), `simd-adler32`(MIT). `cargo deny`가 새로 실패하지 않는다.
- **검증**: 단위 테스트(DOCX·HWPX 글자 뽑기). 통합 테스트(상태·`assignee=me`·날짜 범위·모듈 필터, 잘못된 값 400, DOCX 본문 검색과 요약, 검색 서비스 없을 때의 필터). 브라우저(`cargo xtask dev --oidc` + Vite): 검색 창에서 첨부(.txt) 본문 일치와 요약, 내 담당(종류가 업무로 고정), 날짜부터, 거꾸로 된 범위 안내.

## 39. 댓글·강좌 리뷰 반영 (2026-10-17, D-27·D-37·D-39, 마이그레이션 0034)

34·36절 구현을 다시 검토해 고친 것.

- **문서를 옮긴 뒤의 댓글**: 댓글에 따로 적어 두던 워크스페이스를 없애고 대상(문서·업무)에서 읽는다. 댓글 달린 문서를 다른 워크스페이스로 옮기면 옛 값이 남아 옛 워크스페이스를 영구 삭제할 수 없었고(외래 키) 새 워크스페이스 반출에서 빠졌다.
- **알림의 댓글 첫 부분**: 알림을 만들 때 적어 두지 않고 읽을 때 지금의 댓글에서 가져온다. 고친 댓글은 고친 글이, 지운 댓글은 아무것도 보이지 않는다(전에는 옛 글이 남았다).
- **답글과 지우기가 겹칠 때**: 답글을 다는 동안 스레드 첫 댓글을 잠근다. 동시에 첫 댓글을 지우면 답글이 함께 사라지던 경합을 막는다(먼저 지워졌으면 답글은 404, 답글이 먼저면 첫 댓글은 "지운 댓글"로 남는다).
- **알림 한 번**: 업무를 만든 사람이 담당자이기도 하면 새 댓글 알림이 두 번 가던 것을 한 번으로.
- **완료한 날**: 화면이 보낸 사용자의 오늘로 적는다(서버 날짜와 하루 넘게 다르면 400). 전에는 UTC 날짜라 한국 시간 오전 9시 전 완료가 어제로 셈됐다. 완료를 풀면 완료한 날을 지우고, 화면의 "오늘 완료"는 지금 완료인 회차만 센다.
- **함께 수강**: 지금 그 강좌를 볼 수 있는 사람만 보인다(항목이 비공개로 바뀌면 볼 수 없게 된 멤버는 빠진다).
- **링크**: 강좌·회차 링크는 `http://`·`https://`만 받는다(서버 400, 화면도 그 밖의 값은 링크로 만들지 않는다). 기존의 다른 값은 마이그레이션이 비운다.
- **다시 보내기**: 회차 추가·CSV 등록과 댓글·답글은 요청 id를 입력할 때 정해 성공할 때까지 지킨다. 응답을 잃고 다시 눌러도 같은 회차·댓글이 된다(전에는 누를 때마다 새 id라 두 번 들어갈 수 있었다). 실패하면 입력을 지우지 않는다.
- **CSV**: 따옴표 안의 줄바꿈, 맨 앞의 BOM·빈 줄 뒤 머리글, 빈 칸만 있는 줄 건너뛰기, 제목 길이(회차 300자, 업무 200자)·길이 100시간 한도를 줄 번호로 알린다.
- **화면**: 이름을 볼 수 없는 댓글 작성자는 "이름을 볼 수 없는 사람"(전에는 나간 멤버도 "공유받은 사람"), 댓글 지우기는 확인을 한 번 묻고 지운 뒤 초점을 댓글 제목으로, 회차 추가 안내는 서버가 실제로 더한 개수.
- **검증**: 통합 테스트(옮긴 문서의 댓글과 옛 워크스페이스 영구 삭제·새 반출, 고친·지운 댓글의 알림 첫 부분, 만든 사람 겸 담당자 알림 한 번, 잘못된 링크 400, 사용자의 오늘과 너무 먼 날 400, 완료 풀기, 비공개로 바뀐 강좌의 집계). 웹 테스트(CSV 줄바꿈·머리글·한도, 오늘 완료는 지금 완료만). 브라우저: 댓글 지우기 확인(확인 단추에 초점, 지운 뒤 댓글 제목으로 초점), 회차 추가 뒤 입력 비움·완료 표시 후 오늘 완료 1, `javascript:` 강좌 링크 거부.

## 40. 항목 정보와 항목 멤버 (2026-10-17, D-33·D-34·D-36·D-38·D-20, 마이그레이션 0035)

- **항목 정보**: 상태(계획·진행·보류·완료·취소), 기간(시작일·종료일), 리더, 목표, 범위와 모듈 종류별 정보. 연구는 분야·키워드·연구 질문·가설·모형·방법·표본(리더는 "책임자", 멤버는 "공동 연구자"), 스터디는 수준·목표 분량(멤버는 "참여자"), 취미는 활동 유형(멤버는 "참여자"). 종류에 없는 칸은 400, 모르는 칸은 422로 서버가 거른다. 쓸 수 있는 사람이 고치고, 상태·리더 변경은 감사 기록에 남는다. 모듈 목록에 상태·기간이 보인다.
- **구현 선택(D-47)**: D-33 "프로젝트별 상태"는 항목 상태로 둔다. 업무 상태는 D-31의 세 가지(할 일·진행 중·완료)를 그대로 쓴다(검색 필터·CSV·기한 알림이 같은 값을 쓴다). 종류별 정보는 항목의 `details`(JSON)에 두고 형식은 서버가 확인한다. D-33의 요구사항·설계·회의록·결정·매뉴얼·인계 문서는 별도 유형을 두지 않고 프로젝트 항목의 문서(트리)와 문서 템플릿(47절)으로 쓴다.
- **리더**: 표시만 하고 권한을 더 주지 않는다. 그 항목을 볼 수 있는 사람만 고를 수 있고, 워크스페이스를 떠났으면 보이지 않으며, 멤버에서 빠져 볼 수 없게 되면 비운다.
- **항목 멤버(사람·그룹)**: 비공개 항목을 함께 쓰는 사람. 문서의 명시적 공유(D-20)와 같은 뜻으로 `app_item_access` 하나에 넣었다. 멤버는 워크스페이스 역할 안에서 본다(작성자 → 쓰기, 독자 → 읽기). 정하는 것은 항목 소유 권한자(만든 사람·관리자)뿐이다(재공유는 소유자만). 사람은 그 워크스페이스 멤버, 그룹은 그 워크스페이스 그룹만. 빠지거나 그룹에서 나가거나 워크스페이스를 떠나면 다음 요청부터 보지 못하고, 그 항목 업무의 담당에서도 빠진다. 항목 아래 문서·업무·일정·첨부·댓글·검색·알림·함께 수강 집계가 모두 같은 함수를 따른다. 공개 항목에서는 멤버가 권한을 바꾸지 않는다.
- **모듈 아이콘**: 관리자가 모듈 설정에서 그림 문자 등(16자까지)을 정한다. 모듈 제목 앞에 보인다.
- **반출·백업**: 워크스페이스 반출에 항목 정보(항목 행)와 항목 멤버를 넣는다. 항목을 영구 삭제하면 멤버 행도 지워진다.
- **검증**: 통합 테스트 2건(비공개 항목은 멤버만·역할 안에서·멤버는 소유자만 정함·워크스페이스 밖 사람 400·그룹 멤버·빠지면 404와 담당·리더 해제·그룹에서 나가면 404, 종류별 칸·모르는 칸 422·상태·기간 400·목록의 상태·모듈 아이콘 관리자만)와 기존 권한 격리 테스트 전체. 단위 테스트(종류별 칸 정리). 웹 테스트(보낼 칸 고르기, 사람 이름). 브라우저: 정보 고치기(상태·기간·리더·목표) 저장과 표시, 멤버 고르기 열기.

## 41. 항목 기록과 분석 당시 파일 버전 고정 (2026-10-17, D-33·D-34·D-35·D-36·D-38, 마이그레이션 0036)

- **기록 종류**(결정 문서의 낱말 그대로, X-08): 프로젝트는 산출물(상태·담당·완료 기준·마감일)·위험(가능성·영향·상태·대응)·문제(심각도·상태·해결)·회고. 연구는 문헌(저자·연도·학술지·DOI·URL·읽기 상태·태그·요약·인용·페이지·Zotero 키, PDF·원문 연결)·연구 기록(실험·인터뷰·랩미팅·피드백·결정·설계 변경, 참여자)·데이터(원자료·정제·분석 자료 구분, 출처·외부 위치·수집일·자료 버전·결측·제외·품질·권한 관리)·변수(변수 사전·코드북)·분석(목적·담당·상태·설정·로그·오류·컴퓨터 실행 시간·결과, 입력 데이터·코드·결과 파일 연결)·투고(대상·상태 준비~게재·거절, 제출 예정일·수정 마감일·공동저자·체크리스트·IRB, 제출본·심사 의견·답변서 연결). 스터디는 자료(책·논문·문제집·웹·강좌, 강의 모듈 강좌 연결, 분량·교육과정)·모임(진행자·준비 과제·출석)·시험(응시자·점수·만점·피드백)·복습(주제·취약 영역·반복 횟수·다음 복습일). 취미는 재료·장비(보유·필요·준비 중·준비됨, 장비 설정)·장비 사용·활동(시간·참여자·결과·배운 점, 사진·영상)·결과물(회고·다음 목표). 강의 모듈은 34절의 강좌·회차를 쓴다.
- **한 정의**: 종류·칸·고를 값·파일 역할은 `features/records.rs` 한곳에 있다. 서버가 그대로 확인하고(종류에 없는 칸·잘못된 값·http·https가 아닌 주소·워크스페이스 밖 사람·볼 수 없거나 강좌가 아닌 항목 400), 화면은 `GET /api/record-kinds`로 받아 입력 칸을 그린다.
- **권한**: 항목을 그대로 따른다(볼 수 있으면 읽고, 쓸 수 있으면 만들고 고친다, 보관된 항목은 409). 지우기는 만든 사람이나 항목 소유 권한자. 고치기는 읽은 판이 아니면 409. 같은 요청 id면 같은 기록. 만들기·지우기는 감사 기록에 남는다.
- **분석 당시 버전 고정(D-35)**: 기록에는 그 항목 첨부의 특정 버전을 연결한다(버전을 고르지 않으면 지금 버전). 새 버전이 올라와도 연결은 당시 버전을 가리키고, 연결된 버전은 하나만 따로 지울 수 없다(409, 연결을 바꾸면 지울 수 있다). 파일 자체를 영구 삭제하면(휴지통 비우기·만료, 항목·워크스페이스 영구 삭제) 연결은 파일 이름·버전·SHA-256·크기로 남고 "삭제됨"으로 보인다.
- **사람의 시간과 컴퓨터 실행 시간**: 분석의 "컴퓨터 실행 시간(분)"은 기록 칸이고, 사람의 작업 시간은 시간 기록(24절)에 따로 있다(D-34).
- **반출·백업**: 워크스페이스 반출에 기록과 연결(이름·버전·SHA-256)을 넣는다. 항목을 영구 삭제하면 기록도 지워진다.
- **아직 없음**: Zotero 정기 동기화(키를 저장해야 해서 보관 방식 결정이 필요하다. 가져오기는 51절). 복습일·마감 알림은 44절. 기록 검색은 45절.
- **검증**: 통합 테스트 2건(분석의 v1 연결·같은 요청 id·새 버전 뒤에도 v1·연결된 v1 지우기 409·독자 403·밖의 사람 404·다른 모듈 종류·잘못된 고를 값·주소·밖의 사람·다른 항목 파일·파일 없는 종류·빈 제목 400·판이 다르면 409·연결을 v2로 바꾼 뒤 v1 지우기·남이 지우기 403·파일 영구 삭제 뒤 연결 정보 유지, 스터디 자료의 강좌 연결과 강좌가 아닌 항목 400·시험 점수·종류 정의 공개). 단위 테스트(종류 이름 중복 없음, 칸 확인). 웹 테스트(입력 칸 ↔ 보낼 값, 요약). 브라우저: 연구 항목에서 분석 기록에 첨부 survey.csv v1 연결·담당·상태 저장과 표시.

## 42. 스터디 계획과 개인별 진행 (2026-10-17, D-36, 마이그레이션 0037)

- **계획 단위**: 스터디 항목에 번호·제목·교육과정·챕터(단계)·분량(쪽·문제·분·개)·공통 계획일을 둔다. 스터디를 볼 수 있는 사람이 함께 보고 쓸 수 있는 사람이 더하고 고치고 지운다(같은 요청 id는 한 번만, 지우면 모든 사람의 그 진행도 지워짐, 감사 기록).
- **개인별 진행**: 사람마다 시작 전·진행 중·완료, 한 분량, 내 계획일(공통 계획일을 덮어씀), 메모(나만 봄). 완료한 날은 사용자의 오늘이고 완료를 풀면 지운다(39절과 같은 규칙). 진행 행은 본인만 읽는다(RLS). 독자도 자기 진행을 남긴다.
- **계획 대비 진행(화면)**: 단위 수 진행률, 완료·진행 중·남은 단위, 분량 종류별 목표·한 분량·남은 분량, 오늘 예정·완료, 밀린 단위(내 날짜 기준).
- **미완료 재배치**: 밀린 단위를 사용자가 고른 시작일부터 하루 N개씩 내 계획일로 다시 배정한다(진행 상태는 그대로). 날짜를 자동으로 추정하지 않는다(X-07).
- **함께 하는 사람**: 진행을 남긴 사람 중 지금 그 스터디를 볼 수 있는 사람의 완료·진행 중·밀린 단위 수와 한 분량만 보인다(메모·개인 날짜는 보이지 않음).
- **개인 및 공동 계획(D-36)**: 공통 계획일은 모두의 계획, 내 계획일은 개인 계획이다. 자료·모임·시험·복습은 41절 기록, 강좌 연결은 41절 자료 기록, 시간은 시간 기록(24절, 종류 "공부")을 쓴다.
- **반출·백업**: 워크스페이스 반출에 계획 단위, 개인 반출에 내 진행을 넣는다. 항목을 영구 삭제하면 단위와 진행도 지워진다.
- **검증**: 통합 테스트(단위 추가·같은 요청 id·잘못된 분량 종류 400·독자 추가 403·밖의 사람 404·스터디가 아닌 항목 404, 진행은 사람마다·메모 비공개·사용자의 오늘, 재배치는 내 날짜만·다른 항목의 단위 404, 함께 하는 사람 집계와 비공개 전환 뒤 빠짐, 단위 지우기). 웹 테스트(진행·분량·오늘·밀림, 재배치 날짜). 브라우저: 단위 3개에서 밀린 2개를 오늘부터 하루 1개로 재배치, 완료 표시 후 33%·30/55쪽·오늘 완료 1.

## 43. 항목 복제와 재사용 템플릿 (2026-10-17, D-33·D-20·D-48, 마이그레이션 0038)

- **복제**: 항목 화면의 "복제"로 같은 워크스페이스의 같은 종류 모듈(보관되지 않은 것)에 새 항목을 만든다. 볼 수 있는 사람이 하고, 대상 모듈에 항목을 만들 수 있어야 한다(독자 403). 같은 요청 id면 같은 항목. 새 항목은 비공개이고 복제한 사람이 만든 사람이다(D-20). 감사 기록에 원본과 건너뛴 문서 수가 남는다.
- **옮기는 것**: 항목 정보(리더 제외), 업무(하위 업무·우선순위·예상·태그·마일스톤·체크리스트·선행 관계), 항목 문서(트리 구조, 리비전 1부터), 강좌 정보·회차, 스터디 단위. 업무와 문서는 대상 모듈에서 새 번호를 받는다(D-48).
- **옮기지 않는 것**: 담당자(새 항목은 비공개라 볼 수 없음), 반복 규칙, 기록·첨부 파일, 사람마다의 진도·진행, 복제를 허용하지 않은 문서(그 문서의 소유 권한자가 아니면 건너뛰고 수를 알린다, D-20 복제 허용. 팀 문서의 소유 권한은 관리자에게 있으므로 작성자는 복제 허용이 켜진 문서만 옮긴다). 문서 본문의 이미지(47절)는 원래 항목의 첨부를 가리키므로, 원래 항목을 볼 수 없는 사람에게는 복제본에서 이미지가 보이지 않는다(필요하면 복제본에 다시 넣는다).
- **새로 시작**: 켜면(기본) 업무 상태(할 일로)·날짜·체크 표시와 항목 상태(계획)·기간, 강좌·스터디의 날짜를 비운다. 끄면 그대로 옮긴다.
- **재사용 템플릿**: 쓸 수 있는 사람이 항목을 "템플릿으로 표시"한다(목록·항목 화면에 "템플릿"). 모듈 화면의 "템플릿에서 만들기"는 그 모듈의 템플릿을 새로 시작으로 복제해 새 항목을 연다. 회고(41절 기록)는 항목의 기록이며 템플릿에는 옮겨지지 않는다.
- **검증**: 통합 테스트(다른 종류 모듈 400·독자 403, 복제 결과 새 번호·비공개·원래 멤버도 404·업무 상태·날짜 비움·우선순위·예상 유지·담당 없음·하위 업무·선행 관계·체크리스트 해제·남의 문서 건너뜀 1·같은 요청 id, 새로 시작 끔은 상태·날짜 유지, 템플릿 표시 권한과 목록 표시). 브라우저: 항목을 템플릿으로 표시한 뒤 모듈 화면에서 템플릿으로 새 항목을 만들어 업무가 새 번호(DEV-5·6)로 옮겨지고 비공개로 열림.

## 44. 항목 알림: 강좌 만료·복습일·마감 (2026-10-17, D-37·D-36·D-34·D-33·D-39, 마이그레이션 0039)

- **대상 넓히기**: 알림 대상에 항목을 더했다(문서·업무·항목 중 정확히 하나). 받는 사람이 지금 그 항목을 볼 수 있을 때만 만들고, 읽을 때도 다시 확인한다(36절과 같은 규칙). 항목 알림을 열면 그 항목으로 간다.
- **강좌(D-37 만료·마감 알림)**: 수강권 만료일 7일 전·당일, 목표 완료일 하루 전·당일. 받는 사람은 그 강좌에 진도를 남긴 사람, 아무도 없으면 강좌를 만든 사람.
- **스터디 복습(D-36)**: 복습 기록(41절)의 다음 복습일 하루 전·당일, 기록을 만든 사람에게.
- **마감(D-34·D-33)**: 연구 투고의 제출 예정일·수정 마감일, 프로젝트 산출물의 마감일 하루 전·당일. 투고는 기록을 만든 사람과 공동저자, 산출물은 만든 사람과 담당에게. 게재·거절·완료·승인 상태면 보내지 않는다.
- **작업**: 백그라운드 작업이 업무 기한 알림과 함께 매시간 돌고, 같은 사람·대상·날짜는 한 번만 만든다. 날짜는 서버의 오늘(UTC) 기준이다(36절 기한 알림과 같음).
- **아직 없음**: 일정 시작 알림(36절), 알림 메일·브라우저 푸시(H-03).
- **검증**: 통합 테스트(만료 7일 전 알림은 진도를 남긴 사람에게 한 번·만든 사람에게는 없음·대상 이름과 날짜, 투고 마감은 만든 사람·공동저자에게 한 번·끝난 투고는 없음, 하루 두 번 돌아도 한 번, 항목을 비공개로 바꾸면 공동저자에게 보이지 않음). 화면 문장은 웹 검사(형식·타입)로만 확인했고 브라우저로는 보지 않았다(작업이 한 시간에 한 번 돌아 개발 서버에서 바로 만들 수 없음).

## 45. 기록 검색 (2026-10-17, D-39, 마이그레이션 0040)

- 통합 검색에 항목 기록(41절)을 더했다(종류 `record`, 화면 이름 "기록"). 제목·메모·칸의 글자를 색인하고, 결과는 그 항목을 지금 볼 수 있는 사람에게만 보인다(`app_search_visible`이 항목 권한으로 다시 거름). 결과를 고르면 그 항목으로 간다.
- 필터: 종류·모듈·날짜(기록 날짜, 없으면 고친 날)를 쓴다. 상태·담당자 필터는 업무 전용이라 기록에는 걸리지 않는다. 검색 서비스가 없을 때의 제목 검색도 기록 제목을 찾는다.
- 항목이 휴지통에 들어가거나 되살아나면 그 기록도 다시 색인한다. 마이그레이션이 기존 것을 모두 다시 올린다.
- **검증**: 통합 테스트(제목·칸 글자로 찾기·종류 `record`·항목 id, 비공개 항목의 기록은 멤버에게 보이지 않음, 검색 서비스 없이 `kinds=record` 제목 검색).

## 46. 업무 검색·필터·정렬, 항목 달력, 저장한 보기 (2026-10-17, D-31, 마이그레이션 0041)

- **검색·필터·정렬**: 항목의 업무 화면에서 제목·설명·번호의 글자, 상태, 담당(전체·내 담당·담당 없음·사람), 우선순위, 태그, 마감일 범위로 거르고, 기본 순서·마감일·우선순위·제목·최근 고침으로 정렬한다. 칸반·목록·간트·달력이 모두 거른 결과를 보인다. 기본 순서가 아니면 칸반의 위·아래 옮기기는 끈다(열 옮기기는 된다). 거르는 중이면 "보이는 수/전체"를 알린다.
- **항목 달력**: 업무를 마감일(없으면 시작일) 칸에 놓는 달 보기. 날짜 없는 업무 수를 알린다. 워크스페이스 달력(22절)은 일정과 업무를 함께 보인다.
- **저장한 보기**: 보기 방식·필터·정렬을 이름을 붙여 저장하고 고르면 그대로 되살린다. 사람마다 따로이며 본인만 읽는다(같은 이름이면 덮어씀). 그 항목을 볼 수 없게 되면 보기도 보이지 않는다. 개인 반출에 들어간다.
- **구현 선택(D-47)**: 거르기·정렬은 이미 받은 업무 목록에서 화면이 한다(항목의 업무는 한 화면 분량). 보기 내용은 화면이 해석하는 JSON이고 서버는 크기(4000자)·형식만 확인한다.
- **검증**: 통합 테스트(보기 저장·같은 이름 덮어쓰기·남에게 안 보임·남이 지우기 404·밖의 사람 404·빈 이름 400·항목을 볼 수 없으면 404). 웹 테스트(글자·상태·담당·우선순위·태그·마감 범위 거르기, 정렬, 저장된 보기 읽기, 날짜별 배치). 브라우저: 내 담당으로 거르기(1/2개), 달력 보기, "내 마감 순" 저장 후 새로 고침해 되살리기.

## 47. 편집기의 표·수식·이미지·첨부와 문서 템플릿 (2026-10-17, D-27·D-20, 마이그레이션 0042)

- **표**: 서식 도구의 "표"가 머리글 줄이 있는 3×3 표를 넣는다. 표 안에서는 행·열 추가와 지우기, 표 지우기가 보인다(Tiptap 표 확장).
- **수식**: "수식"이 LaTeX 입력 칸을 연다. 기본은 글 안 수식이고 "따로 한 줄에"를 고르면 블록 수식이다. 넣은 수식을 누르면 같은 칸에서 고치고, 비우고 바꾸면 지운다(KaTeX로 그림).
- **이미지·첨부**: "파일·이미지"가 고른 파일을 이 문서의 첨부로 올린 뒤(31절과 같은 업로드), 그림 형식(PNG·JPEG·GIF·WebP)이면 그 버전의 미리보기 주소로 이미지를, 그 밖이면 파일 이름 링크를 본문에 넣는다. 이미지는 첨부를 볼 수 있는 사람에게 보인다(공개 링크로 보는 사람에게는 보이지 않는다).
- **본문 안전 규칙(서버)**: 이미지 노드의 주소는 이 서비스의 첨부 내용 주소(`/api/files/{id}/content`, `version`·`inline`만)만 받는다. 바깥 주소는 읽는 사람의 접속 정보를 남에게 보내므로(추적 이미지) 400. 링크는 스킴이 있으면 http·https·mailto만 받고(`javascript:`·`data:` 거부), 스킴이 없는 상대 경로·앵커는 받되 다른 호스트로 가는 `//`는 막는다.
- **문서 템플릿**: 문서의 소유 권한자(팀 문서는 관리자, 개인 노트는 본인)가 "템플릿으로 쓰기"를 켠다(복제 허용과 같은 소유자 결정, D-20). 위키·항목 문서·개인 노트를 만들 때 "템플릿"에서 고르면 그 본문으로 시작한다. 목록은 그 워크스페이스에서 내가 읽을 수 있는 템플릿과 내 개인 노트 템플릿이고, 읽을 수 없게 된 템플릿은 404. 표시·해제는 감사 기록에 남는다.
- **새 의존성**: `@tiptap/extension-table`·`@tiptap/extension-image`·`@tiptap/extension-mathematics` 3.32.0(MIT, 기존 Tiptap과 같은 판), `katex` 0.18.10(MIT, 수식 확장이 받는 범위의 최신 정상판. 0.18.11은 배포자가 철회 표시, 0.19는 범위 밖). KaTeX 글꼴은 OFL-1.1이라 승인 전에는 넣지 않고 시스템 글꼴로 그린다(9절).
- **아직 없음**: 이미지 크기 조절·정렬, 표 칸 합치기 단추(키보드 명령은 Tiptap 기본). 참조 확인(49절)은 문서 리비전 본문을 글자로 찾으므로 리비전이 아주 많아지면 느려질 수 있다(셀프호스트 규모에서는 문제없음).
- **검증**: 통합 테스트(템플릿 표시 권한·목록·밖의 사람 404, 템플릿 본문으로 만들기, 템플릿이 아니거나 볼 수 없으면 404, 개인 노트 템플릿은 본인 목록에만, 바깥 이미지·`javascript:` 링크 400, 첨부 이미지·표 저장). 단위 테스트(이미지·링크 주소 규칙). 브라우저: 표 넣기와 표 도구, 표 안의 수식, PNG 넣기(첨부로 올라가고 미리보기 주소로 보임) 후 저장, 템플릿으로 표시하고 위키에서 그 템플릿으로 새 문서를 만들어 표·수식·이미지가 그대로 옴.

## 48. 공부 시간 연결, 내 시간대, 기록 비교 (2026-10-17, D-32·D-50·D-34, 마이그레이션 0043)

- **스터디 단위 시간 기록(D-32 "공부에 연결")**: 스터디 단위의 "측정" 단추가 종류 "공부"로 스톱워치를 시작하고 그 단위를 대상으로 단다(강의 회차와 같은 방식). 대상 이름은 `STU-1 2. 연습문제`처럼 보이고, 그 스터디를 볼 수 없는 사람은 대상으로 고를 수 없다(404).
- **내 시간대(D-50)**: 사용자 메뉴의 "내 시간대"에서 IANA 시간대를 고르거나 브라우저 시간대를 쓴다. 홈·개인 시간 집계·달력·강좌·스터디의 "오늘"과 날짜가 이 시간대를 따른다(정확한 시각은 그대로 UTC로 저장). 설정은 본인만 읽는 별도 표에 두고 다른 사람에게 보이지 않는다. 모르는 이름은 400.
- **기록 비교(D-34)**: 분석 기록에 "비교한 이전 실행"·"재실행 결과 비교", 문헌 기록에 "연구 방법"·"주요 결과"·"한계" 칸을 더했다. 같은 종류 기록이 둘 이상이면 "표로 비교하기"로 칸을 열로 나란히 본다(선행연구 비교표, 재실행 비교 등). 파일 내부 차이 분석은 하지 않는다(X-03).
- **검증**: 통합 테스트(스터디 단위로 타이머 시작과 대상 이름·밖의 사람 404, 시간대 저장·다른 사람에게 안 보임·모르는 이름 400·비우기). 단위 테스트(작업 id가 겹치지 않음). 브라우저: 스터디 단위 "측정"으로 타이머(대상 `STU-1 2. 연습문제 1`), 내 시간대를 America/Los_Angeles로 바꾸면 스터디의 오늘이 그 날짜로 바뀜.

## 49. 초기 관리자, 이전 버전 보존, 복원 규칙, 관리자 열람 안내 (2026-10-17, D-51·D-56·D-58·D-29·D-21, 마이그레이션 0044)

- **초기 관리자(D-51, X-10)**: 운영자가 `FVOCI_INITIAL_ADMIN_SUBJECT`에 관리자로 정한 사람의 OIDC subject(설정한 발급자의 것)를 넣으면, 그 사람이 처음 로그인할 때 `FVOCI_INITIAL_WORKSPACE`(기본 `fvoci`) 이름의 첫 워크스페이스를 만들고 소유자(관리자)로 둔다. 서버 인스턴스에서 한 번만 하며(`instance_state`), 다른 사람이 먼저 로그인해도 관리자가 되지 않는다. 비우면 지금처럼 각자 만들거나 초대를 받는다. 셀프호스트 `.env`·Compose에 칸을 더했다.
- **이전 버전 보존(D-56)**: 관리자가 휴지통 화면에서 "이전 파일 버전 보존(일)"을 정한다(1~3650, 비우면 영구 보존이 기본). 작업 프로세스가 휴지통 만료와 함께 매시간 기간이 지난 이전 버전을 지우되, 기록이 연결한 버전(41절), 문서 리비전 본문이 가리키는 버전(47절의 이미지 등), 지금 버전은 지우지 않는다. 그런 버전은 사람이 하나씩 지우려 해도 409. 워크스페이스 저장 한도(31절)를 넘으면 올리기를 거부하고 알린다(참조 버전을 몰래 지우지 않음).
- **복원 규칙(D-58, D-29)**: 문서를 옛 리비전으로 되돌리면 그 문서의 본문(본문 체크리스트 포함)만 돌아간다. 업무의 완료 상태와 강의 진도는 별도 표에 있어 바뀌지 않는다. 테스트로 고정했다.
- **관리자 열람 안내(D-21)**: 위키 화면 위에 "관리자는 비공개 문서를 포함해 이 공간의 모든 문서를 볼 수 있고, 개인 노트·개인 시간 기록은 관리자에게도 보이지 않는다"를 보인다. 문서 비공개 설정·항목 공개 설정·개인 노트·시간 화면의 안내와 같은 내용이다.
- **검증**: 통합 테스트(초기 관리자: 다른 사람 먼저 로그인해도 없음·지정된 사람에게 첫 워크스페이스와 관리자 역할·두 번째는 없음, 이전 버전: 보존 기간 관리자만·범위 400·열흘 지난 버전 중 기록 연결 v1·리비전 이미지 v3·지금 v4는 남고 v2만 지워짐·리비전이 가리키는 버전 직접 삭제 409, 복원: 본문 체크리스트만 돌아가고 업무 완료·강의 진도는 그대로). 웹 검사(형식·타입·린트·테스트).

## 50. 출시 후보 검증: 새 checkout (2026-10-17, 커밋 `0375362`)

커밋 `0375362`를 새로 `git clone`한 디렉터리에서 자체 `target/`으로 처음부터 빌드해 확인했다.

| 단계                                                        | 결과    | 내용                                                                                                                                                                                                                                                             |
| ----------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm install --frozen-lockfile`                            | PASS    |                                                                                                                                                                                                                                                                  |
| `cargo xtask check`                                         | FAIL    | fmt·clippy·단위 테스트·계약 최신·형식·린트·타입·웹 테스트는 PASS. `cargo deny`(라이선스, 9절의 크레이트 20개 그대로)와 `pnpm licenses`(9절의 npm 9개 그대로)만 FAIL — O-30 승인 대기, 예외를 넣지 않음                                                           |
| `pnpm --filter @fvoci/web build`                            | PASS    | 운영 빌드(KaTeX 글꼴 포함)                                                                                                                                                                                                                                       |
| `cargo xtask test --oidc`                                   | PASS    | 168건 통과·실패 0. OIDC 4건(발급자+subject 식별·로그아웃, 위조 state 거부, 다른 사용자 문서 거부, 세션 만료)과 S3 호환(SeaweedFS) 3건 포함                                                                                                                       |
| 셀프호스트 설치(`compose.selfhost.yaml --profile keycloak`) | PASS    | 이미지 빌드·bootstrap·migrate·모든 서비스 healthy·`/healthz` ok                                                                                                                                                                                                  |
| 초기 관리자(D-51)                                           | PASS    | Keycloak 사용자 둘. 지정하지 않은 사람이 먼저 로그인해도 워크스페이스 없음. `FVOCI_INITIAL_ADMIN_SUBJECT`로 지정한 사람이 로그인하자 `우리 팀`의 관리자                                                                                                          |
| 편집기 내용·재시작                                          | PASS    | 표·수식·첨부 이미지가 든 위키 문서와 파일 버전을 고정한 분석 기록을 만든 뒤 전체 재시작, 운영 이미지에서 표·KaTeX·이미지 표시                                                                                                                                    |
| 백업·복구                                                   | PASS    | `backup create`(표 45개·파일 2개·마이그레이션 44) → 새로 설치한 두 번째 인스턴스에 `backup restore` → 다시 로그인해 관리자 역할·문서 리비전·이미지(200 image/png)·기록의 고정 버전 v1·검색 다시 만들기(본문 낱말로 찾음) 확인. 0033~0044가 복구 경로를 처음 지남 |
| R2 실검증                                                   | NOT RUN | 승인된 테스트 대상과 자격 증명이 없다(자체 S3 서버를 만들지 않음)                                                                                                                                                                                                |
| 원격 CI·CI의 OIDC 테스트(D-63·D-64)                         | NOT RUN | 원격 push 승인 전                                                                                                                                                                                                                                                |

검증 뒤 두 설치와 볼륨은 이 프로젝트 이름으로만 내렸다(전역 정리 없음). 검증 뒤 바뀐 것: KaTeX 글꼴(OFL-1.1, 9절)을 번들에서 뺀 웹 변경 하나. 형식·타입·린트·웹 테스트·운영 빌드(글꼴 파일 0개)와 시스템 글꼴 수식 표시를 다시 확인했다. 서버·마이그레이션·배포 파일은 바뀌지 않았다.

## 51. Zotero 읽기 연동 (2026-10-17, D-34·X-05)

- **무엇을**: 연구 항목의 "문헌" 기록에서 "Zotero에서 가져오기". 내 라이브러리 또는 그룹(숫자 ID)과 Zotero 설정의 API 키를 넣고, "컬렉션 읽기"로 컬렉션을 고르거나 전체(최상위 문헌)를 가져온다. 서지(제목·저자·연도·학술지·DOI·URL·페이지·초록→요약·태그)와 컬렉션 이름, Zotero 원본 링크, Zotero 키를 문헌 기록 칸에 넣는다. 첨부·메모는 건너뛴다. PDF·주석 가져오기와 Zotero에 쓰기는 하지 않는다(X-05).
- **다시 가져오기**: 같은 Zotero 키의 문헌은 새로 만들지 않고 서지 칸만 바꾼다. 읽기 상태·인용·연구 방법·주요 결과·한계·메모 같은 내 칸은 그대로 두고, 요약은 비어 있을 때만 채우며 태그는 합친다. 같은 항목에 가져오기가 겹치면 차례로 한다.
- **키 다루기(구현 선택, D-47)**: 키는 가져오는 요청 동안만 서버가 Zotero Web API(v3) 머리글로 쓰고 저장·기록하지 않는다(주소에도 넣지 않는다). 화면도 입력 칸에만 두고 닫으면 지운다. 키를 저장해 두고 정기 동기화하는 방식은 키 보관 방식 결정이 필요해 하지 않았다. 바깥 주소는 `FVOCI_ZOTERO_API_URL`(기본 `https://api.zotero.org`).
- **권한**: 가져오기는 그 연구 항목에 쓸 수 있는 사람(독자 403, 연구가 아닌 항목 400). 키 거부·없는 라이브러리·컬렉션은 400, Zotero 연결 실패·혼잡은 502(응답에 키를 넣지 않음).
- **검증**: 통합 테스트(대역 Zotero 서버: 컬렉션 목록·틀린 키 400과 응답에 키 없음·연구가 아닌 항목 400·독자 403·문헌 1건 생성과 메모 건너뜀·서지·컬렉션·원본 링크·내 칸을 고친 뒤 다시 가져오면 서지만 바뀌고 내 칸 유지·없는 컬렉션 400·DB의 기록·감사 기록에 키 없음). 단위 테스트(서지 변환·`javascript:` 주소 버림·내 칸 합치기·입력 확인). **실제 Zotero 서비스: NOT RUN**(테스트 계정·키 없음).

## 52. 독립 리뷰 반영: 항목 멤버 이후 접근, 버전 보존 (2026-10-17)

`2a2eb5a..10bb502`의 서버 변경(권한·RLS·복제·버전 고정·보존·첫 관리자·동시성)을 따로 리뷰했다. 높은 위험은 없었고 아래를 고쳤다(마이그레이션 0045).

- **그룹 변경 뒤 정리**: 그룹에서 빼거나 그룹을 지워 비공개 항목을 볼 수 없게 된 사람은 그 항목 업무의 담당과 항목 리더에서도 빠진다. 지금까지는 항목 멤버를 직접 바꿀 때만 정리했다. 담당 정리(`app_drop_unseen_assignees`)가 리더도 함께 정리하고(감사 `unset_leader`), 그룹 쪽도 멤버 행을 먼저 잠가 동시 지정과 순서를 맞춘다. 휴지통의 항목 리더는 되살릴 때를 위해 둔다.
- **떠나면 항목 멤버도 끝**: 워크스페이스를 떠나거나 내보내지면 항목 멤버 행도 지운다. 다시 초대돼도 예전 비공개 항목이 저절로 열리지 않는다. 이미 떠난 사람의 행도 마이그레이션에서 지웠다.
- **이동 미리보기·담당자 후보**: 둘 다 항목을 볼 수 있는지를 `app_item_access_as` 하나로 본다. 비공개 항목의 멤버도 담당자 후보에 나오고, 문서를 비공개 항목으로 옮길 때 멤버가 "볼 수 없게 되는 사람"으로 잘못 나오지 않는다.
- **리비전의 버전 참조(D-56)**: 리비전을 저장할 때 본문의 첨부 주소에서 (파일, 버전)을 뽑아 `document_file_refs`에 적는다. 예전 리비전은 마이그레이션에서 채웠다. 같은 워크스페이스(개인 영역 파일은 같은 소유자의 노트)의 문서만 버전을 붙잡는다. 다른 곳의 문서가 주소를 적어 남의 버전을 지우지 못하게 하는 일은 막는다. 문서를 옮기면 지금 자리로 다시 본다.
- **첨부 주소 표준형**: 이미지와 `/api/files/`로 시작하는 링크는 화면이 만드는 꼴(소문자 하이픈 id, `version`이 먼저, 앞의 0 없음, 32비트 범위, 뒤에 `&inline=true`만)만 받고 나머지는 400. 다른 꼴로 적어 참조 검사를 피하는 일을 막는다.
- **보존 기간 계산**: 이전 버전은 다음 버전이 올라온 때부터 센다(올린 때가 아니라). 오래전에 올린 v1도 v2가 방금 올라왔으면 기간만큼 남는다. 붙잡힌 버전은 정리 후보에서 미리 빼서, 붙잡힌 버전이 500개를 넘어도 정리가 멈추지 않는다. 참조 검사는 리비전 본문 전체를 훑지 않고 색인을 쓴다.
- **사용자 결정으로 올림(O-34)**: 휴지통 만료가 참조 중인 버전의 파일을 영구 삭제하는 지금 동작은 D-56 문구와 어긋난다. 삭제·보존 정책이라 바꾸지 않고 권장안과 함께 올렸다.
- **검증**: 통합 테스트 2건(그룹 멤버 빼기·그룹 지우기 뒤 리더·담당 비움, 그룹 멤버가 담당자 후보와 이동 미리보기에 맞게 나옴, 떠났다가 다시 초대된 사람은 비공개 항목 404·멤버 목록 비어 있음; 열흘 전 v1도 v2가 방금이면 남고 이틀 지나면 정리, 같은 워크스페이스 문서가 붙잡은 오래된 버전 502개가 있어도 다른 버전 정리, 남의 개인 노트가 적은 주소는 기록되지만 버전을 붙잡지 못함, 대문자 id·매개변수 순서가 다른 주소 400). 단위 테스트(첨부 주소 표준형과 링크). 전체 `cargo xtask test` 통과.

## 53. 출시 후보 다시 검증: 새 checkout과 브라우저 첫 사용자 흐름 (2026-10-17, 커밋 `ae4c3a4`)

50절 뒤의 변경(Zotero 가져오기, 리뷰 반영 0045)을 담은 커밋 `ae4c3a4`를 새로 `git clone`한 디렉터리에서 자체 `target/`으로 다시 확인했다. 셀프호스트는 Keycloak 프로필로 새로 설치하고 첫 관리자 지정 없이(빈 `FVOCI_INITIAL_ADMIN_SUBJECT`) 화면에서 워크스페이스를 만들었다.

**검사가 위반을 잡는지(심어 본 위반, 확인 뒤 되돌림)**: 서버 코드에 `unwrap()`과 한 줄로 몰아 쓴 함수, 웹 `.ts`에 `as` 단언과 타입 오류·형식 위반, `.svelte`에 `any`를 넣고 `cargo xtask check`(와 웹 형식·타입 검사)를 돌렸다. `cargo fmt --check`(형식), `cargo clippy`(`clippy::unwrap_used`), `pnpm lint`(`no-explicit-any`, `consistent-type-assertions`), `pnpm format:check`(Prettier), `pnpm check`(`Type 'string' is not assignable to type 'number'`)가 각각 FAIL. 되돌린 뒤 작업 트리는 깨끗하다.

| 단계                                     | 결과              | 내용                                                                                                                                                                                                                                                                                                                                         |
| ---------------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm install --frozen-lockfile`         | PASS              |                                                                                                                                                                                                                                                                                                                                              |
| `cargo xtask check`                      | FAIL              | 라이선스 두 검사만 FAIL(O-30, 크레이트 20개·npm 9개 그대로, 예외를 넣지 않음). 나머지 PASS                                                                                                                                                                                                                                                   |
| `pnpm --filter @fvoci/web build`         | PASS              |                                                                                                                                                                                                                                                                                                                                              |
| `cargo xtask test --oidc`                | PASS              | 173건 통과·실패 0. OIDC 4건, S3 호환 3건, Zotero·리뷰 반영 새 테스트 포함                                                                                                                                                                                                                                                                    |
| 셀프호스트 설치(`--profile keycloak`)    | PASS              | 이미지 빌드·bootstrap·migrate(0045까지)·app·postgres·meilisearch healthy, `/healthz` ok. Keycloak은 기동만 확인했고 로그인은 아래 브라우저 흐름으로 따로 확인                                                                                                                                                                                |
| 브라우저: OIDC 로그인                    | PASS              | 내장 브라우저에서 Keycloak 화면으로 로그인. 사용자는 발급자+subject로 저장(이메일 병합 없음)                                                                                                                                                                                                                                                 |
| 브라우저: 워크스페이스 만들기            | PASS              | 빈 이름(공백)은 화면에서 막고 알림. "검증 팀" 생성                                                                                                                                                                                                                                                                                           |
| 브라우저: 위키 문서 쓰기·저장            | PASS              | 문서 만들기(WIKI-1), Tiptap 편집기에 글쓰기, 저장으로 리비전 2                                                                                                                                                                                                                                                                               |
| 브라우저: 저장 실패와 재시도             | PASS              | 앱 컨테이너를 멈춘 채 편집·저장하자 "재시도 대기"와 브라우저 보관 안내. 앱을 다시 띄우자 저절로 리비전 3. DB에는 리비전마다 요청 id 하나(중복 저장 없음)                                                                                                                                                                                     |
| 브라우저: 새로고침·다시 로그인·다시 열기 | PASS              | 새로고침 뒤 내용 그대로. 로그아웃 뒤 같은 세션의 API는 401, Keycloak 세션도 끝나 다시 비밀번호를 물음. 다시 로그인해 문서를 열자 리비전 3                                                                                                                                                                                                    |
| 브라우저: 두 탭 동시 저장 충돌           | PASS              | 같은 문서를 연 두 탭. 탭 A 저장(리비전 4) 뒤 탭 B 저장은 "다른 곳에서 먼저 저장됐습니다"와 서버 버전·내 초안 비교. 탭 B를 새로고침해도 초안 복원 안내가 나오고, 복원해 저장하면 다시 충돌(서버 내용을 덮어쓰지 않음). "내 초안으로 다시 저장"을 확인하자 리비전 5, 리비전 4는 기록에 남음                                                    |
| 다른 사용자의 접근                       | PASS              | 두 번째 사용자로 로그인해 문서 주소를 열면 "워크스페이스가 없거나 볼 권한이 없습니다", 문서 GET·PUT 404, 검색 결과 없음, 리비전 수 그대로                                                                                                                                                                                                    |
| 역할 분리                                | PASS              | 앱 연결은 `fvoci_app`뿐이고 이 역할은 표에 직접 쓸 권한이 없다(`has_table_privilege(..., 'INSERT')` false). 마이그레이션은 `fvoci_migrator`                                                                                                                                                                                                  |
| 재시작 뒤 데이터                         | PASS              | 스택 전체 `down`·`up`(볼륨 유지) 뒤 리비전 1~5·사용자 둘 그대로, 세션도 유지. 다시 로그인해 리비전 기록과 r4↔r5 비교 표시                                                                                                                                                                                                                    |
| 백업·복구(커밋 `dcc355e`)                | PASS              | 같은 셀프호스트 이미지로 문서 이미지가 이전 버전(v1)을 가리키는 문서를 만들고(v1 지우기 409) `backup create`(표 46개·파일 2개·마이그레이션 45) → 새로 설치한 두 번째 인스턴스에 `backup restore` → 다시 로그인. 리비전 2, v1 이미지 200 `image/png`, `document_file_refs` 1행 복구, v1 지우기 여전히 409, 검색 다시 만들기(본문 낱말로 찾음) |
| 공유 철회·공개 링크 거두기               | PASS(통합 테스트) | `tests/sharing.rs`(공유 권한·철회·미검증 이메일)와 `tests/public_links.rs`가 위 `test --oidc`에서 통과. 이번 브라우저 확인에서는 다시 하지 않음                                                                                                                                                                                              |
| R2 실검증                                | NOT RUN           | 승인된 테스트 대상과 자격 증명 없음                                                                                                                                                                                                                                                                                                          |
| 원격 CI·CI의 OIDC 테스트                 | NOT RUN           | 원격 push 승인 전                                                                                                                                                                                                                                                                                                                            |

테스트 사용자 비밀번호는 이 설치에만 쓰는 임의 값으로 만들고 설치와 함께 지웠다. 확인 뒤 이 프로젝트 이름(`fvoci-rc2`, `fvoci-rc3`)의 컨테이너·볼륨만 내렸다. `ae4c3a4` 뒤의 `dcc355e`는 감사 기록 화면의 동작 이름(새 감사 동작 12개)만 더한 웹 변경이고, 그 커밋에서 형식·린트·타입·웹 테스트를 다시 통과했다. 백업·복구 확인은 `dcc355e`로 다시 만든 이미지에서 했다.

## 54. 휴지통 만료의 참조 버전 보호(O-34), 라이선스 검사 분리, 실시간·Office 조사 (2026-10-17)

- **휴지통 만료(D-56 일관성 수정, 사용자 지시)**: 작업 프로세스가 만료된 휴지통 묶음을 자동 영구 삭제할 때, 그 묶음과 함께 사라지지 않는 기록(D-35 연결)이나 같은 곳의 문서 리비전(52절 범위)이 참조하는 파일 버전이 있으면 지우지 않고 "만료" 상태로 둔다(마이그레이션 0046). 묶음이 지우는 파일은 파일 묶음, 지워지는 문서의 첨부, 지워지는 항목의 첨부이고, 참조가 같은 묶음 안에서 함께 사라지면(예: 기록과 파일이 같은 항목에 있고 항목째 만료) 그대로 지운다. 참조가 풀리면 다음 실행에서 지운다. 휴지통 화면은 이런 묶음에 "기록이나 문서가 이 파일의 버전을 가리키고 있어 자동으로 지우지 않습니다"를 보인다(`kept_for_references`). 관리자의 직접 영구 삭제는 기존 승인 정책 그대로 두었다(이번 수정에 넣지 않음). 지운 수는 실제로 지운 묶음만 센다.
- **동시성**: 문서 저장이 리비전의 참조를 적을 때 그 버전 행을 공유 잠금(FOR KEY SHARE)하고, 자동 정리는 지울 버전 행을 먼저 배타 잠금(FOR UPDATE)한 뒤 참조를 다시 본다. 저장이 먼저 잡으면 정리는 저장이 끝나기를 기다렸다가 새 참조를 보고 건너뛴다. 정리가 먼저 지웠으면 저장은 기다렸다가 오류 없이 끝난다(지워진 뒤에 적힌 주소일 뿐, 지울 때는 참조가 없었다). 기록 연결은 원래 휴지통의 파일을 받지 않고 파일·버전 행을 공유 잠금한다.
- **이전 버전 정리의 경합(테스트 중 발견)**: 정리 후보를 고른 뒤 지우기 전에 새 참조가 생기면 지우기 트리거가 오류를 내 그 실행의 정리 작업 전체가 멈췄다(데이터 손실은 없음). 후보 행을 잠근 뒤 참조를 다시 보고 건너뛰게 했다. 트리거 확인은 마지막 방어선으로 남는다.
- **독립 리뷰 반영(마이그레이션 0047)**: (1) 남겨 둘 묶음까지 버전 행을 잠가 작업 트랜잭션 내내 저장을 막고, 묶음 순서와 저장의 잠금 순서가 달라 교착할 수 있었다 → 참조를 먼저 잠금 없이 보고, 지울 묶음만 `FOR UPDATE SKIP LOCKED`로 잠근다. 저장 중인 트랜잭션이 행을 잡고 있으면 기다리지 않고 이번 실행에서 건너뛴다(교착 없음). 이전 버전 정리도 같다. (2) 지울 파일을 종류별로 색인 있는 조건으로 고르고 기록은 버전 id로 잇는다(파일 표 전체 훑기 제거). (3) 서로의 첨부를 가리키는 두 문서가 각각 만료되면 둘 다 영원히 남았다 → 참조하는 쪽이 이미 만료된 휴지통에 있으면(되살릴 수 없음) 살아 있는 참조로 보지 않는다. (4) 테스트: 공유 DB 전체의 정리 수 대신 그 묶음의 상태를 보고, 저장이 정리를 기다리는지는 시간 대신 서버의 잠금 대기(`pg_stat_activity`)로 확인한다. 저장이 버전을 잡은 동안 정리가 건너뛰는지는 트리거 잠금이 없으면 실패한다. 문서 묶음(바깥 문서가 가리키면 남김, 서로 가리키는 만료 문서는 지움) 테스트를 더했다. 전체 173건 통과.
- **남은 판단(O-35)**: 참조 때문에 남긴 만료 묶음은 되살릴 수 없고(만료 뒤 되살리기 409) 파일도 열 수 없어, 참조하는 문서의 그림·기록의 파일 열기는 깨진 채 저장량만 차지한다. 휴지통 정책(만료 뒤 되살리기) 변경이라 사용자 결정으로 올렸다. 화면의 "자동으로 지우지 않습니다" 안내는 참조하는 쪽의 이름을 보이지 않으므로, 볼 수 없는 기록·문서의 존재만 1비트 드러난다(의도한 일반 안내).
- **검증**: 통합 테스트 2건. (1) 기록이 연결한 파일·다른 위키 문서가 그림으로 가리키는 파일은 만료 뒤에도 남고 참조 없는 파일은 지워짐, 휴지통 목록의 `expired`·`kept_for_references`, 기록 연결을 풀면 다음 실행에서 지워짐, 같은 항목 안의 기록만 참조하던 파일은 항목째 만료되면 지워짐. (2) 경합: 리비전을 넣는 트랜잭션을 열어 둔 채 정리를 돌리면 정리가 기다리고(0.8초 뒤에도 끝나지 않음), 저장을 확정하면 파일이 남음. 정리 트랜잭션을 열어 둔 채 저장하면 저장이 기다렸다가 200으로 끝나고 파일은 지워져 있으며 정리 수는 참조 없는 묶음 1개. 전체 `cargo xtask test` 172건 통과.
- **라이선스 검사 분리(사용자 지시)**: `cargo xtask check`의 라이선스 단계를 `licenses (cargo deny, pnpm)` 한 단계로 모으고 `cargo deny`는 advisories·bans·sources만 본다. `cargo xtask check --skip-licenses`는 라이선스만 빼고, CI는 `check` job에서 이것을, 따로 `licenses` job에서 `cargo xtask licenses`를 돌린다. 허용 목록 밖이면 `licenses` job이 실패로 그대로 보이고, 다른 job은 그 결과를 기다리지 않는다. 로컬 `cargo xtask check`는 여전히 전부 돈다. SPDX 판정 오류 수정과 O-30 갱신은 9절.

### 실시간 공동 편집: smallstr 권고 조사 (결정 전 사실 정리)

| 항목              | 내용                                                                                                                                                                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 권고              | RUSTSEC-2026-0215 (2026-07-22), `informational = "unmaintained"`. 취약점 권고가 아니라 유지 중단 알림. 모든 판 해당, 수정 판 없음(`patched = []`). 권고는 compact_str·smol_str를 대안으로 든다          |
| 도입 경로         | fvoci-server → yrs 0.28.0(MIT, Yjs 호환 CRDT) → smallstr 0.3.1(마지막 갱신 2025-08-13). yrs 0.24~0.28 전부 `smallstr ^0.3`을 필수(선택 기능 아님) 의존성으로 둔다                                       |
| 사용 기능         | yrs `block.rs`의 글 조각 저장(`SplittableString`, `PrelimString`)에 `SmallString<[u8; 8]>`(8바이트까지 힙 없이 저장)로 쓴다. 공동 편집 본문(Text·XmlFragment) 저장의 핵심 경로라 기능 끄기로 뺄 수 없다 |
| 상위 갱신 가능성  | yrs 최신 0.28.0(2026-09-17)도 smallstr를 쓴다. y-crdt 저장소에 smallstr 교체 이슈·PR이 없다(2026-10-17 검색). 가까운 시일의 상위 수정은 기대하기 어렵다                                                 |
| `cargo deny` 영향 | advisories 검사가 unmaintained를 실패로 본다. 지금 트리에는 yrs가 없어 PASS(실시간은 41차에 보류)                                                                                                       |

최소 변경안(권장 순): (1) `deny.toml`에 이 ID 하나만 이유·제거 조건(yrs가 smallstr를 떼면 지움)과 함께 무시 — 코드 변경은 yrs 도입뿐이고 운영 구성은 그대로. 취약점이 아닌 유지 중단 알림이라 rsa 예외(9절)와 같은 형식. 검사 예외라 승인 필요. (2) y-crdt에 smallstr → smallvec 기반(이미 의존 중) 교체 PR을 올리고 받아들여지면 판 올림 — 시점을 알 수 없다. (3) 고친 yrs를 `[patch]`로 자체 유지 — 유지 부담. (4) 별도 Node 서비스(Yjs 서버) — 새 서비스·운영 부담, 서버의 본문 검증·권한·리비전과 이어 붙이는 일이 생긴다. 사용자 결정 전까지 실시간 ON은 보류(지금은 실시간 OFF 저장·충돌 처리만, D-53).

### Office·HWP 미리보기: 남은 Office 엔진 대안 (rhwp 사용 결정 유지)

HWP·HWPX는 rhwp(`@rhwp/core` 0.8.7, MIT, npm 의존성 없음, WASM 약 11.8 MB)로 한다. 패키지는 확인했다(`LICENSE`·JS·WASM·타입만). WASM 안에 컴파일된 Rust 크레이트(D-60)를 v0.8.7 태그의 `Cargo.lock`과 매니페스트로 wasm32 대상 `cargo tree`(일반 의존성 144개)를 떠서 확인했다: 대부분 MIT/Apache-2.0 계열이고, 허용 목록으로 풀리지 않는 것은 **zlib-rs 0.6.7(Zlib)** 하나다(rhwp → image → png → flate2 → zlib-rs). unicode-ident(Unicode-3.0)는 빌드 때만 쓰인다. WASM 문자열에서 글꼴 라이선스(OFL·SIL) 문구는 찾지 못했다. **따라서 rhwp HWP 미리보기도 O-30의 Zlib 승인이 있어야 브라우저 번들에 넣을 수 있다**(PDF 본문 검색·브라우저 DOCX·PPTX 뷰어와 같은 승인 하나로 묶인다). 승인 전에는 의존성으로 추가하지 않았다. Office(DOCX·XLSX·PPTX) 대안과 부담:

| 방식                      | 후보(판, 라이선스)                                                                                                                      | 새로 생기는 라이선스·배포 부담                                                                                 | 재현 수준·자원                                                 |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| 브라우저 경량 뷰어 — DOCX | docx-preview 0.4.1 (Apache-2.0) → jszip 3.10.2 `(MIT OR GPL-3.0-or-later)`(MIT 선택) → pako `(MIT AND Zlib)`                            | Zlib(pako)이 브라우저 번들에 들어감 → O-30의 Zlib 승인에 묶임                                                  | 문단·표·그림 양호, 페이지 나눔·수식 근사. 브라우저 메모리만 씀 |
| 브라우저 경량 뷰어 — XLSX | read-excel-file 9.3.12 (MIT, fflate·saxen·unzipper-esm 모두 MIT)                                                                        | 없음(허용 목록 안)                                                                                             | 셀 값 표만. 서식·차트·수식 결과 표시는 제한                    |
| 브라우저 경량 뷰어 — PPTX | pptx-preview 1.0.7 (ISC, echarts Apache-2.0·lodash MIT·tslib 0BSD), @aiden0z/pptx-renderer 1.3.0 (Apache-2.0, mtx-decompressor MPL-2.0) | 0BSD(tslib) 또는 MPL-2.0이 번들에 들어감, 둘 다 jszip → pako(Zlib)                                             | 도형·글자 근사, 재현 편차 큼                                   |
| 서버 측 변환              | Gotenberg(MIT) + LibreOffice 헤드리스(MPL-2.0 등) → PDF                                                                                 | 새 서비스 컨테이너(수백 MB 이미지), 신뢰할 수 없는 문서를 다루는 격리, CPU·메모리. **새 서비스라 사용자 결정** | 재현 높음(PDF), 변환 지연                                      |
| 제외                      | OnlyOffice Docs(AGPL-3.0, D-60 밖), Collabora Online(새 서비스·무거움)                                                                  | —                                                                                                              | —                                                              |

권장: XLSX는 허용 목록 안인 read-excel-file로 먼저, DOCX·PPTX는 O-30의 Zlib 승인 여부를 보고 브라우저 뷰어(DOCX는 docx-preview) 또는 변환 서비스 중 고른다. 어느 쪽이든 미리보기는 지금처럼 같은 원점의 읽기 전용 화면에서 하고 문서의 스크립트·바깥 주소는 막는다.

## 55. 사용자 결정 D-68~D-71 반영 (2026-10-17)

- **공유 철회·공개 링크 거두기 브라우저 확인**(`cargo xtask dev --oidc` + Vite, 개발 Keycloak 테스트 사용자 둘, 코드 `ab59b67`와 같음): test-user가 개인 노트를 test-user-2에게 읽기로 공유하고 공개 링크를 만듦 → test-user-2의 "공유받은 문서"에 보이고 열면 "읽기 전용", 고치기 API 403. 로그인 없이 공개 링크 API 200·공개 화면 표시 → test-user가 공유와 공개 링크를 철회 → 공개 링크 API 404, 공개 화면 "링크가 없거나 만료·철회되었습니다", test-user-2의 공유 목록 비고 문서 화면 "문서가 없거나 볼 권한이 없습니다"·API 404. PASS.
- **Zlib 허용과 제3자 고지(D-68)**: `deny.toml`·`xtask/src/licenses.rs` 허용 목록에 Zlib만 더했다. SPDX 회귀 테스트는 미승인 라이선스(MPL-2.0·Unicode-3.0)로 바꾸고 Zlib 통과 단언을 더했다. `cargo xtask notices`(9절 "D-68 이후 상태")가 `THIRD_PARTY_NOTICES.txt`를 만들고 제품 이미지 두 곳에 넣는다. `cargo xtask check`에 "third-party notices up to date"를 더했다.
- **참조로 보존된 만료 묶음 되살리기(D-69, 마이그레이션 0048)**: 되살리기의 기한 확인에 "참조 때문에 남긴 묶음이면 허용"만 더했다. 권한 확인(관리자·지운 사람, 지운 사람은 지금도 항목에 쓸 수 있어야 함)은 그대로이고, 자동 정리와는 묶음 행 잠금으로 순서가 정해진다. 휴지통 화면은 그런 묶음에 되살리기 단추와 안내를 보인다. 통합 테스트: 참조 없는 만료 묶음 되살리기 409, 만료 뒤 정리에서 남음, 휴지통의 파일은 독자에게 404, 지우지 않은 작성자·독자의 되살리기 404, 지운 사람이 되살리면 독자에게 원래 v1 내용(200, 바이트 일치)이 열리고 문서 본문의 그림 주소 그대로, 기록의 파일 연결 `available`, 휴지통 목록에서 빠짐. 0047 경합 테스트의 두 번째 단계는 열어 둔 트랜잭션이 다른 테스트의 묶음까지 잡지 않도록 이 묶음만 같은 정리 경로(`app_purge_batch_as(.., true)`)로 지우게 바꿨다(한 번 간헐 실패, 이후 두 번 연속 전체 통과).
- **PDF 본문 검색(D-68 후속): 보류, 결정 필요.** `pdf-extract` 0.12.1(MIT) → `lopdf` 0.42.0으로 구현해 봤다. 크레이트 27개가 더해지고 라이선스는 새로 실패하지 않으며, 서버에는 zlib-rs가 링크되지 않는다(flate2의 miniz_oxide 경로, `cargo tree -e all -i zlib-rs` 결과 없음). 견본: Chrome으로 만든 한국어 PDF는 본문·표 글자를 모두 뽑아 개발 스택에서 "무지개"·"표 안의 글자"로 찾았고, macOS Quartz(cupsfilter)로 만든 PDF는 한글이 빠지고 영문만 남았다(같은 파일을 macOS 색인기·poppler는 한글까지 읽음). lopdf 자체의 글자 뽑기는 두 견본 모두 깨졌다. 그런데 lopdf 0.42가 `ttf-parser` 0.25.1을 선택 기능이 아닌 필수 의존성으로 두고(PDF를 만들 때 글꼴을 넣는 데만 쓰며 글자 뽑기 경로는 쓰지 않음), 그 크레이트에 **RUSTSEC-2026-0192**(유지 중단 알림, 수정 판 없음, 취약점 아님, 원문 확인)가 있어 `cargo deny` advisories가 실패한다. 승인된 예외는 RUSTSEC-2026-0215 하나뿐이라 의존성을 되돌리고 구현은 패치로 보관했다(검색 색인의 글자 뽑기 시간 한도 30초는 남김: 형식 해석기가 멈추거나 패닉해도 색인 작업이 다음으로 넘어감). pdf_oxide도 ttf-parser를 쓴다. 선택지: RUSTSEC-2026-0192 단일 예외 승인, 또는 PDF 본문 검색 보류 유지.
- **rhwp HWP·HWPX 미리보기(D-71): 보류, 결정 필요.** `@rhwp/core` 0.8.7 WASM에 컴파일되는 크레이트(rhwp v0.8.7 태그의 잠금 파일, wasm32 일반 의존성 144개)를 cargo-deny advisories로 확인했다. 브라우저에 실리는 것 중 **zip 8.6.0에 취약점 권고 3건**(RUSTSEC-2026-0367·0368·0369: `ZipArchive::extract`·`ZipStreamReader::extract`의 심볼릭 링크·권한 처리, 9.0.0에서 수정), 유지 중단 알림 **rustybuzz 0.20.1**(RUSTSEC-2026-0206)·**ttf-parser 0.25.1**(RUSTSEC-2026-0192), 회수된(yanked) **chacha20 0.10.1**이 있다(paste는 빌드 때만). rhwp 소스에서 zip의 `extract` 호출은 찾지 못했고 브라우저 WASM에는 파일 시스템이 없어 취약 경로가 닿지 않는 것으로 보이지만, 배포물의 보안 검사 예외라 승인 없이 넣지 않았다. rhwp main 브랜치도 아직 zip 8.x다. 선택지: 이 권고들을 이유와 함께 받아들이고 도입, rhwp 상위가 zip 9로 올릴 때까지 보류, 또는 상위에 갱신 제안.

## 56. 실시간 공동 편집 (2026-10-17, D-27·D-70, 마이그레이션 0049)

- **켜고 끄기**: 워크스페이스 관리자가 위키 화면의 "실시간 공동 편집"으로 켜고 끈다(`PUT /api/workspaces/{id}/realtime`, 감사 `set_realtime`). 워크스페이스 문서만 해당하고 개인 노트는 지금처럼 저장 방식이다. 끄면 열린 편집 방은 리비전을 남기고 닫힌다(4410).
- **구성(D-70)**: 서버는 Rust/yrs 0.28(Yjs 호환), 브라우저는 Tiptap Collaboration·CollaborationCaret(@tiptap/y-tiptap)과 y-websocket. 별도 Node 서비스는 없다. 웹소켓 `GET /api/documents/{id}/collab`은 y-sync 메시지를 그대로 주고받는다. 열기 전에 공개 원점(Origin, GET이라 공통 원점 검사를 거치지 않으므로 직접), 세션, 문서 권한, 실시간 켜짐을 본다(401·403·404·409). 메시지 크기 한도는 문서 크기 한도의 두 배.
- **방(한 앱 프로세스)**: 문서마다 작업 하나가 Yjs 문서를 혼자 맡는다(잠금 없음). 셀프호스트의 한 앱 프로세스(`role=All`)를 전제로 하며 방은 메모리에 있다. 여러 앱 프로세스로 나누는 구성은 지원하지 않는다.
- **내용 검사 경계**: 쓰기 권한이 있는 연결의 변경만, 방 문서를 복사한 임시 문서에 먼저 넣어 저장 API와 같은 본문 검사(`validate_body`: 바깥 주소 그림·링크 형식·크기·깊이·참조 수)를 통과해야 방 문서에 넣고 다른 연결에 보낸다. 통과하지 못하면 버리고 그 연결을 닫는다(4400). 읽기 연결은 내용을 바꾸지 못하고(쓰면 4400) 커서·참여자 표시만 나눈다. 작은 변경은 0.1초씩 모아 처리한다.
- **저장(하나의 저장 경로)**: 몇 초마다 Yjs 상태를 `document_collab_states`에 둔다. 60초 조용하거나 마지막 사람이 나가면 저장 API와 같은 함수(`documents::write_revision`: 문서 행·리비전·링크·언급·버전 참조 트리거)로 리비전을 만든다(마지막으로 고친 사람이 작성자). 실시간 중 저장 API는 제목만 바꾸고 본문 변경은 409. 상태는 이어받은 리비전 번호를 함께 적고, 그 뒤로 본문이 바뀌지 않았으면(제목만 바뀐 저장) 그대로 쓴다.
- **다른 길로 바뀐 본문**: 복원하면 열린 방은 저장 없이 닫고(4409) 복원된 본문으로 다시 연다. 방을 본문에서 새로 만들 때마다 계보 표시(Yjs 맵 `fvoci.lineage`)를 새로 둔다. 예전 계보를 들고 다시 붙은 클라이언트의 변경은 섞지 않고 다시 열게 한다(4409). 브라우저는 4409를 받으면 새 Yjs 문서로 다시 연결한다. 리비전을 만들 때 본문이 그새 다른 길로 바뀌었으면 쓰지 않고 다시 열게 한다.
- **권한·세션 끝**: 연결마다 세션과 권한을 주기적으로(`FVOCI_COLLAB_RECHECK_SECS`, 기본 30초) 다시 보고 끝났으면 닫는다(4403). 쓰기 권한이 읽기로 바뀌면 그때부터 읽기만 된다.
- **화면**: 실시간 문서는 "실시간 연결됨/연결 중/끊김" 상태, 다른 사람 커서와 이름표, 제목 저장 단추를 보인다. 서식 편집만 보이고 Markdown 원문 편집은 감춘다(문서 전체를 바꿔 다른 사람 편집을 덮을 수 있어서). 되돌리기는 Yjs가 맡는다.
- **문서 형식 기준**: 서버(Rust)와 브라우저가 같은 Yjs 표현을 쓰는지 `contracts/collab`의 고정 파일로 양쪽 테스트가 확인한다(웹 테스트가 y-tiptap으로 만든 바이트를 서버가 읽고, 서버가 만든 바이트를 웹 테스트가 읽어 편집기 스키마로 다시 읽으면 저장 형식과 같음). 표·수식·참조·그림·작업 목록·표시(링크 속성 포함)를 담았다.
- **검증**: 통합 테스트 `live_editing_end_to_end`(실제 웹소켓, yrs 클라이언트): 꺼짐 409·관리자만 켜기(작성자 403)·밖의 사람 404·원점 없음 403·세션 없음 401, 두 작성자 수렴, 독자의 쓰기 4400, 바깥 주소 그림 4400이고 남에게 가지 않음, 저장 API 본문 변경 409·제목만 200, 마지막 사람이 나가면 리비전(작성자 = 마지막으로 고친 사람), 방을 다시 열어도 내용 유지, 복원 시 4409와 복원된 본문, 옛 계보 클라이언트 4409이고 옛 내용이 섞이지 않음, 워크스페이스에서 빠지면 4403, 로그아웃한 세션 4403, 끄면 리비전을 남기고 4410 뒤 연결 409. 변환 단위 테스트 2건과 웹 고정 파일 테스트 3건. 브라우저(`cargo xtask dev --oidc` + Vite, 같은 사용자 두 탭): 관리자가 켬, 두 탭 모두 "실시간 연결됨", 한 탭의 글·굵게가 다른 탭에 바로 나타남, 다른 탭 커서와 이름표 표시, 두 탭을 닫았다가 다시 열어도 내용 유지, 나간 뒤 리비전 2·3이 저장 API와 같은 길로 생김. 서로 다른 두 사용자의 브라우저 동시 편집과 HTTPS(wss) 프록시 뒤 연결은 이번에 브라우저로 확인하지 않았다(통합 테스트는 서로 다른 사용자로 확인).
- **의존성**: yrs 0.28.0(MIT), axum `ws`(tokio-tungstenite, MIT·Apache), @tiptap/extension-collaboration·collaboration-caret 3.32.0·@tiptap/y-tiptap 3.0.10·yjs 13.6.33·y-protocols 1.0.7·y-websocket 3.1.0·lib0(MIT). 새 라이선스 실패 없음. 보안 권고는 사용자가 승인한 RUSTSEC-2026-0215(smallstr, 유지 중단 알림) 하나만 `deny.toml`에 사유·제거 조건과 함께 둔다.
- **독립 리뷰 반영(커밋 `230aedc`)**: (1) 브라우저는 표시 이름 끝의 `--xxxxxxxx`를 지우고 읽어서 `link--AAAAAAAA` 같은 이름으로 링크 검사를 피할 수 있었다 → 받은 뒤의 Yjs 문서가 편집기 스키마 모양인지(`convert::check_shape`: 최상위는 `default`·`fvoci`만, 노드·표시 이름은 스키마 목록과 정확히 같아야, 속성은 값만, 글 안 끼운 값 금지, 계보 맵은 `lineage` 글 하나) 먼저 본다. (2) 한 작성자가 큰 프레임에 작은 변경을 많이 담아 서버를 붙잡을 수 있었다 → 한 프레임 메시지 16개, 연결마다 초당 120개·4 MB, 연결마다 모인 변경을 합쳐 한 번 검사, 검사는 블로킹 작업으로, Yjs 상태는 본문 한도의 네 배까지. (3) 참여자 표시 수·크기 한도와 가로채기 → 연결마다 id 4개·항목 4 KB, 처음 알린 연결만 그 id를 바꿈. (4) y-websocket은 4400~4499에서 다시 붙지 않는다 → 혼잡·느림은 4503, 서버 다시 시작은 1012(다시 붙음), 재확인 중 DB 오류로는 닫지 않고, 그때 실시간이 꺼졌으면 리비전을 남기고 4410. (5) 마지막으로 고친 사람이 쓸 수 없게 되면 리비전이 남지 않았다 → 최근에 고친 사람·연결된 작성자 순으로 쓴다. 리비전 직후 상태를 바로 저장한다. 끌 때는 켜진 동안 방을 먼저 닫고(리비전 남김) 끈 뒤 한 번 더 닫는다. 서버를 끝낼 때 모든 방의 리비전을 남긴다. (6) 닫히는 중인 방에 들어오면 실시간이 꺼졌다고 잘못 알렸다 → 답 없이 버려 새 방을 연다. 닫힌 연결은 주기적으로 걸러 낸다. (7) 같은 브라우저 탭끼리 서버를 거치지 않고 주고받지 않게(`disableBc`), 앞선 변경이 빠진 변경은 4409로 새로 열게 한다. (8) 읽기 연결의 변경은 닫지 않고 버린다(편집기가 스스로 맞춘 것일 수 있음).
- **리뷰 반영 검증**: 통합 테스트 `live_editing_guards`(해시 붙은 링크 표시·모르는 최상위·모르는 노드 4400이고 남에게 가지 않음, 메시지 20개 프레임 4400, 앞이 빠진 변경 4409, 다른 연결의 참여자 id 가로채기 무시, 마지막으로 고친 사람을 독자로 바꾼 뒤 나가면 다른 작성자가 작성자인 리비전에 두 사람 글이 모두 남음)와 `live_editing_end_to_end`(독자의 변경은 버리고 연결 유지). 전체 178건 통과. 브라우저(개발 스택): 다른 사용자(test-user)를 독자로 초대해 실시간 문서를 열면 "실시간 연결됨", 본문·제목 읽기 전용, 내용 표시, 연결 유지. 남은 것: 재확인 간격(기본 30초) 동안은 권한이 끝난 연결도 받는다. 실시간을 끄는 사이 저장 API로 본문을 바꾸면 그 방의 마지막 리비전 뒤 편집은 남지 않는다(다시 열라는 닫기와 경고 기록).

## 57. Office·HWP 미리보기: 브라우저 평가와 DOCX·XLSX 구현 (2026-10-17, D-05·D-71)

**견본**(저장소 밖에서 만듦, MIT 생성기 docx 9.12·exceljs 4.4·pptxgenjs 4.0): DOCX(제목, 맑은 고딕·Times New Roman, 굵게·기울임, 머리행 음영 표, 그림, 쪽 나눔 뒤 목록), XLSX(두 시트, 머리행 굵게·채움, 숫자·날짜·SUM 수식, 병합 셀, 열 너비, 그림), PPTX(제목 슬라이드, 머리행 채움 표, 그림·도형). 기준 그림은 macOS 훑어보기(Quick Look)로 만들었다.

| 형식     | 후보(판, 라이선스)                         | 표현                                                                                                                                                                 | 라이선스·보안                                                              | 결론                                                                                    |
| -------- | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| DOCX     | docx-preview 0.4.1 (Apache-2.0, 의존 14개) | 쪽 나눔 2쪽, 제목 색, 글꼴(대체 글꼴로 한글), 표와 머리행 음영, 그림 모두 표시                                                                                       | 모두 허용 목록 안(jszip는 MIT 선택, pako는 D-68 Zlib). npm audit 해당 없음 | **구현**                                                                                |
| XLSX     | read-excel-file 9.3.12 (MIT, 의존 7개)     | 시트 두 개, 값·날짜·수식 결과(179). 서식·병합·그림·차트는 보이지 않음                                                                                                | 모두 허용 목록 안                                                          | **구현**(값 표)                                                                         |
| PPTX     | pptx-preview 1.0.7 (ISC)                   | 견본을 그리지 못함(빈 화면)                                                                                                                                          | tslib 0BSD(미승인), npm audit 중간 3건(echarts·uuid)                       | 제외                                                                                    |
| PPTX     | @aiden0z/pptx-renderer 1.3.0 (Apache-2.0)  | 세 슬라이드·표 채움·그림·도형 잘 표시                                                                                                                                | tslib 0BSD·mtx-decompressor MPL-2.0(미승인)                                | **사용자 결정 필요**(0BSD·MPL-2.0 승인 시 도입)                                         |
| HWP·HWPX | @rhwp/core 0.8.7 (MIT, WASM)               | 빈 서식에서 한글 글·2×2 표를 넣어 HWP·HWPX로 내보내고 다시 읽어 1쪽 SVG로 그림(둘 다 1쪽, 글·표 표시). SVG에 스크립트·바깥 주소 없음. 한컴 글꼴은 함초롬 계열로 대체 | WASM 안 zip 8.6.0 취약점 권고 3건 등(55절)                                 | 보류(권고 결정 필요). 한컴에서 만든 실제 HWP 파일 렌더링은 견본 라이선스 문제로 NOT RUN |

**구현(DOCX·XLSX)**: 첨부 목록에 "미리보기" 단추(내려받을 수 있는 사람에게만 — 바이트를 브라우저로 받아 그리므로 내려받기 제한을 넓히지 않는다). DOCX는 docx-preview로 만든 HTML을 스크립트를 돌릴 수 없는 iframe(`sandbox=""`)의 srcdoc에 넣고, CSP(`default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:`)로 바깥 요청(추적 그림 등)을 막는다. docx-preview는 그림을 문서 안 파일에서만 data URL로 만든다(바깥 관계는 링크에만 씀). XLSX는 read-excel-file로 시트별 값을 우리 표로 그린다(글자 노드, 2000행까지). 둘 다 필요할 때만 불러오는 분리 번들이고, 20 MB보다 크면 내려받기를 안내한다. 검증: 개발 스택 브라우저에서 견본 DOCX(표·그림·쪽)와 XLSX(시트 탭·값) 미리보기, iframe sandbox에 스크립트 허용 없음과 CSP 확인. 웹 단위 테스트(형식 판별). 제3자 고지에 새 번들 패키지가 들어감.

**변환 서비스(Gotenberg·LibreOffice)**: DOCX·XLSX는 브라우저로 충족(XLSX는 값만). 구체적 부족은 (1) PPTX: 잘 그리는 브라우저 렌더러가 미승인 라이선스를 요구, (2) XLSX 서식·병합·차트 표시, (3) 옛 이진 형식(.doc·.xls·.ppt). 이것들이 필요하면 변환 서비스를 새 서비스로 제안한다(새 컨테이너·자원·격리 부담, 사용자 결정).

## 58. 최종 커밋 다시 검증: 새 checkout·셀프호스트 브라우저·재시작·백업 복구 (2026-10-17, 커밋 `29915e1`·`ee74a80`)

원격 작업 브랜치를 새로 `git clone`한 디렉터리(커밋 `29915e1`, 자체 `target/`)에서 문서의 명령을 다시 돌리고, 같은 checkout으로 셀프호스트(`--profile keycloak`, 프로젝트 `fvoci-rc5`)를 새로 설치해 내장 브라우저로 확인했다. 확인 중 고친 `ee74a80`(아래)은 같은 설치를 `git pull` 뒤 `up -d --build --wait`로 올려(올리기 경로) 이어서 확인했다. 테스트 사용자 둘(alice·bob)과 모든 비밀 값은 이 설치에만 쓰는 임의 값이고, 확인 뒤 `fvoci-rc5`·`fvoci-rc6`의 컨테이너·볼륨과 값 파일을 지웠다.

| 단계                                 | 대상                           | 결과             | 내용                                                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------ | ------------------------------ | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm install --frozen-lockfile`     | `29915e1`                      | PASS             |                                                                                                                                                                                                                                                                                                                                                                 |
| `cargo xtask check`                  | `29915e1`                      | FAIL(라이선스만) | 형식·clippy·단위 테스트·advisories·bans·sources·OpenAPI·웹 형식·lint·타입·제3자 고지·웹 테스트 PASS. 라이선스 두 검사만 FAIL: Unicode-3.0 크레이트 19개, npm 9개(O-30 미승인분 그대로, 예외 없음)                                                                                                                                                               |
| `pnpm --filter @fvoci/web build`     | `29915e1`                      | PASS             |                                                                                                                                                                                                                                                                                                                                                                 |
| `cargo xtask test --oidc`            | `29915e1`                      | PASS             | 182건, 실패 0                                                                                                                                                                                                                                                                                                                                                   |
| 셀프호스트 설치                      | `29915e1`                      | PASS             | 이미지 빌드·bootstrap·migrate(0049)·모든 서비스 healthy, `/healthz` ok, 웹 200                                                                                                                                                                                                                                                                                  |
| 브라우저 첫 흐름                     | `29915e1`                      | PASS             | Keycloak 로그인 → 워크스페이스 "최종 검증" → 위키 문서 작성·저장(리비전 2) → 새로고침 → 로그아웃(API 401, Keycloak도 비밀번호를 다시 물음) → 다시 로그인해 같은 내용                                                                                                                                                                                            |
| 올리기                               | `29915e1` → `ee74a80`          | PASS             | 같은 볼륨으로 이미지를 다시 만들어 올림. 세션·데이터 유지                                                                                                                                                                                                                                                                                                       |
| 만료 → 보존 → 되살리기               | `ee74a80`                      | PASS             | 문서 그림이 파일 v1을 가리키고 v2를 올린 뒤 파일을 휴지통으로. 묶음의 지운 시각을 DB에서 40일 앞당기고(시간 경과 대신) 작업 프로세스의 정리 예약을 당겨 실제 작업이 돌게 함 → 묶음 남음·버전 2개 그대로. 휴지통 화면 "만료됨"과 "자동으로 지우지 않습니다…되살릴 수 있습니다" → 되살리기 → 문서의 v1 그림이 다시 열림(200 `image/png`, SHA-256 일치), 첨부는 v2 |
| 다른 사용자 격리                     | `ee74a80`                      | PASS             | bob: 워크스페이스 문서 화면 "볼 권한이 없습니다", GET 404·PUT 404, 파일 404, 검색 결과 없음                                                                                                                                                                                                                                                                     |
| 공유·철회·공개 링크                  | `ee74a80`                      | PASS             | alice가 개인 노트를 bob에게 읽기로 공유·공개 링크 생성 → bob "공유받은 문서"에 보이고 읽기 전용(PUT 403), 로그인 없이 공개 링크 200 → 철회 → 공개 링크 API 404·화면 "링크가 없거나 만료·철회되었습니다", bob 공유 목록 비고 노트 404                                                                                                                            |
| 재시작                               | `ee74a80`                      | PASS             | 스택 `down`·`up`(볼륨 유지) 뒤 사용자·리비전·파일 버전·세션 그대로                                                                                                                                                                                                                                                                                              |
| 백업 → 새 설치에 복구                | `ee74a80`                      | PASS             | `backup create`(표 47개·파일 2개·마이그레이션 49) → 새로 설치한 `fvoci-rc6`에 5절 순서 그대로 복구 → 옛 세션 401, 같은 Keycloak 신원으로 다시 로그인. alice: 리비전 6개, v1 그림(SHA-256 일치)·v2, 개인 노트, 검색 다시 만들어짐. bob: 여전히 404                                                                                                               |
| 셀프호스트 실시간                    | `ee74a80`                      | PASS             | 복구한 설치에서 관리자가 실시간을 켜자 문서가 웹소켓으로 "실시간 연결됨"(공개 주소 원점 확인 통과), 입력 뒤 나가면 리비전 7에 내용이 남음                                                                                                                                                                                                                       |
| 실시간 두 탭·독자·DOCX·XLSX 미리보기 | `891ba31`·`677332c`(개발 스택) | PASS             | 실시간을 켠 워크스페이스에서 같은 문서를 연 두 탭이 서로의 글과 커서 이름을 바로 봄. 독자로 초대받은 다른 사용자는 "실시간 연결됨"·본문과 제목 읽기 전용·작성자의 글 표시, 재확인 간격(30초)을 넘겨도 연결 유지. DOCX는 스크립트 없는 샌드박스 iframe(CSP 그대로)에 2쪽·표·머리행 음영·그림, XLSX는 값·날짜·수식 결과(179) 표                                   |
| R2 실검증·실제 Zotero                | —                              | NOT RUN          | 승인된 대상·자격 증명 없음                                                                                                                                                                                                                                                                                                                                      |

**확인 중 고친 것(`ee74a80`)**: 휴지통의 "만료된 N개 영구 삭제"가 확인 없이 한 번에 지웠고, 참조 때문에 남긴 묶음(같은 화면이 "자동으로 지우지 않습니다"라고 안내한 것)까지 지워 문서의 그림이 끊겼다. 관리자 직접 영구 삭제가 이런 묶음도 지우는 것은 O-34에서 정한 기존 정책이라 동작은 그대로 두고, 확인 단계를 넣고 남긴 묶음이 몇 개 들어 있는지와 그 그림·파일 연결이 끊긴다는 것을 알린다(묶음 하나의 영구 삭제 확인에도 같은 안내). 이 일괄 삭제가 남긴 묶음을 건너뛰게 하는 것은 관리자 권한을 줄이는 정책 변경이라 사용자 결정으로 남긴다.

## 59. 사용자 결정 D-72~D-74 반영과 실시간 끄기 편집 보존 (2026-10-17)

- **라이선스(D-72)**: 허용 목록은 그대로 두고 제시한 패키지·판에만 예외를 둔다. Cargo는 `deny.toml`의 `[licenses] exceptions`(`이름@판`, Unicode-3.0만)이라 판이 바뀌면 다시 실패한다(임시 설정에서 한 판을 바꿔 `rejected`와 `license-exception-not-encountered`가 나는 것을 확인). npm은 `xtask/src/licenses.rs`의 `NPM_EXCEPTIONS`(이름·판·라이선스 식이 모두 같아야 통과, 단위 테스트). 이 컴퓨터(macOS)에서는 Linux용 lightningcss 항목이 "쓰이지 않음"으로만 표시된다.
- **PDF 본문 검색(D-73)**: 보관해 둔 구현(55절)을 다시 넣었다. pdf-extract 0.12.1 → lopdf 0.42.0 → ttf-parser 0.25.1이고 크레이트 27개가 더해진다. `deny.toml` advisories에 RUSTSEC-2026-0192 하나만 사유·제거 조건과 함께 더했고 advisories·bans·licenses·sources 모두 통과. 통합 테스트: PDF 본문 낱말로 파일을 찾고, 깨진 PDF는 이름으로만 색인되며 색인 작업이 멈추지 않는다. 글자 뽑기는 30초 시간 한도 안에서 돈다.
- **PPTX 미리보기(D-74)**: @aiden0z/pptx-renderer 1.3.0. 렌더러는 슬라이드를 DOM으로 그리고 하이퍼링크 주소를 그대로 `href`에 넣으므로 앱 화면에서 돌리지 않는다. `vite.pptx-frame.config.ts`가 프레임 스크립트를 한 파일(`pptx-frame.js`, IIFE)로 만들고(`public/`에 두어 개발 서버와 운영 빌드가 함께 씀, Git에는 넣지 않음), 화면은 그 글을 받아 `sandbox="allow-scripts"`(같은 출처 아님) iframe의 srcdoc에 요청마다 새 nonce와 함께 넣는다. 프레임 CSP는 `default-src 'none'; script-src 'nonce-…'; img-src blob: data:; style-src 'unsafe-inline'; font-src blob: data:`라 바깥 요청이 없다. 바이트는 프레임이 준비를 알린 뒤 `postMessage`로 넘기고(내려받기 권한이 있는 사람만), 프레임은 출처가 없을 때만(주소를 직접 열면 아무것도 하지 않음) 그리며 슬라이드의 링크 주소를 걷어 낸다. 처음에는 URL로 연 격리 프레임에서 모듈 스크립트를 불렀으나 (1) 출처 없는 문서의 모듈 요청은 CORS(`Origin: null`)가 필요하고 (2) 내장 브라우저가 출처 없는 프레임의 네트워크 요청을 막아, 요청이 전혀 없는 지금 방식으로 바꿨다. 운영 번들에는 렌더러·echarts 6.1.0·zrender 6.1.0·tslib 2.3.0이 이 한 파일에만 들어간다(앱 번들에는 없음). **확인한 사실**: 렌더러 번들 안에 mtx-decompressor **1.4.2**(MPL-2.0, 수정 없음)가 들어 있고, 따로 설치되는 mtx-decompressor 1.8.0은 배포물에 들지 않는다. 렌더러는 ECMA-376 도형 정의에서 뽑은 자료의 ECMA 저작권 고지도 함께 낸다(D-74에 적고 사용자 확인 대기).
- **제3자 고지**: `cargo xtask notices`가 패키지의 `THIRD_PARTY_NOTICES*`와 `licenses/`·`LICENSES/` 폴더도 싣고, 라이선스 식에 MPL이 있으면 그 판의 레지스트리 소스 묶음 주소를 적는다. 렌더러 항목에 mtx-decompressor 1.4.2의 소스 입수 경로(태그 묶음)·MPL-2.0 원문·ECMA 고지가 실린다. 프레임 스크립트도 소스맵과 함께 따로 빌드해 패키지를 모은다(서버 크레이트 250개, 웹 73개).
- **실시간 끄기의 편집 보존(사용자 지시)**: 전에는 끄기가 "방 닫기 → 플래그 끄기 → 방 한 번 더 닫기"였다. 그 사이 방이 다시 열리거나, 끈 뒤 저장 API가 본문을 바꾸면 방의 마지막 리비전 뒤 편집이 리비전 쓰기에서 거절되어 사라졌다. 이제 (1) 그 워크스페이스에 새 방이 열리지 않게 막고(방 목록과 같은 잠금 아래, 들어오려는 연결은 다시 붙는 4503), (2) 열린 방마다 편집을 리비전으로 남기게 한 뒤 답을 기다리며, (3) 모두 남았을 때만 플래그를 끈다. 실시간이 켜진 동안 저장 API의 본문 변경은 409라 그 사이에 끼어들지 못하고, 끈 뒤에는 남은 방이 없다. 리비전을 남기지 못한 방(쓸 수 있는 사람이 없음, DB 오류, 10초 넘음)이 있으면 끄지 않고 409이며, 그 방은 닫지 않아 편집과 연결이 그대로 남고 다음 시도에서 다시 리비전을 남긴다. 닫기 완료를 `Notify`로 기다리던 것은 이미 끝난 방의 알림을 놓쳐 10초를 다 기다릴 수 있어 방의 답(oneshot)으로 바꿨다. 회귀 테스트 `turning_realtime_off_keeps_live_edits`: 끄는 중 새 연결 4503 → 마지막으로 고친 사람이 독자가 되어 남길 수 없으면 409·실시간 켜짐·연결 유지·본문 그대로 → 다시 작성자가 되면 204·연결 4410·편집이 리비전에 있음 → 끈 뒤 옛 리비전 바탕 저장은 409, 새 리비전 바탕 저장은 200. 남은 것: 마지막 사람이 나가 방이 스스로 닫힐 때 쓸 수 있는 사람이 없어 리비전을 남기지 못한 편집은 실시간 상태로만 남는다(다시 켜면 이어지지만, 꺼진 동안 저장 API로 본문을 바꾸면 버려진다).
- **독립 리뷰 반영(커밋 `00ca7d5`)**: (1) **PDF 압축 폭탄**: lopdf는 Flate·LZW 스트림을 크기 한도 없이 풀어 작은 PDF 하나로 메모리를 다 쓰고 프로세스가 죽을 수 있었다(시간 한도·패닉 잡기로는 못 막음). 해석기에 넘기기 전에 스트림마다 zlib·날 deflate를 합계 64 MiB 안에서 미리 풀어 보고, 넘거나 LZW·암호화 PDF면 이름으로만 색인한다(단위 테스트: 70 MB를 푸는 1 MB 미만 PDF는 1초 안에 거절, 압축된 보통 PDF는 읽음). (2) **방 없이 남은 실시간 상태**: 마지막 사람이 나갈 때 리비전을 남기지 못한 방의 편집은 상태로만 남아 끄기 확인을 빠져나갔다. 상태에 `pending`(리비전 뒤 편집 있음)을 적고(마이그레이션 0050), 끄기 전에 그런 상태를 같은 리비전 길로 남긴다. (3) **편집한 사람이 더는 쓸 수 없을 때**: 끄는 관리자가 대신 작성자가 되어 리비전을 남긴다(독자만 남은 방 때문에 끄기가 막히지 않음). (4) **겹친 끄기 요청**: 막음을 워크스페이스별 개수로 세어 한 요청이 끝나도 다른 요청 동안 방이 열리지 않는다. (5) **켜기와 저장의 순서**: 저장 API가 실시간 여부를 워크스페이스 행 공유 잠금과 함께 읽어(`app_document_realtime_locked`) 켜기·끄기와 엇갈리지 않는다. (6) PPTX 프레임: 스크립트에 `</script`·`<!--`가 있으면 고쳐 쓰지 않고 넣지 않으며(프레임 빌드도 실패), 바이트는 프레임마다 한 번만 보낸다. 회귀 테스트는 끄는 중 4503, 열린 방(독자가 된 편집자 → 관리자 작성자)과 방 없이 남은 상태가 모두 리비전으로 남음, 겹친 끄기 두 요청 모두 204, `pending` 0, 끈 뒤 옛 바탕 저장 409를 본다. 남은 것: 리비전을 남기지 못하는 경우는 DB 오류·10초 넘음·본문 검사 실패뿐이고 그때는 409로 켜진 채 둔다. 같은 리뷰가 지적한 "작업이 죽으면 같은 파일을 다시 잡아 또 죽음"은 PDF 쪽 원인을 막아 두었고 대기열 시도 횟수 기록은 하지 않았다.
- **rhwp 취약 의존성 갱신 가능성(사용자 지시, 2026-10-17 확인)**: 배포판 @rhwp/core 0.8.7 = 상위 main(`1a76570`)은 zip `8.5`·ttf-parser 0.25·rustybuzz 0.20. 개발 가지 devel(`515bd72`, 오늘)은 rhwp 자신의 zip을 `9.0`으로 올렸지만, 새로 들어온 PDF 가져오기 기능(`pdf-import`, **기본 켜짐**)의 office_oxide가 `=0.1.9`로 고정(pdf_oxide 0.3.78과의 호환 문제라고 적혀 있음)이라 **zip 8.6.0이 WASM에 그대로 남는다**. 같은 감사(cargo-deny, wasm32, rhwp 일반 의존성)에서 devel도 7건: zip 8.6.0 취약점 3건(RUSTSEC-2026-0367·0368·0369), rustybuzz 0.20.1(RUSTSEC-2026-0206)·ttf-parser 0.25.1(RUSTSEC-2026-0192) 유지 중단, paste(빌드 때만), 회수된 chacha20 0.10.1. 갱신 가능성: chacha20은 0.10.2(회수 안 됨)로 잠금 파일만 바꾸면 해결(`cargo update -p chacha20` 확인). office_oxide 0.1.15(10-15)가 zip `^9.0`이라 고정을 풀면 zip 8이 빠지지만 상위가 적은 호환 문제를 풀어야 하고, `--no-default-features`(PDF 가져오기 끔)로 만들면 지금도 zip 8이 빠진다(확인). rustybuzz·ttf-parser는 고친 판이 없고 후속(harfrust 0.14, skrifa — 이미 트리에 있음)으로 옮기려면 상위 코드 수정이 필요하다. 재제안은 `docs/decisions.md` O-36.
- **제품 이미지 확인(코드 `124424c`와 같음, 셀프호스트 `fvoci-rc7`, 내장 브라우저)**: 이미지가 `/pptx-frame.js`를 내준다. PPTX 미리보기 3장(표·그림·도형), 프레임은 `sandbox="allow-scripts"`·부모에서 읽을 수 없음(출처 없음)·nonce CSP. 한국어 PDF(Chrome으로 만든 견본)를 올리면 약 30초 안에 본문 낱말 "무지개"로 찾고 본문 조각이 보인다. 확인 뒤 설치·볼륨·값 파일을 지웠다. 로컬 `cargo xtask check` 전 단계 PASS(라이선스 포함), `cargo xtask test --oidc` 187건 통과.

## 60. 문서 한 건 내보내기(D-75)와 rhwp HWP·HWPX 전용 빌드 시험 (2026-10-17)

- **내보내기 구조**: 서버는 `POST /api/documents/{id}/revisions/{revision}/export`(형식 `markdown`·`zip`·`pdf`)에서 권한을 보고(`app_document_can_export`: 고칠 수 있거나, 볼 수 있고 소유자가 복제·내려받기 허용 — 첨부 내려받기 규칙과 같음, 개인 노트·공유 문서도 같음) 내보내기 기록을 남긴 뒤(워크스페이스 감사 기록, 개인 노트는 노트 소유자 개인 영역, 마이그레이션 0051) 고른 저장 리비전을 준다. 문서 보기에 `can_export`가 있어 화면은 그때만 "내보내기"를 보인다. 변환은 브라우저가 편집기와 같은 스키마·`@tiptap/markdown`으로 한다(`apps/web/src/lib/features/documents/export.ts`, 두 번째 문서 모델 없음). @참조는 참조 해석 API로 이 사람이 볼 수 있는 대상만 "키 제목"과 앱 링크로, 볼 수 없으면 "볼 수 없는 대상"으로 바꾼다(이름·id를 남기지 않음). 체크리스트는 `- [x]`/`- [ ]`, 표·코드·링크·수식(`$$`)은 Markdown 그대로. 최상위 블록마다 Markdown으로 썼다가 다시 읽어 그대로 돌아오지 않는 블록(예: 병합한 표 칸)을 "몇 번째 블록(표)"로 알리고 파일 맨 위 주석과 창에 적는다. Markdown 한 파일은 그림·첨부를 앱 주소(로그인 필요)로 두고, ZIP은 파일 내용 API로 받아(파일마다 내려받기 권한 다시 확인) 받을 수 있는 것만 `files/<id 끝 8자>-v<판>-<이름>`에 담고 상대 경로로 잇는다(못 받은 것은 "(이름: 내려받을 수 없어 넣지 못함)"과 알림). PDF는 같은 변환으로 만든 HTML(수식은 KaTeX로 배치)을 스크립트 없는 같은 출처 iframe에 넣고 그림을 다 불러온 뒤 브라우저 인쇄를 연다(새 서비스·PDF 엔진 없음). 앱은 글꼴을 배포하지 않고(KaTeX 글꼴도 미포함) 사용자 기기의 글꼴을 쓰며 PDF 글꼴 임베딩은 브라우저가 그 글꼴의 허용 범위 안에서 한다. 저장하지 않은 편집이 있으면 창에 "먼저 저장하거나 저장된 리비전 N을 내보낸다"를 알리고 단추도 "저장된 리비전 N 내보내기"로 바뀐다. 실시간 문서는 리비전으로 남은 내용까지 내보낸다고 알린다. 업무·진도 상태는 문서 본문이 보이는 그대로(참조는 제목·키)만 담고 원본을 바꾸지 않는다. 위키 전체·하위 문서 일괄 내보내기는 범위 밖(별도 범위).
- **검증**: 통합 테스트 `export_follows_download_permission_and_is_audited`(작성자·관리자 200, 독자는 허용 전 403·허용 뒤 200, 볼 수 없는 사람 404, 없는 리비전 404, 모르는 형식 거절, 고른 리비전 본문, 개인 노트 공유 독자도 같은 규칙, 기록 3건·형식·리비전, 개인 노트 기록은 소유자 영역·행위자는 내보낸 사람). 웹 단위 테스트(제목·링크·체크리스트·코드·표 보존, 볼 수 있는 참조만 이름, 병합 표 알림, ZIP은 내려받을 수 있는 파일만·상대 경로, PDF HTML의 KaTeX·한글, 파일 이름). 브라우저(개발 스택): 저장하지 않은 편집 알림과 단추 문구, Markdown 파일 내용, ZIP(`연구 노트.md` + `files/…-그래프.png`, 상대 링크), PDF용 HTML을 headless Chrome으로 A4 PDF로 만들어 확인(5쪽, 한글·표·코드·체크리스트·수식·그림, 표가 쪽 사이에서 갈리지 않음, 글꼴은 기기의 Apple SD Gothic Neo·SF 부분 임베딩). 내장 브라우저에서 인쇄 창은 페이지를 막아 자동 확인은 인쇄 직전 HTML로 했다.
- **rhwp HWP·HWPX 전용 WASM 시험(사용자 지시)**: 상위 devel `515bd72dcde0`에서 Rust 1.93.1(고정 도구 모음)·wasm-bindgen-cli 0.2.127(잠금 파일과 같은 판, 시험 폴더에만 설치)로 `--no-default-features --features console_error_panic_hook`(기본에서 `pdf-import`만 뺌) 빌드. 잠금 파일은 chacha20 0.10.1→0.10.2(회수 해소)만 바꿈. 실제 wasm32 그래프 144개(기본 212개), 빌드 기록상 wasm32로 컴파일된 크레이트 131개에 zip은 9.0.0 하나뿐이고 zip 8.6.0·office_oxide·pdf_oxide·tiny-skia는 **잠금 파일에만 있고 컴파일되지 않음**. 같은 그래프의 cargo-deny 권고는 유지 중단 알림 3건만: rustybuzz 0.20.1(RUSTSEC-2026-0206)·ttf-parser 0.25.1(RUSTSEC-2026-0192, 둘 다 배포물에 들어감), paste(RUSTSEC-2024-0436, 빌드 때만). 취약점·회수 없음. 크기 14.8 MB(wasm-opt 전, gzip 5.1 MB; npm 0.8.7은 wasm-opt 후 11.8 MB), 깨끗한 빌드 218초. 검증(Node, nodejs·web 빌드 같음): 빈 문서에 한글·2×2 표 → HWP·HWPX 저장 → 다시 열기 → 1쪽 SVG, 상위 견본 HWP 3개·HWPX 4개 모두 0.8.7과 쪽 수·한글 글리프 수 같음, SVG에 스크립트·이벤트 속성·바깥 주소 없음(그림은 data:). PDF는 `PDF_IMPORT_DISABLED`로 깨끗하게 거절. 남은 유지 중단 알림 2건(배포물)과 도입 방식(빌드 단계·CI 도구)은 O-36에 별도 범위로 적었다.

## 61. HWP·HWPX 미리보기: rhwp 전용 WASM 도입 (2026-10-17, D-76)

- **빌드 입력과 단계**: `deploy/rhwp/`의 네 파일(build.sh·inputs.env·Cargo.lock·deny.toml)이 입력이다. 상위 커밋 `515bd72dcde0`, 기능 `console_error_panic_hook`만(`pdf-import` 끔), 잠금 파일은 상위 것에서 chacha20 0.10.2·unicode-ident 1.0.27만 바꿈. 제품 이미지(`deploy/docker/app.Dockerfile`)의 `rhwp-build` 단계가 Rust 1.93.1·wasm32·wasm-bindgen-cli 0.2.127·cargo-deny 0.20.2로만 만든다(앱 서버는 1.99.0 그대로). build.sh는 도구 판을 입력과 대조하고, 고정 커밋만 필요한 경로로 받아 커밋을 확인한 뒤(.git은 지움, 일부만 받은 저장소에서 cargo가 파일 목록을 못 읽기 때문) 우리 잠금 파일로 빌드한다. 상위 작업 공간은 다른 구성원이 기본 기능을 켜 cargo-deny가 기능을 합치므로, 구성원을 `crates/*`로 줄인 사본(잠금 파일은 빠지기만 하고 판은 같아야 함, 확인)에서 실제 브라우저 그래프를 `deploy/rhwp/deny.toml`로 검사한다(취약점·회수·라이선스·출처, 예외는 RUSTSEC-2026-0206·0192를 HWP 용도로, paste는 빌드 때만). 결과는 입력 해시 폴더(`/rhwp/8e5afd88811d30c6/`)에 rhwp.js·rhwp_bg.wasm과 br·gzip 사본, `manifest.json`, 고지 자료, 크기 보고.
- **다시 쓰기**: 같은 입력이면 Docker 단계 캐시를 쓰고, `cargo xtask rhwp`는 `target/rhwp/<입력 해시>`가 있으면 빌드하지 않는다(해시는 build.sh와 같은 계산, 둘 다 `8e5afd88811d30c6`로 맞음). CI의 `rhwp` job은 같은 네 파일로 캐시 키를 만들고 `cargo xtask rhwp` 뒤 고지 파일이 그대로인지 본다. `cargo xtask check`는 고지 파일 첫 줄의 입력 해시가 지금 입력과 같은지 본다.
- **고지**: `deploy/rhwp/THIRD_PARTY_NOTICES.txt`(WASM에 컴파일되는 크레이트 131개의 라이선스 식·원문, 상위 THIRD_PARTY_LICENSES.md)를 앱 고지 파일이 그대로 싣는다. WASM에 글꼴·문서 파일은 들어가지 않는다(상위의 `include_bytes!` 중 브라우저 빌드에 들어가는 것은 빈 서식 HWP 하나).
- **전송과 캐시**: 서버는 `/rhwp`를 따로 내보내 `Accept-Encoding`에 따라 br·gzip 사본을 그대로 보내고(`Content-Encoding`), 해시 폴더 파일은 `Cache-Control: public, max-age=31536000, immutable`, `manifest.json`만 `no-cache`. 없는 파일은 404(SPA 화면으로 넘어가지 않음). 통합 테스트 `rhwp_wasm_is_served_compressed_and_cached`. 크기(제품 이미지에서 실제 응답): **WASM 최적화 전 14,850,105 B**(wasm-opt 없음), gzip 5,126,232 B, **brotli 3,557,265 B**; 붙이는 JS 474,083 B → br 36,774 B.
- **화면**: 첨부의 "미리보기"를 누를 때만 `/rhwp/manifest.json`을 읽고(16자리 해시만 받음) 작업자를 만든다. Web Worker가 같은 출처의 해시 폴더에서 rhwp.js·WASM을 불러 문서를 열고 쪽마다 SVG를 보낸다. 화면은 SVG를 `<img>`(blob 주소, 닫을 때 해제)로만 보인다. 한도: 파일 20 MB, 처음 50쪽, 30초(넘으면 작업자를 끝냄), "멈추기" 단추. 손상·지원하지 않는 파일은 "미리보기를 만들지 못했습니다"로 끝나고 화면은 멈추지 않는다. 단위 테스트(멈춘 작업자 시간 초과·취소 뒤 늦은 답 무시·실패·모양이 틀린 메시지 무시).
- **검증(제품 이미지, 셀프호스트 `fvoci-rc8`, 내장 브라우저)**: 미리보기를 열기 전에는 `/rhwp/` 요청이 없음. 상위 견본 HWP 3개(1·1·6쪽)·HWPX 3개(1·1·6쪽)가 그려지고(한글 서식·표·체크 표시), 무작위 바이트 `.hwp`와 가짜 ZIP `.hwpx`는 실패 안내, 화면 오류 0건. 개발 서버에서도 같은 결과. 확인 뒤 설치·볼륨·값 파일을 지웠다.
- **남은 것**: wasm-opt로 크기를 더 줄이는 것은 하지 않았다(도구가 하나 더 필요). rustybuzz·ttf-parser 유지 중단 알림은 rhwp가 후속 크레이트로 옮기면 예외를 지운다.
- **검토 뒤 고침**: build.sh가 받은 소스 안에서 rustc 판을 다시 확인하고 감사 작업 공간을 결과(`audit/`)에 남긴다. `cargo xtask rhwp --audit`이 그 작업 공간으로 권고만 다시 본다(입력이 같아 다시 빌드하지 않는 동안 새 권고를 놓치지 않게, CI `rhwp` job이 매번 실행). 내보내기: ZIP 안 파일 링크를 퍼센트 인코딩(공백·괄호가 든 이름도 링크 유지), PDF도 그림을 내려받을 수 있는지 먼저 보고 못 넣은 그림은 안내 문구로 바꿈, 리비전 목록을 끝까지 넘겨 오래된 리비전도 고를 수 있음, 편집기를 닫으면 "저장하지 않은 편집" 표시를 지움. 웹 단위 테스트 2건 추가.

## 62. ETag·조건부 요청·캐시 범위 (2026-10-17, D-77)

- **캐시 범위**: `crates/server/src/http/cache.rs`의 `policy`가 라우터 맨 바깥 층에서 경로별 `Cache-Control`을 정한다(표는 `docs/self-hosting.md` 7절). `/api`·`/auth`·`/healthz`는 핸들러가 재검증을 허용한 GET·HEAD의 200·304만 `private, no-cache, no-transform`이고 나머지는 핸들러 값과 상관없이 `no-store, no-transform`. `/assets`는 `/rhwp`처럼 따로 내보내 없는 해시 자원이 SPA 화면(200)이 아니라 404(`no-store`)가 된다(오래 캐시되는 주소에 HTML이 남지 않게). 이전의 `rhwp_cache`는 이 층으로 합쳤다.
- **문서 ETag**: `documents::representation`이 단건 조회 응답 바이트와 사용자 ID로 강한 ETag(SHA-256 앞 16바이트, 따옴표 포함)를 만든다. 조회·저장·복원·충돌이 같은 계산을 쓴다. 바이트를 해시하므로 제목·본문·사용자별 필드(`can_edit`·`can_export` 등)·직렬화 형식이 모두 반영되고, 사용자를 넣어 브라우저에서 사용자를 바꿔도 앞 사람의 검증자로 304가 나지 않는다. 앱은 API를 압축하지 않으므로 압축 변형은 하나(identity)다. `If-None-Match`(약한 비교, `*`·목록)는 세션(401)·권한(404) 확인 뒤에만 본다.
- **저장 If-Match**: 문서 행을 `FOR UPDATE`로 잠근 뒤 같은 `request_id` 재시도 판단 → `If-Match`(있을 때, 잠근 행으로 만든 지금 표현과 강한 비교, 약한 태그·빈 값은 불일치, `*`는 통과) → `base_revision` 비교 순서. 불일치면 412 `precondition_failed`와 서버 최신본·그 ETag(`ConflictBody.etag`, 409도 같음). 저장·복원 200도 `ETag`를 준다(곧바로 조회한 값과 같음, 테스트로 확인). 실시간 편집의 제목 저장·CRDT·저장 확인은 바꾸지 않았다(`If-Match`를 보내지 않음).
- **웹**: `api.document`·저장·복원이 응답의 `ETag`를 `DocumentView.etag`에 붙이고, 저장은 강한 값일 때만 `If-Match`를 보낸다(약하거나 없으면 `base_revision`만). 409·412는 같은 충돌 화면(초안 보존). 412인데 리비전이 그대로면(권한 표시만 바뀜) 새 ETag로 한 번만 다시 저장한다. 문서 조회는 브라우저 HTTP 캐시가 `If-None-Match`로 알아서 재검증한다.
- **CDN 예시**: `deploy/cdn/`의 Cloudflare 캐시 규칙·압축 규칙과 CloudFront 행동 조각. 관리형 정책 ID는 AWS 문서(관리형 캐시 정책·관리형 원본 요청 정책)에서 확인했고, Cloudflare의 Respect Strong ETags·압축과 ETag 약화 표·`no-transform`이면 압축하지 않음·캐시 규칙 겹침(뒤가 이김)은 Cloudflare 문서에서 확인했다. 단위 테스트 `cdn_examples_match_the_scopes`가 예시의 경로·정책이 `cache.rs`의 범위와 맞는지 본다.
- **검증**: 통합 테스트 `tests/cache.rs` 3건 — 같은 조회 304(강한·`W/`·목록), 압축 요청 유무와 상관없이 같은 바이트·ETag·`Content-Encoding` 없음, 사용자 전환 200·다른 ETag, 변경 뒤 200·새 ETag와 저장 응답 ETag 일치, 권한 철회(멤버 제거) 뒤 이전 ETag로 404, 로그아웃 뒤 401(`Set-Cookie` 유지); 같은 ETag로 동시 저장 → 200 하나·412 하나(412의 최신본은 이긴 쪽, 진 쪽 내용 미저장), 이긴 요청 재시도는 낡은 `If-Match`여도 멱등 200, 약한 `If-Match` 412, `*` 통과, `If-Match` 없이 낡은 기준 409(ETag 포함); 정적 범위(해시 자원 immutable, HTML·SPA·`pptx-frame.js`·manifest `no-cache`, 없는 해시 자원 404·HTML 아님, 모두 `Vary`)와 민감 응답 12종(성공·401·404·로그인 시작(`/auth/login`)·로그아웃·내보내기·반출 ZIP·공개 링크)이 모두 `no-store, no-transform`. 단위 테스트(범위, ETag, 약한/강한 비교, CDN 예시)와 웹 단위 테스트(ETag 보관, 강한 값만 `If-Match`, 412·409 충돌과 ETag).
- **브라우저(`cargo xtask dev --oidc` + Vite 프록시, Keycloak 테스트 계정, 코드 `6c4687f`)**: 개인 노트 자동 저장이 조회의 ETag로 `If-Match`를 보내 200과 새 ETag(`no-store, no-transform`)를 받고, 다음 저장은 그 새 ETag를 씀. 두 번째 탭이 먼저 저장한 뒤 첫 탭 저장은 412 → 충돌 화면(서버 버전·내 초안 모두 표시, 초안 보존) → "내 초안으로 다시 저장"이 412 본문의 ETag로 `If-Match`를 보내 200(리비전 5). 같은 문서 다시 조회는 `private, no-cache, no-transform`이고 브라우저 캐시 재검증(전송 300 B < 본문 530 B, 304 재사용), 두 탭의 ETag 같음.
- **NOT RUN**: 실제 Cloudflare·CloudFront(승인된 대상·계정 없음). 예시는 문서 확인과 정합성 테스트까지다.
- **판단**: 문서 단건 조회 말고는 재검증(ETag)을 두지 않고 `no-store`로 둔다(개인 데이터를 브라우저 디스크에 남기지 않음). 문서 JSON은 CDN·프록시에서도 압축 없이 오간다(강한 ETag를 지키기 위한 선택, 문서 크기 한도 안).

## 63. 모듈별 기능 대조 (2026-10-17, D-78, 기준 main `da11216`)

이 표는 그날 기준의 기록이다. 뒤에 바뀐 줄: 항목 시간 합계는 65절, 모듈 설명은 66절, 제출·계획·수강 예정일의 달력 표시는 68절(D-80), 모듈별 연결 점검과 개발 예제는 83절.

표기: **구현됨**(코드와 통합 테스트), **브라우저**(사람이 화면에서 만들기→고치기→저장→다시 열기까지 확인한 절), **부분**(일부 흐름이 없음), **미구현**. 절 번호는 이 문서의 구현 기록이다. 공통 기능(업무·문서·체크리스트·첨부와 버전·시간 기록·일정·권한·알림·감사)은 모듈마다 따로 만들지 않았다(D-78).

| 모듈     | 기능                                                        | 상태            | 검증                                  | 근거                                            |
| -------- | ----------------------------------------------------------- | --------------- | ------------------------------------- | ----------------------------------------------- |
| 프로젝트 | 항목 만들기·고치기·복제·템플릿                              | 구현됨          | 통합·브라우저                         | 15·43절                                         |
| 프로젝트 | 키·이름·아이콘(모듈), 설명·리더·목표·기간(항목)             | 구현됨          | 통합·브라우저                         | 15·40절. 모듈 설명은 없음(O-37)                 |
| 프로젝트 | 멤버·접근 권한(사람·그룹, 비공개 항목)                      | 구현됨          | 통합(브라우저는 멤버 고르기 열기까지) | 40·52절                                         |
| 프로젝트 | 문서(트리·리비전·템플릿)                                    | 구현됨          | 통합·브라우저                         | 12·17·18·47절                                   |
| 프로젝트 | 업무·하위 업무·칸반·목록·달력·간트·저장한 보기              | 구현됨          | 통합·브라우저                         | 20·21·46절                                      |
| 프로젝트 | 칸반 끌어서 놓기·터치                                       | 이번 보완(D-79) | 웹 단위·브라우저                      | PR fvoci/GroundZero#3                           |
| 프로젝트 | 마일스톤·선행 관계·후속 재배치                              | 구현됨          | 통합·브라우저                         | 21절                                            |
| 프로젝트 | 산출물·위험·문제·회고 기록                                  | 구현됨          | 통합                                  | 41절                                            |
| 프로젝트 | 보관·해제(항목·모듈), 휴지통·복구(항목)                     | 구현됨          | 통합·브라우저(휴지통)                 | 15·26절. 모듈 휴지통 없음(O-37)                 |
| 프로젝트 | 예상·실제 작업 시간                                         | 부분            | 통합                                  | 업무 단위는 24절, 항목 합계 없음 → 보완         |
| 연구     | 연구 정보(분야·질문·가설·모형·방법·표본·책임자·공동 연구자) | 구현됨          | 통합·브라우저                         | 40절                                            |
| 연구     | 문서·업무·일정                                              | 구현됨(공통)    | 통합·브라우저                         | 12·20·22절                                      |
| 연구     | 투고(제출 예정일·수정 마감일·상태·공동저자)와 마감 알림     | 구현됨          | 통합(알림은 화면 미확인)              | 41·44절                                         |
| 연구     | 제출 예정일·수정 마감일이 달력에 보임                       | 미구현          | —                                     | 보완(D-80)                                      |
| 연구     | 데이터·코드·결과 파일 버전, 분석 기록의 당시 버전 고정      | 구현됨          | 통합·브라우저                         | 31·41절                                         |
| 연구     | 참고문헌(문헌 기록·비교표)·Zotero 가져오기                  | 구현됨          | 통합(실제 Zotero NOT RUN)             | 41·48·51절                                      |
| 연구     | 분석 코드 실행                                              | 하지 않음(X-02) | —                                     | D-34                                            |
| 스터디   | 학습 목표·수준·목표 분량, 공통 계획일·내 계획일             | 구현됨          | 통합·브라우저                         | 40·42절                                         |
| 스터디   | 문서·업무·칸반·간트                                         | 구현됨(공통)    | 통합·브라우저                         | 20·21절                                         |
| 스터디   | 진도(개인)·재배치·복습 기록·복습 알림                       | 구현됨          | 통합·브라우저(알림 화면 미확인)       | 41·42·44절                                      |
| 스터디   | 개인 진도와 공유 업무 상태 구분                             | 구현됨          | 통합                                  | 42절(진행 행 본인만), 20절(업무 공유)           |
| 스터디   | 순 공부시간(종류 "공부", 단위 측정)                         | 구현됨          | 통합·브라우저                         | 24·48절                                         |
| 스터디   | 스터디 화면의 공부시간 합계                                 | 미구현          | —                                     | 보완(항목별 시간 합계)                          |
| 스터디   | 계획일이 달력에 보임                                        | 미구현          | —                                     | 보완(D-80)                                      |
| 강의     | 과목·강좌·회차·회차 길이·CSV 등록                           | 구현됨          | 통합·브라우저                         | 34절                                            |
| 강의     | 날짜별 수강 계획(공통·개인), 이수·미이수·진행률·밀린 회차   | 구현됨          | 통합·브라우저                         | 34·39절                                         |
| 강의     | 공동 노트(항목 문서)·참여자별 진도 집계                     | 구현됨          | 통합·브라우저                         | 34절                                            |
| 강의     | 재수강: 시간은 더하고 완료 회차는 한 번                     | 구현됨          | 통합                                  | 34·39절                                         |
| 강의     | 실제 수강시간과 순 공부시간 구분                            | 부분            | 통합                                  | 종류로 구분(24절), 강좌 화면 합계 없음 → 보완   |
| 강의     | 수강 예정일이 달력에 보임                                   | 미구현          | —                                     | 보완(D-80)                                      |
| 취미     | 활동 정보·문서·업무·칸반·간트                               | 구현됨(공통)    | 통합                                  | 40절                                            |
| 취미     | 재료·장비·장비 사용·활동·결과물 기록                        | 구현됨          | 통합                                  | 41절                                            |
| 취미     | 연습 일정(항목 일정·반복)                                   | 구현됨          | 통합·브라우저                         | 22·23절                                         |
| 취미     | 시간 기록(항목 대상)                                        | 부분            | 통합                                  | 전용 종류 없음(O-37), 항목 합계 없음 → 보완     |
| 취미     | 결과물 파일과 버전                                          | 구현됨          | 통합                                  | 31·41절                                         |
| 공통     | 워크스페이스 달력(일정·업무)                                | 구현됨          | 통합·브라우저                         | 22·46절. 모듈·항목 범위와 다른 날짜 출처는 D-80 |

## 64. 칸반 끌어서 놓기와 터치 (2026-10-17, D-79, PR #3 `8c3bd6a`)

- **구현**: `features/tasks/board.ts`의 `dropBefore`·`planDrop`·`applyMove`가 놓을 자리를 계산하고 기존 업무 이동 API를 그대로 부른다. `TaskBoard.svelte`는 포인터 이벤트로 끌기(마우스는 카드 어디서나 6px 넘게 움직이면, 터치는 ⠿ 손잡이로만 해서 목록 스크롤을 막지 않음), Escape·pointercancel로 취소, 끈 뒤의 클릭은 카드 열기로 보지 않는다. 화살표 버튼과 Alt+화살표는 그대로.
- **실패**: 먼저 화면에서 옮기고 실패하면 되돌리며 이동 알림(오류와 따로)을 남긴다. 403·409(권한 철회·동시 변경·보관된 항목)는 항목을 다시 불러온다.
- **검증**: 웹 단위 테스트 5건(놓을 자리, 같은 열·다른 열, 되돌리기, 오류 문구). 브라우저: 마우스·터치(모바일 보기) 끌기, 실패 되돌림, 목록 보기와 같은 순서, 새로고침 뒤 유지.

## 65. 항목 시간 합계와 취미 시간 종류 (2026-10-17, D-78, PR #4 `4942957`, 마이그레이션 0052)

- **구현**: 시간 종류에 `hobby`. `app_item_time(item)`이 항목·그 업무·문서·회차·공부 단위의 기록을 종류별로 묶어 내 합계와 공유된 합계(공유한 사람 수)를 준다(`GET /api/items/{id}/time`). 항목 화면의 시간 칸은 모듈 종류에 맞는 종류로 바로 시작한다(프로젝트·연구 work, 스터디·강의 study, 취미 hobby).
- **검증**: 통합 `item_totals_group_entries_under_the_item_by_kind`(통합 전체 194건 통과), 브라우저 시작·멈춤·합계.

## 66. 모듈 설명과 모듈 휴지통·되살리기 (2026-10-17, D-78, PR #5 `974b1a4`, 마이그레이션 0053)

- **구현**: `modules.description`(4000자), 관리자만 모듈을 휴지통으로(`DELETE /api/modules/{id}`). 모듈 행만 표시하고 그 안의 항목은 접근 함수가 숨긴다(따로 지운 항목 묶음은 그대로). 되살리기는 그 묶음만, 지금 관리자만. 모듈 안 항목을 먼저 되살리려 하면 409. 영구 삭제는 모듈의 항목·번호·키까지 지운다.
- **검증**: 통합 `module_trash_hides_its_items_and_restores_only_the_module`(따로 지운 항목은 모듈 복구 뒤에도 휴지통에 남음, 권한 확대 없음, 195건 통과), 브라우저 설명 고치기·지우기·되살리기.

## 67. 표시 설정과 자정 분할 집계 (2026-10-17, D-81, PR #6 `51ba6e6`, 마이그레이션 0054)

- **구현**: 계정별 언어·시간대·주 시작·12/24시간(`app_set_display_settings`, NULL은 그대로). 브라우저 값은 처음 제안에만 쓰고 저장된 값을 덮지 않는다. 공통 형식 함수(`$lib/display`)가 날짜·시각·달·요일·숫자를 설정대로 쓰고, 오늘·주·달 경계를 사용자 시간대로 정한다. 시간 요약은 기록을 자정 경계로 나눠 날짜별로 더한다(원본은 나누지 않음, 누적 비율의 내림 차이라 합계가 원본과 같음). `week_start` 쿼리로 일요일 시작 주.
- **검증**: 통합 `display_settings_are_personal_and_kept_independently`, 요약 시험(자정을 넘는 기록 1800/1800, 멈춘 시간 3200/1600, 일요일 시작 주, 196건 통과), 웹 `display.test.ts`. 브라우저: 재로그인 뒤 설정 유지, 월요일 시작 격자.

## 68. 달력 범위와 날짜 출처 (2026-10-17, D-80, PR #7 `e13a682`)

- **구현**: 같은 달력 API에 `module_id`·`item_id` 범위(볼 수 없으면 404). 업무·일정에 더해 강의 회차 시청일, 스터디 계획일, 연구 투고·수정 마감, 산출물 날짜를 `dates`로 준다(개인 계획일이 있으면 그것, 없으면 공유 날짜). 워크스페이스 달력은 모듈·항목 거르기, 모듈·항목 화면은 그 범위의 달력을 넣는다. 일정 사본을 만들지 않는다.
- **검증**: 통합 `calendar_scopes_and_date_sources_follow_item_access`(범위 거르기, 개인 계획일은 본인만, 볼 수 없는 항목 제외, 197건 통과), 브라우저 세 범위.

## 69. DST 반복 회차 (2026-10-17, D-81, PR #8 `65d3ded`, 마이그레이션 0055)

- **구현**: `app_local_instant`가 현지 시각을 가장 이른 유효 순간으로 바꾸고 없는 시각이면 NULL. 반복 회차는 없는 시각이면 건너뛰고(`skipped`로 표시) 두 번 있는 시각은 앞선 순간 한 번. 반복의 시간대·원래 시각은 그대로. 일정 입력은 없는 현지 시각을 거절한다.
- **검증**: 통합 `weekly_events_skip_missing_local_times_and_take_the_first_of_repeated_ones`(미국 동부 3월 8일 건너뜀, 11월 1일 05:30Z, 198건 통과). NOT RUN: 일정 입력 거절 문구의 브라우저 확인.

## 70. 화면 문구 번역과 오류 코드 번역 (2026-10-17~18, D-81, PR #10 `e597577`, PR #14 `7582a9d`)

- **구현**: Paraglide JS 2(영역 23개 메시지 파일, 한국어 기준·영어). `cargo xtask check`의 i18n 단계가 두 언어 키 일치·빈 값·영역 접두어·영역 사이 중복·컴파일 결과를 본다(1324키). 고정 이름표는 부를 때 계산(언어를 바꾸면 따라감). API 오류는 `errorText(e, 행동)`이 오류 코드로 이유를 붙이고, 400·409의 서버 영어 설명은 영어 화면에서만 보인다. 내보내기 알림·대체 글자는 내보낸 사람의 언어. 생성기가 넣은 UI 구성요소의 영어 문구도 메시지로(`ui/README`).
- **남긴 것**: CSV 가져오기의 상태·우선순위 값(파일 형식), 서버가 주는 기록 종류 이름표(한국어, 서버 쪽 번역은 후속), 코드 주석.
- **검증**: `cargo xtask check` 통과(vitest 108건 포함), 브라우저 영어(홈, 항목, 빈 제목 입력 오류, 업무 만들기·칸반 이동·지우기 확인·토스트)와 한국어(항목 화면, 사이드바 문구). NOT RUN: 모든 화면의 두 언어 전수 확인.

## 71. 사용자 피드백 규칙 (2026-10-17, D-82, PR #11 `1dfdf79`)

- **구현**: `svelte-sonner`(shadcn-svelte 생성) 토스트(PC 오른쪽 아래, 모바일 위쪽 가운데, 같은 키는 하나로 갱신, 닫기 버튼, 3개까지), 연결 배너(`role=status`, 5초마다 `/healthz` 확인, 게이트웨이 502·503·504와 네트워크 오류로 판단), 저장 실패·충돌은 화면에 남는 메시지(기존 동작), 자동 저장 성공은 편집기 상태.
- **검증**: 웹 테스트 3건(토스트 중복 없음·닫기, 배너 표시와 해제, 앱 오류와 게이트웨이 오류 구분), 브라우저 토스트·배너.

## 72. 운영 로그 (2026-10-18, D-83, PR #12 `fe8372c`, 마이그레이션 0056)

- **구현**: `crates/server/src/logging.rs`. `FVOCI_LOG`(없으면 `RUST_LOG`, 기본 info)와 `FVOCI_LOG_FORMAT`(json·pretty). 한 사건 한 줄 JSON(공통 필드와 요청·작업 id), 요청마다 접근 줄 하나(경로 틀, 거절이면 `error_code`), 5xx는 응답으로 바꾸는 곳에서 한 번(`error_code`·`detail` 분류). 라이브러리 상한(info)과 PDF 라이브러리 끔, sqlx SQL 문장 로그 끔, 작업 함수 경고는 SQLSTATE만, 패닉 한 줄. 오류 본문과 응답 머리글에 요청 id, 화면은 5xx에 안전한 문구와 요청 ID. Docker `json-file` 회전(10 MB × 5).
- **리뷰**: 별도 검토자가 PDF 라이브러리의 첨부 내용 출력, sqlx 느린 쿼리·DB 알림의 원문, 실시간 연결 실패 무기록, 패닉 출력, 필드 덮어쓰기를 찾아 같은 PR에서 고쳤다.
- **검증**: 통합 `tests/logging.rs` 6건(48개 동시 요청에서 줄마다 JSON, 500을 요청 id로 찾고 한 번만 기록, 받은 요청 id 검사, TRACE에서도 가짜 비밀 값(쿠키·Authorization·OIDC 코드·state·공개 링크 토큰·쿼리·본문·깨진 JSON·DB 비밀번호·라이브러리 trace) 없음, 레벨·모듈 거르기와 라이브러리 상한·PDF 끔·필드 덮어쓰기 없음, pretty 한 줄, ERROR 거르기에서도 감사 기록 유지), 전체 통합 207건 통과(로컬 격리 Compose), CI 통과. 실제 개발 서버 출력에서 모든 줄 JSON·`/healthz` 숨김·쿼리 미기록·요청 id 일치를 확인.

## 73. 열린 PR 전부를 합친 로컬 릴리스 후보 검증 (2026-10-18, `rc/local` `95ed19e`)

열린 PR #3~#14를 하나로 합친 로컬 브랜치(원격에 올리지 않음)를 새로 `git clone`한 디렉터리에서 문서(2·4·5절)대로 설치했다. #3과 #14는 칸반 화면이 겹쳐, #14에 #3을 합치고 끌어서 놓기 문구를 번역했다(`32f974f`, #3을 먼저 병합하면 #14가 그대로 올라간다). 테스트 사용자(alice·bob)와 모든 비밀 값은 이 설치에만 쓴 임의 값이고, 확인 뒤 두 설치(`fvoci-rc7`·`fvoci-rc8`)의 컨테이너·볼륨과 클론(비밀 값·백업 사본)을 지웠다. 로컬에서 브라우저와 앱 컨테이너가 같은 Keycloak 주소(`auth.localhost:18081`)에 닿도록 앱에 `extra_hosts`(host-gateway)를 더한 커밋하지 않는 덮어쓰기 파일만 썼다.

| 단계                                                        | 결과          | 내용                                                                                                                                                                                                                                                                                                                                                                                                            |
| ----------------------------------------------------------- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo xtask check --skip-licenses`, `cargo xtask licenses` | PASS          | i18n 1324키(이후 기록 종류 193키 더함), vitest 116                                                                                                                                                                                                                                                                                                                                                              |
| `cargo xtask test --oidc`                                   | PASS          | 211건, 실패 0                                                                                                                                                                                                                                                                                                                                                                                                   |
| 셀프호스트 설치(빌드 캐시 없음)                             | PASS          | 이미지 빌드부터 모든 서비스 healthy까지 8분 32초, 마이그레이션 56, `/healthz` ok, 웹 200. 앱 로그는 모든 줄 JSON(작업 줄에 `job_id`), 로그 설정 `json-file` 10m × 5                                                                                                                                                                                                                                             |
| 브라우저 첫 흐름                                            | PASS          | Keycloak 로그인(PKCE) → 워크스페이스 → 위키 문서 작성·저장(리비전 2, 편집기 상태 "저장됨") → 새로고침 → 로그아웃(API 401, Keycloak도 비밀번호를 다시 물음) → 다시 로그인해 같은 내용                                                                                                                                                                                                                            |
| 다른 사용자 격리                                            | PASS          | bob: alice의 워크스페이스 문서 목록·문서 GET·PUT 모두 404, 검색 결과 없음                                                                                                                                                                                                                                                                                                                                       |
| 이번 변경의 데이터                                          | PASS          | bob: 표시 설정(en·월요일·America/New_York), 취미 모듈과 설명, 항목, 취미 시간 기록(뉴욕 23:30~00:30) → 항목 합계 3600초, 날짜별 요약 1800/1800(원본은 한 건), 모듈 휴지통                                                                                                                                                                                                                                       |
| 재시작                                                      | PASS          | `down`·`up`(볼륨 유지) 뒤 세션·설정·모듈 설명·시간·요약 그대로                                                                                                                                                                                                                                                                                                                                                  |
| 백업 → 새 설치에 복구                                       | PASS          | `backup create`(표 47개, 마이그레이션 56) → 새 설치 `fvoci-rc8`에 5절 순서대로 복구 → 옛 세션 401, 같은 Keycloak 신원으로 다시 로그인. bob의 설정·모듈 설명·휴지통의 모듈(되살리기 204 → 모듈 다시 보임)·시간·요약 그대로, alice 문서는 여전히 404. alice: 문서 리비전 2개와 본문, 검색 다시 만들어짐                                                                                                           |
| 화면 전수(두 언어)                                          | PASS          | 개발 스택(`rc/local`)에서 위키 문서·하위 문서·개인 노트·다섯 종류 모듈과 항목·업무(의존·반복·체크리스트)·일정·강의 회차·기록·타이머를 만든 뒤 25개 화면을 한국어와 영어로 열었다. 오류 알림·콘솔 오류·빠진 메시지 키 0건. 영어 화면의 한글은 모두 사용자 내용(제목 등)                                                                                                                                          |
| 화면 동작                                                   | PASS          | 영어: 리비전 기록에서 r2와 지금 비교(제목·지운 줄·더한 줄) → r2로 되살리기 → 리비전 4, 다시 비교하면 차이 없음. 업무 목록(체크리스트 1/2)·간트·달력 보기, 워크스페이스 달력의 반복 예정 회차 3개·일정·강의 회차 날짜, 검색 대화상자(업무·문서 본문)                                                                                                                                                             |
| 찾아 고친 것                                                | PASS          | 깨진 JSON·빠진 필드 같은 요청 거절이 글자 본문이라 화면이 "예상하지 못한 응답"만 보이던 것을 API 오류 형식(코드·설명·요청 id)으로 바꿈(#12 `1b8e906`, 통합 208건 통과)                                                                                                                                                                                                                                          |
| 두 사용자 흐름(개발 스택, Keycloak 테스트 계정 둘)          | PASS          | 관리자가 작성자로 초대·그룹 만들기(화면) → 개인 노트를 읽기로 공유·공개 링크 → 받은 사람: 공유 목록에 보이고 읽기 전용(PUT 403), 가입 전 위키 문서 404 → 초대 수락(화면) → 위키 문서 읽기·고칠 수 있음, 비공개 항목은 404 → 관리자가 공유·공개 링크 철회, 독자로 낮춤, 그룹에 넣음 → 공개 링크 API 404·화면 "링크가 없거나 만료·철회되었습니다", 받은 사람의 공유 목록 비고 노트 404, 위키 문서 읽기만(PUT 403) |
| 실시간 켜기·끄기                                            | PASS          | 켜면 문서가 "Live"로 붙고 본문 저장 API는 409(`invalid_state`, 요청 id 포함, D-70 그대로). 관리자와 작성자의 실시간 입력이 나간 뒤 리비전 5·6·7로 남음. 끄면 일반 편집기                                                                                                                                                                                                                                        |
| 찾아 고친 것: 실시간 목록 Enter                             | PASS          | 실시간에서 목록·체크리스트에 Enter를 누르면 연결이 닫히고 입력이 사라졌다(y-tiptap 3.0.10이 만드는 최상위 `__tiptap_position_migrations:…` 맵을 서버가 거절). 평범한 값만 담긴 그 맵을 허용(#16 `bd8e49c`), 거절 이유를 로그에 남김(#12). 고친 뒤 같은 동작이 리비전으로 남음                                                                                                                                   |
| Markdown 가져오기                                           | PASS          | 실시간을 끈 문서에서 Markdown 원문(제목·굵게·체크리스트 미완료·완료)을 넣고 서식 편집으로 돌아와 저장 → 리비전 2에 heading·paragraph·taskList(checked false·true)                                                                                                                                                                                                                                               |
| 실제 Zotero                                                 | NOT RUN       | 승인된 Zotero 계정·키 없음(통합 시험은 대역 서버로 확인)                                                                                                                                                                                                                                                                                                                                                        |
| 첨부 파일 복구                                              | NOT RUN(이번) | 이번 설치에는 첨부가 없었다(0개). 첨부·그림 버전 복구는 58절에서 확인                                                                                                                                                                                                                                                                                                                                           |
| R2·실제 CDN·실제 휴대 기기                                  | NOT RUN       | 승인된 대상·자격 증명·기기 없음                                                                                                                                                                                                                                                                                                                                                                                 |

## 74. main 병합 뒤 최종 커밋 검증 (2026-10-18, main `8ac42c7`)

사용자 승인으로 이번 작업 PR(#3~#16)을 main에 병합했다. 병합 전 모든 PR을 하나로 합친 후보(`fc913d4`)의 CI가 통과했고, 병합 뒤 main의 트리는 그 후보와 같다. 순서와 병합 커밋: #13 `1e9ddc2`, #16 `f00c56e`, #3 `828ad8c`, #9 `520e7c4`, #4~#8 `be5ad3c`·`ad8f805`·`f894588`·`fe6ef53`·`bc8ffcb`, #15 `c6ff149`, #10 `5f2b8ef`, #11 `7501898`, #12 `0496236`, #14 `8ac42c7`. 흔들리던 보존 시험 때문에 빨갛던 #10·#15는 main(#13 포함)을 받아 CI를 다시 돌려 통과시킨 뒤 병합했다. 태그·릴리스·배포는 하지 않았다.

| 단계                                                                   | 결과           | 내용                                                                                                                                                                                                                                                                                                                                   |
| ---------------------------------------------------------------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| main CI(`8ac42c7`)                                                     | PASS           | check·integration·oidc·licenses·rhwp                                                                                                                                                                                                                                                                                                   |
| 새 clone의 `cargo xtask check --skip-licenses`, `cargo xtask licenses` | PASS           | 원본은 새 clone, Rust 빌드 산출물은 디스크 여유 때문에 기존 `target/`을 같이 썼다(캐시 없는 재현은 CI가 맡는다)                                                                                                                                                                                                                        |
| 셀프호스트 설치(새 clone, `--profile keycloak`)                        | PASS           | 모든 서비스 healthy, 마이그레이션 56, `/healthz` ok, 웹 200                                                                                                                                                                                                                                                                            |
| 브라우저 첫 흐름                                                       | PASS           | Keycloak 로그인 → 워크스페이스 → 위키 문서 작성·저장(리비전 2) → 새로고침                                                                                                                                                                                                                                                              |
| 실시간 체크리스트 Enter(#16)                                           | PASS           | 실시간을 켜고 체크리스트에서 Enter·입력 → 연결 유지, 나간 뒤 리비전 4                                                                                                                                                                                                                                                                  |
| 다른 사용자 격리                                                       | PASS           | bob: 문서 GET·PUT·위키 목록 404, 검색 결과 없음                                                                                                                                                                                                                                                                                        |
| 백업 → 새 설치에 복구                                                  | PASS           | 표 47개·마이그레이션 56 → 새 설치에 복구, 옛 세션 401, 같은 Keycloak 신원으로 다시 로그인해 리비전 4·리비전 4개·본문·실시간 입력 항목 그대로                                                                                                                                                                                           |
| 캐시 없는 새 clone의 `cargo xtask check --skip-licenses`               | PASS           | 빈 `target/`에서 처음부터 빌드(3.3 GB). 모든 단계 PASS                                                                                                                                                                                                                                                                                 |
| 대표 위반                                                              | FAIL(의도대로) | 같은 clone에 서버 코드 `unwrap()`, 웹 `any`, 한국어에만 있는 메시지 키, 형식 어긋남을 넣자 `cargo fmt --check`·`cargo clippy`(`unwrap_used`)·`pnpm format:check`·`pnpm lint`(`no-explicit-any`)·i18n 검사가 실패하고 종료 코드 1. 되돌린 뒤 지움                                                                                       |
| 동시 저장 충돌(브라우저 두 탭)                                         | PASS           | 같은 문서를 연 두 탭에서 A가 먼저 저장(리비전 2) → B 저장은 "충돌 — 선택 필요", 서버 버전과 내 초안을 함께 보이고 초안 보존. "내 초안을 내 노트로 보존"으로 B의 글이 개인 노트로 남고 위키 문서는 리비전 2 그대로                                                                                                                      |
| 저장 실패·다시 시도                                                    | PASS           | 서버를 멈추고 저장 → 연결 배너, 편집기 "저장 실패", 입력 보존. 서버를 다시 띄우면 배너가 내려가고 저장 한 번에 리비전 3(리비전 3개, 중복 없음)                                                                                                                                                                                         |
| 찾아 고친 것: 게이트웨이 오류 문구                                     | PASS           | 서버가 멈췄을 때 오류가 "unexpected response (502)"(번역 안 됨)로 보였다. "저장하지 못했습니다. 서버에 닿지 않았습니다…"처럼 행동과 번역된 이유로 바꿈(#18)                                                                                                                                                                            |
| 기능 확인(main, 개발 스택)                                             | PASS           | 스톱워치 시작·일시정지(1초 기록)·재개·종료, 문서 휴지통(조회 404) → 휴지통 목록 → 되살리기(200), 감사 기록(워크스페이스·문서 만들기, 지우기, 되살리기), 워크스페이스 반출 ZIP(200, `application/zip`, `no-store`)                                                                                                                      |
| 멘션 알림·Zotero 가져오기                                              | 통합 시험      | 브라우저로는 다시 하지 않았다. 멘션 알림은 통합 시험(references), Zotero는 대역 서버 통합 시험으로 확인. 실제 Zotero 계정은 NOT RUN                                                                                                                                                                                                    |
| 새 설치 한 흐름(main `f47b5f2`)                                        | PASS           | 새 clone → 셀프호스트 설치 → alice 로그인 → 워크스페이스·문서 작성·저장(리비전 2) → 새로고침 → 스택 재시작(볼륨 유지, 세션 유지 200, 리비전 2 그대로) → 로그아웃(401) → bob 로그인·초대 수락(화면) → alice 다시 로그인해 bob을 @언급 → bob 다시 로그인하면 알림함에 "alice Test님이 WIKI-1 연속 흐름에서 나를 언급했습니다"(읽지 않음) |
| 라이선스                                                               | PASS           | `cargo xtask licenses` 통과, 예외는 결정 D-68·D-72·D-74·D-84에 패키지·판 단위로 기록. D-84 대상은 제품 이미지 웹 번들에 없음                                                                                                                                                                                                           |
| R2·실제 Zotero·Cloudflare/CloudFront·실제 휴대 기기                    | NOT RUN        | 승인된 대상·자격 증명·기기 없음                                                                                                                                                                                                                                                                                                        |

확인 뒤 두 설치의 컨테이너·볼륨과 클론(비밀 값·백업 사본)을 지웠다.

## 75. Debian 기반·digest 고정 이미지 체크포인트 (2026-10-18, D-85, `chore/pin-images-debian` `49cba68`)

목표와 비교해 빠진 점 세 가지를 고쳤다. 외부 이미지(PostgreSQL·Meilisearch·Keycloak·SeaweedFS)가 태그만으로 지정돼 있었고, PostgreSQL이 Alpine이었으며, `base-image.yml`에 초기 작업 브랜치(`init/repo-scaffold`)용 임시 트리거가 남아 있었다. 사용자 지시("데비안 기반으로 사용해")에 따라 PostgreSQL을 `18.3-bookworm`으로, 빌드·실행 단계를 bookworm으로 바꾸고 모든 이미지를 멀티 아키텍처 인덱스 digest로 고정했다(D-85). 공식 Debian 이미지가 없는 Meilisearch·SeaweedFS(Alpine)와 Keycloak(UBI)은 digest 고정만 한다.

아래는 이 커밋을 새로 clone한 곳(빈 `target/`)에서 돌린 결과다.

| 단계                                                                 | 결과           | 내용                                                                                                                                                                                                     |
| -------------------------------------------------------------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo xtask check --skip-licenses`                                  | PASS           | 캐시 없이 처음부터, 모든 단계 PASS                                                                                                                                                                       |
| `cargo xtask licenses`                                               | PASS           | 예외는 기존 결정 그대로                                                                                                                                                                                  |
| `cargo xtask test --oidc`                                            | PASS           | 278 통과, 0 실패(SeaweedFS S3 포함)                                                                                                                                                                      |
| 대표 위반                                                            | FAIL(의도대로) | `unwrap()`·`any`·한국어에만 있는 키·형식 어긋남 → `cargo fmt --check`·`cargo clippy`·`pnpm format:check`·`pnpm lint`·i18n 실패, 종료 코드 1. 되돌린 뒤 작업 트리 깨끗                                    |
| 제품 이미지 빌드(`--no-cache`)·셀프호스트 실행(`--profile keycloak`) | PASS           | migrate·bootstrap 종료 0, 모든 서비스 healthy, `/healthz` ok, 웹 200, OIDC 발견 문서 200. PostgreSQL 컨테이너는 Debian 12, `postgres` uid 999, 18.3(pgdg), DB 정렬 `en_US.utf8`. `down --volumes`로 정리 |
| 개발 환경 `cargo xtask dev --oidc` → `cargo xtask down`              | PASS           | 서버 listening, `/healthz` ok. down 뒤 이 clone의 컨테이너·볼륨·네트워크 0                                                                                                                               |
| R2·실제 Zotero·Cloudflare/CloudFront·실제 휴대 기기                  | NOT RUN        | 승인된 대상·자격 증명·기기 없음                                                                                                                                                                          |

기존 Alpine PostgreSQL 데이터(uid 70, musl 정렬)는 그대로 올리지 않는다. 셀프호스트는 `docs/self-hosting.md` 6절대로 백업 → 새 설치 → 복구, 개발 스택은 `cargo xtask down`으로 볼륨을 지우고 다시 띄운다.

## 76. 프론트엔드 사용성 개선 (2026-10-18, `feat/frontend-usability`)

사용자 지시: 기존 기능을 실제로 쓰기 편하게 다듬는다(새 기능·전면 재작성 없음). 개발 스택(`cargo xtask dev --oidc` + Vite)에서 테스트 계정으로 로그인 → 워크스페이스(아주 긴 이름, 빈 워크스페이스) → 같은 유형 모듈 둘(프로젝트 PAPA·PAPB)을 포함한 모듈 6개 → 5단계 문서 트리와 문서 17개 → 항목·칸반·달력 → 강의 항목·스톱워치 → 검색 → 휴지통·되살리기를 브라우저로 썼다.

| 찾은 불편                                                                        | 고친 것                                                                                                                                                                                                                                                 |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 사이드바의 모든 메뉴가 선택된 것처럼 보임                                        | 가져온 sidebar 버튼이 비활성일 때도 `data-active="false"`를 남기고 Tailwind v4 `data-active:`가 속성 존재만 봄. 비활성이면 속성을 뺌(`src/lib/ui/README.md` 기록)                                                                                       |
| 워크스페이스 기능(멤버·달력·휴지통·감사)이 본문 밑줄 링크, 모듈은 페이지 맨 아래 | 사이드바를 개인 영역(홈·알림·노트·공유받은 문서·시간·휴지통·활동)과 현재 워크스페이스 영역(바꾸기 메뉴, 위키와 접고 펴는 문서 트리, 모듈 이름·키, 달력·멤버·휴지통·감사)으로 나눔. 마지막 워크스페이스는 이 브라우저에만 기억(지금 목록에 있을 때만 씀) |
| 모바일 서랍이 이동 뒤에도 열려 있고 닫기 단추가 없음, 서랍 제목이 영어 고정      | 이동하면 닫고, 닫기 단추·Esc·바깥 누르기로 닫힘. 서랍 제목·설명 번역                                                                                                                                                                                    |
| 모바일 머리글 제목이 다섯 줄로 줄바꿈, 사용자 메뉴가 머리글을 차지               | 머리글은 한 줄로 줄임. 계정 메뉴(표시 설정·내려받기·로그아웃)는 사이드바 아래로                                                                                                                                                                         |
| 개인 화면(내 시간·휴지통·활동) 머리글이 "홈"                                     | 화면 이름을 보임                                                                                                                                                                                                                                        |
| 칸반·달력이 768px 칸에 갇힘                                                      | 문서·노트는 좁게, 목록·칸반·달력 화면은 넓게                                                                                                                                                                                                            |
| 항목 화면이 길어 업무가 한참 아래                                                | 제목 아래 절 바로 가기(업무·강좌·정보·기록·시간·첨부·문서·일정), 제목에 초점. 경로를 "워크스페이스 › 모듈 · 키" 한 줄로                                                                                                                                 |
| 한국어 화면의 "Up Next", 홈 최근 항목의 이모지(글꼴 없으면 네모)                 | "다음 할 일", 앱 아이콘                                                                                                                                                                                                                                 |
| 한국어 제목이 어절 중간에서 줄바꿈                                               | 제목에 `word-break: keep-all`, 고정 머리글 아래로 숨지 않는 여백                                                                                                                                                                                        |
| 같은 목록 중복 요청                                                              | 모듈·위키 문서 목록을 사이드바·워크스페이스 화면·모듈 절이 한 번 받아 나눔(만들기·이름 바꾸기·휴지통·되살리기 뒤 다시 받음). 항목 화면의 첨부 목록 이중 요청을 기록 폼을 열 때로 미룸. 읽지 않은 알림 수는 이동 때 30초에 한 번, 창 복귀·읽음 처리 때   |
| 사이드바 목록 실패 때 복구 수단 없음                                             | 실패 문구와 다시 시도 단추                                                                                                                                                                                                                              |

보존: 저장·초안·충돌·실시간 편집·권한·개인 노트 비공개·참조·번호·시간·자료 버전·내보내기 계약과 API는 바꾸지 않았다. 사이드바 이동도 앱 라우터(미저장 보호)를 거친다. 사이드바는 서버가 권한으로 거른 목록만 그린다.

서체(Noto Sans·Noto Serif·D2Coding 고정)는 세 서체 모두 OFL-1.1이라 허용 목록 밖이고, 같은 지시가 라이선스 예외를 별도 승인 대상으로 두었으므로 O-37로 올리고 서체 파일은 넣지 않았다. 지금도 웹 글꼴 요청은 0건이다.

| 검증                                | 결과    | 내용                                                                                                                                                                                                                                                                                              |
| ----------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 형식·lint·타입·웹 테스트            | PASS    | `pnpm run format:check`·`lint`·`check`, vitest 121건(사이드바 트리 펼침·접힘·현재 표시, 공유 목록 중복 요청·늦은 응답 버림, `data-active` 회귀 4건 추가)                                                                                                                                          |
| 브라우저(데스크톱 1280px)           | PASS    | 사이드바 현재 위치(문서·모듈·항목의 모듈), 아이콘 막대로 접기·펼치기(상태 유지), 워크스페이스 바꾸기, 긴 이름 줄임, 빈 워크스페이스·없는 워크스페이스(접근 거부 문구, 사이드바에 다른 이름 없음), 문서 휴지통 → 트리에서 빠짐 → 되살리기 → 다시 보임, 목록 요청 실패 → 다시 시도로 회복, 스톱워치 |
| 브라우저(모바일 뷰포트 375px)       | PASS    | 서랍 열기·이동 뒤 닫힘·Esc 닫힘(초점은 여는 단추로), 14개 화면 가로 넘침 0, 실행 중 스톱워치와 머리글 한 줄                                                                                                                                                                                       |
| 영어 화면                           | PASS    | 사이드바·항목 화면·바로 가기 번역, 375px 가로 넘침 0. 한 주 시작 월요일이 홈 "이번 주"에 반영                                                                                                                                                                                                     |
| 네트워크(운영 빌드, `vite preview`) | PASS    | 항목 화면 첫 방문 29건·595 KB(스크립트 2·CSS 1·KaTeX 1·API 24), 다시 방문 29건·28 KB(자산은 304). 요청 원점은 앱 하나뿐, 외부 CDN·글꼴 요청 0. 제품 서버는 `/assets`를 변하지 않는 캐시로 내보내므로(D-77) 다시 방문 때 자산 요청이 더 줄어든다                                                   |
| 실제 휴대 기기·화면 낭독기 실사용   | NOT RUN | 모바일은 뷰포트 흉내만. 낭독기는 접근성 트리(이름·`aria-current`·`aria-expanded`)만 확인                                                                                                                                                                                                          |

남은 것: 서체(O-37 승인 대기), 메인 스크립트 1.69 MB(gzip 487 KB)의 화면별 분할, 항목 화면 절 순서 재배치는 사용 습관을 바꾸므로 하지 않음.

### 76.1 로컬 릴리스 후보 검증 (main + PR #21 + PR #22, 로컬 브랜치 `rc/local-2`, push 안 함)

main `947b874`에 D-85(PR #21)와 이 브랜치를 합친 로컬 후보를 새로 clone해서 확인했다. 두 PR이 같은 자리에 문서 절을 더해 `docs/development.md`·`docs/decisions.md`의 병합 충돌(양쪽 유지)과 절 사이 빈 줄 하나가 생긴다. 병합할 때 같은 식으로 고친다.

| 단계                                                                | 결과 | 내용                                                                                                                                                             |
| ------------------------------------------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo xtask check --skip-licenses`(빈 `target/`)                   | PASS | 모든 단계 PASS. `format:check`는 처음에 검증용으로 둔 `deploy/compose.override.local.yaml`(저장소 밖 파일) 때문에 실패했고 그 파일을 빼면 PASS                   |
| 제품 이미지 `--no-cache` 빌드·셀프호스트 실행(`--profile keycloak`) | PASS | migrate·bootstrap 종료 0, `/healthz` ok, 웹 200, OIDC 발견 문서 200, PostgreSQL Debian 12                                                                        |
| 첫 사용자 흐름(브라우저, Keycloak 표준 OIDC)                        | PASS | alice 로그인 → 워크스페이스 만들기 → 위키 문서 만들기(사이드바 트리에 바로 보임) → 한글·한자·가나 본문 저장(리비전 2) → 새로고침 뒤 그대로                       |
| 동시 저장 충돌                                                      | PASS | 두 탭에서 B 저장(리비전 3) 뒤 A 저장 → "충돌 — 선택 필요", A 초안 보존                                                                                           |
| 다른 사용자 거부                                                    | PASS | bob: 문서 GET·PUT·위키 목록 404, 검색 빈 결과, 워크스페이스 0. 같은 브라우저에 남은 alice의 마지막 워크스페이스 id는 쓰지 않고 사이드바는 "워크스페이스 고르기"  |
| 재시작·다시 로그인                                                  | PASS | 스택 내림·올림(볼륨 유지) → alice 다시 로그인, 리비전 3·리비전 3개·감사 기록(문서 만들기) 그대로                                                                 |
| 백업 → 새 설치에 복구                                               | PASS | 표 47개·마이그레이션 56 백업 → 새 비밀 값의 새 설치에 복구 → 옛 세션 401 → 같은 Keycloak 신원으로 다시 로그인해 본문·리비전 3개·트리 그대로                      |
| 찾아 고친 것                                                        | PASS | 빈 문서에서 편집 영역이 첫 줄(27px)만 눌려 아래 빈 곳을 눌러도 입력이 안 됐다. 편집 가능한 본문에 최소 높이(11rem)·글 커서를 줘 보이는 영역 어디를 눌러도 들어감 |

확인 뒤 두 설치의 컨테이너·볼륨, 이미지, clone, 테스트 사용자 비밀 값과 백업 사본을 지웠다. 개발 서버(`127.0.0.1:5173`)와 셀프호스트(`127.0.0.1:18080`)는 쿠키를 포트로 나누지 않아 한쪽 로그인·로그아웃이 다른 쪽 세션을 바꾼다(검증 환경의 주의점, 제품 동작 아님).

### 76.2 기능 흐름 다시 확인 (`3525ae8` 웹 운영 빌드 + 개발 API, 브라우저)

| 흐름                                                           | 결과    | 내용                                                                                                                                            |
| -------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| 워크스페이스 실시간 공동 편집 ON/OFF                           | PASS    | 관리자가 켬 → 문서가 "실시간 연결됨"으로 열림 → 입력 → 나가면 리비전 2로 저장. 끔 → 같은 문서(같은 id)가 일반 편집으로 열리고 Ctrl+S로 리비전 3 |
| 리비전 비교·복원                                               | PASS    | 기록 창에서 r2 선택 → r3과의 줄 단위 비교 → r2로 복원하면 새 리비전 4(이전 기록 4개 유지)                                                       |
| 편집기 안 Markdown으로 체크리스트                              | PASS    | Markdown 모드에 `- [ ]`·`- [x]` 두 줄 → 저장(리비전 5) → 본문에 체크리스트 2개(하나 완료)                                                       |
| 검색(Meilisearch)                                              | PASS    | Ctrl+K "자료 수집" → 방금 저장한 본문의 체크리스트 글자로 WIKI-6이 찾아짐                                                                       |
| 업무 CSV 등록·간트                                             | PASS    | 두 줄 CSV → "2개 업무를 읽었습니다" → 등록 → PAPA-16(진행 중·높음·태그 설계)·PAPA-17(할 일·보통), 간트에 날짜 막대 둘                           |
| 스톱워치·내 시간                                               | PASS    | 머리글 스톱워치 종료 → 내 시간 주간표(월요일 시작, 24시간 표시 설정 반영)에 작업 34분                                                           |
| 강의 개인 진도                                                 | PASS    | 강의 항목에 회차 추가 → "완료" → 내 진도 100%·완료 1/1                                                                                          |
| 다른 사용자의 멘션 알림·Zotero 실제 계정·R2·CDN·실제 휴대 기기 | NOT RUN | 멘션 알림은 통합 시험(references), 두 번째 사용자 브라우저 세션은 같은 쿠키 저장소라 이번에 하지 않음. 나머지는 승인된 자격 증명·기기 없음      |

### 76.3 최신 head를 합친 후보와 사용자 간 멘션

| 단계                                                                                          | 결과 | 내용                                                                                                                                                                                                                                                                   |
| --------------------------------------------------------------------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 새 clone의 셀프호스트 설치(로컬 `rc/local-2` `89a5506` = main + PR #21 + 이 브랜치 `b18dfa5`) | PASS | 제품 이미지 `--no-cache` 빌드, migrate·bootstrap 종료 0, `/healthz` ok, 웹 200. 내려주는 CSS에 편집 영역 수정(`min-height: 11rem`)이 들어 있고 `/assets`는 `public, max-age=31536000, immutable`(다시 방문 때 자산 요청 없음). 확인 뒤 컨테이너·볼륨·이미지·clone 삭제 |
| 다른 사용자 멘션 알림(개발 스택, 표준 OIDC)                                                   | PASS | Test User가 test-user-2를 작성자로 초대 → test-user-2가 Keycloak 로그인(별도 쿠키 저장소, curl)으로 초대 수락 → Test User가 브라우저 편집기에서 `@Sec` → "사람 Second User" 제안 선택 → 저장 → test-user-2 알림에 읽지 않은 멘션(행위자 Test User)                     |

### 76.4 최종 head 후보의 백업·복구 (로컬 `rc/local-2` `0a3b755` = main + PR #21 + 이 브랜치 `33100ed`)

이 브랜치 `33100ed`의 필수 CI(check·integration·oidc·licenses·rhwp)는 모두 통과했다. 같은 코드를 합친 후보를 새로 clone해 확인했다.

| 단계                              | 결과 | 내용                                                                                                           |
| --------------------------------- | ---- | -------------------------------------------------------------------------------------------------------------- |
| 셀프호스트 설치                   | PASS | 제품 이미지 `--no-cache`, migrate·bootstrap 종료 0, `/healthz` ok, 웹 200                                      |
| 로그인·저장(브라우저, Keycloak)   | PASS | alice → 워크스페이스·문서 → 편집 영역 빈 곳 클릭으로 편집기에 초점 → 한글·한자·가나 본문 저장(리비전 2)        |
| 다른 사용자 거부                  | PASS | bob: 문서·위키 목록 404, 워크스페이스 0, alice 본문 검색 0건                                                   |
| 백업 → 새 설치 복구 → 다시 로그인 | PASS | 표 47개·마이그레이션 56 → 새 비밀 값의 새 설치에 복구 → alice 다시 로그인, 워크스페이스·본문·리비전 2개 그대로 |

curl로 셀프호스트 Keycloak(`auth.localhost`, http)에 로그인하면 Keycloak의 `Secure` 세션 쿠키를 curl이 보내지 않아 400이 난다. 브라우저는 `*.localhost`를 안전한 원점으로 보므로 문제없다(검증 도구의 제약, 제품 동작 아님). 확인 뒤 설치·볼륨·이미지·clone·임시 비밀 값을 지웠다.

## 77. 노션형 편집 조작 (2026-10-18, `feat/notion-editor`)

사용자 지시: 기존 Tiptap 편집기와 문서 형식을 그대로 쓰고 / 명령 메뉴, 블록 손잡이, 선택 도구 막대를 더한다. 편집기를 바꾸지 않고, 문서 형식(스키마)도 바꾸지 않았다. 실시간 편집의 형식 검사(`collab/convert.rs`)와 저장된 문서는 그대로다.

- **/ 메뉴**(`slash-command.ts`, `@tiptap/suggestion`): 텍스트·제목 1~3·글머리·번호·체크리스트·인용·코드·구분선·표·수식·파일(올리기가 되는 곳만)·참조(@ 입력으로 넘김). 한국어·영어·Markdown 표기로 찾는다. 낱말 바로 뒤의 /는 글자다(앞에 빈칸이 있을 때만 연다). 입력기 조합 중의 Enter·방향키·Esc는 메뉴가 쓰지 않는다(`isComposing`, `key === 'Process'`, 편집기의 `view.composing`). 기존 @ 참조 목록에도 같은 처리를 넣었다.
- **블록 조작**(`block-ops.ts`): 위·아래로 옮기기, 복제, 지우기, 아래에 추가가 모두 ProseMirror 트랜잭션 하나다. 그래서 되돌리기와 Yjs 공동 편집에 그대로 실린다. 목록·체크리스트 안이면 그 항목 단위다. 복제는 노드를 그대로 한 번 더 넣는다. 편집기 안 체크 항목은 id가 없는 문서 내용이고 @ 참조는 원본을 가리키는 링크라서, 원본 업무·상태를 바꾸거나 공유하지 않는다. 단축키: Alt+Shift+↑/↓, Ctrl/⌘+Shift+D, Ctrl/⌘+/(메뉴).
- **손잡이**(`BlockControls.svelte`): +는 아래에 빈 블록을 넣고 / 메뉴를 연다. 손잡이 메뉴에는 옮기기·복제·아래에 추가·유형 바꾸기·지우기가 있고, 끌어 옮기기는 ProseMirror 기본 끌어 놓기를 쓴다(정밀한 포인터에서만). 손잡이는 마우스를 올린 블록, 그렇지 않으면 커서가 있는 블록(키보드·터치)에 붙는다.
- **선택 도구 막대**(`BubbleToolbar.svelte`, `@tiptap/extension-bubble-menu`): 유형 바꾸기, 굵게·기울임·취소선·코드, 링크 넣기·고치기·해제·열기. 링크 주소는 https·http·mailto만 받고, 주소처럼 보이는 글에는 https://를 붙인다.
- **바깥 이미지·링크**(`paste-guard.ts`, `content-notices.ts`): 서버 문서 규칙(`check_image_node`·`check_link_mark`)은 그대로 두고, 이미지가 들어오는 길마다 조용히 지우지 않고 알린다. 바깥 이미지를 대신 내려받지는 않는다.
  - 붙여넣기·끌어 놓기: HTML을 편집기가 읽기 전에 고친다. 바깥 http(s) 이미지는 같은 주소의 링크 글(설명 또는 주소)로, 링크로도 둘 수 없는 이미지(`data:` 등)는 빼고, 허용하지 않는 링크는 글자만 남긴 뒤 몇 개를 바꿨는지 편집기 위에 알린다.
  - `![설명](주소)` 입력: 첨부 주소면 이미지, 바깥 주소면 링크로 넣고 알린다(Tiptap 기본 입력 규칙은 끔).
  - Markdown 원문 편집·초안 복원: 원문 아래에 어떤 이미지·링크가 왜 안 되는지 보이고, 저장을 서버에 보내지 않고 원인과 고칠 방법(첨부로 올리기·링크로 바꾸기·지우기)을 알린다. 초안은 그대로 남는다. 편집기 화면에서는 바깥 이미지를 불러오지 않고 주소와 함께 "저장할 수 없음" 표시로 보인다.
- **손잡이 메뉴 열기**: 마우스는 클릭(놓을 때)에 연다. 누를 때 열면 끌기를 시작하는 순간 모달 메뉴가 열려(본문 `pointer-events: none`) 끌어 옮기기와 겹쳤다. 터치·키보드는 메뉴 라이브러리의 기본 동작(터치는 뗄 때, Enter·Space)을 따른다.
- **본문 모양**: 제목 계층·목록 단계·체크리스트 줄맞춤·코드와 넓은 표의 가로 넘김·빈 줄 안내. 서체는 `--font-sans`·`--font-mono` 토큰을 따른다. Noto Sans·Noto Serif·D2Coding 파일은 O-37 승인 전이라 넣지 않았다.
- **새 의존성**: `@tiptap/extension-bubble-menu`·`@tiptap/extensions` 3.32.0(MIT, 기존 Tiptap과 같은 판). 고지 갱신. `@tiptap/extension-drag-handle`은 마우스를 올린 블록만 따르고 키를 누르면 숨어 키보드·터치 요구에 맞지 않아 쓰지 않았다.

| 검증                                        | 결과    | 내용                                                                                                                                                                                                                   |
| ------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 형식·lint·타입·웹 테스트                    | PASS    | vitest 134건(블록 조작 7건: 옮기기·끝에서 멈춤·체크 상태 유지·복제 원본 보존·목록 마지막 항목 지우기·되돌리기·읽기 전용 / 붙여넣기 규칙·링크 주소·/ 검색·입력기 조합 6건 추가)                                         |
| / 메뉴·체크리스트 Enter·Backspace(브라우저) | PASS    | /체크 → 체크리스트, Enter로 다음 항목, 빈 항목에서 Enter로 목록 밖, 다음 문단 첫머리 Backspace로 앞 항목에 합침                                                                                                        |
| 블록 메뉴·단축키·되돌리기                   | PASS    | Ctrl/⌘+/ 메뉴 → 위로 옮기기(줄 끝 커서에서도 그 블록), Alt+Shift+↓, ⌘Z로 한 번에 되돌림                                                                                                                                |
| 선택 도구 막대                              | PASS    | 고른 글자 위에 뜸, 굵게, 링크 넣기(주소 칸에 초점), `javascript:` 거절 안내, 주소 바꾸기, 링크 해제, 적용 뒤 편집기로 초점                                                                                             |
| 실시간 공동 편집(#16 재발 확인 포함)        | PASS    | 실시간 켬 → 체크리스트 Enter·입력, 블록 옮기기·복제·지우기, Yjs 되돌리기·다시 하기 → "실시간 연결됨" 유지, API 로그에 형식 거절·오류 없음. 두 탭이 같은 결과로 수렴                                                    |
| 다른 사용자와 공동 편집                     | PASS    | test-user-2가 표준 OIDC로 로그인한 세션으로 Yjs 클라이언트(Node)를 붙여 문단을 더함 → Test User 편집기에 바로 보임 → 나가면 리비전으로 저장                                                                            |
| 붙여넣기                                    | PASS    | 여러 문단 일반 글, HTML(제목·목록·`script`·`onerror`·바깥 이미지·`javascript:` 링크) → 스크립트·이벤트 속성·바깥 이미지 제거, 나쁜 링크는 글자만. **고친 것**: 전에는 바깥 이미지가 든 붙여넣기에서 실시간 방이 닫혔다 |
| 저장 실패·재시도·초안 복원·리비전 복원      | PASS    | API를 멈춤 → 연결 배너·"저장 실패"·입력 보존 → 다시 띄우고 저장 한 번에 리비전 하나 → 저장 안 한 입력 뒤 새로고침 → "초안 복원" → 저장 → 기록에서 r3으로 복원(새 리비전)                                               |
| 실시간 재접속                               | PASS    | 실시간 중 API 멈춤 → "연결 중…" 상태로 입력 → 다시 띄우면 저절로 다시 붙고 끊긴 동안의 입력이 서버 리비전에 들어감                                                                                                     |
| 모바일 뷰포트(375px)                        | PASS    | 가로 넘침 0, 커서가 있는 블록에 손잡이, 터치 탭으로 메뉴 열기·복제. **고친 것**: 전에는 탭하면 편집기 초점이 빠지면서 손잡이가 사라져 메뉴가 열리지 않았다                                                             |
| 끌어 옮기기                                 | PASS    | 손잡이 dragstart → 다른 줄에 drop → 블록이 옮겨짐(복사 아님), 실시간 유지(브라우저 창이 최소화돼 실제 마우스 대신 같은 DragEvent를 보냄)                                                                               |
| 실제 한글 입력기 조합(/·@·Enter)            | NOT RUN | 브라우저 도구는 OS 입력기를 쓰지 못한다(글자를 바로 넣는다). 조합 중 키를 메뉴가 쓰지 않는 처리는 단위 시험으로만 확인                                                                                                 |
| 실제 휴대 기기·화면 낭독기                  | NOT RUN | 모바일은 뷰포트 흉내, 낭독기는 접근성 트리(이름·역할·`aria-expanded`)만 확인                                                                                                                                           |

위 브라우저 행과 아래 행은 모두 내장 브라우저 창이 최소화된 상태에서 DOM 조회와 스크립트로 보낸 이벤트(PointerEvent·DragEvent·click)로 확인했다. 화면 캡처가 되지 않아 실제 배치·겹침·글자 모양과 실제 마우스·손가락 조작감은 보지 못했다(시각 배치·터치 사용성 NOT RUN).

독립 검토 8건(커밋 `4603d41`에서 모두 고침)과 그 뒤 따로 찾은 결함:

| 항목                                                                                   | 처리                                                                                                   |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| 검토 1. 구분선 등 안쪽이 없는 블록에서 손잡이·메뉴가 다음 블록을 조작                  | 고침. 블록을 늘 "블록 시작"으로 들고(`blockStartingAt`·`blockAtHit`) 구분선·이미지·블록 수식 단위 시험 |
| 검토 2. 끌기를 취소하면 `view.dragging`이 남아 다음 바깥 드롭이 선택 블록을 옮김       | 고침. `dragend`에서 우리가 만든 끌기 상태만 지움                                                       |
| 검토 3. 메뉴를 연 동안 문서가 바뀌면 메뉴가 엉뚱한 위치를 조작                         | 고침. 트랜잭션 위치 대응으로 옮기고 대상이 지워지면 닫음(`mapBlockStart`)                              |
| 검토 4. 메뉴를 열지 않은 누르기(오른쪽 단추·스크롤로 바뀐 터치) 뒤 손잡이 고정이 남음  | 고침. `pointerup`·`pointercancel`·`contextmenu`에서 풀기                                               |
| 검토 5. 초점이 없는데 선택 도구 막대가 뜸                                              | 고침. 편집기나 막대에 초점이 있을 때만                                                                 |
| 검토 6. 숨은 선택 도구 막대가 자리를 차지                                              | 고침. 고정 위치·숨김으로 붙임                                                                          |
| 검토 7. 붙여넣기 정리가 서버 규칙보다 좁음(제어 문자, 정규형이 아닌 `/api/files/`)     | 고침. 서버 규칙과 같게                                                                                 |
| 검토 8. 손잡이에 Tab으로 갈 수 없음                                                    | 고침. Ctrl/⌘+/ 안내(`aria-keyshortcuts`, 화면 낭독기용 설명)                                           |
| 따로 찾음. 바깥 이미지를 붙여넣으면 실시간 방이 닫힘(이 PR 전부터)                     | 고침(`ec09698`). 붙여넣기 정리                                                                         |
| 따로 찾음. `![](바깥 주소)` 입력·Markdown 원문·초안의 바깥 이미지는 저장이 400(전부터) | 고침(`0f1800a`). 입력은 링크로, 원문·초안은 저장 전에 원인·고칠 방법 안내, 초안 보존                   |
| 따로 찾음. `data:` 이미지·`javascript:` 링크를 편집기가 말없이 버림(전부터)            | 고침(`86db300`). 읽기 전에 HTML을 고치고 개수를 알림                                                   |
| 따로 찾음. 복원한 초안의 바깥 이미지를 편집기가 불러옴                                 | 고침(`86db300`). 불러오지 않고 "저장할 수 없음" 표시                                                   |
| 따로 찾음. 마우스로 손잡이를 누르면 끌기 시작과 함께 모달 메뉴가 열림                  | 고침(`0490247`). 마우스는 클릭에 열기                                                                  |

| 검증(2차)                                | 결과 | 내용                                                                                                                                                                                                                                        |
| ---------------------------------------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 웹 테스트                                | PASS | vitest 144건. 이미지·블록 수식의 찾기·복제·지우기·옮기기, 앞 편집·앞 블록 삭제·대상 삭제 때의 위치 대응, HTML 정리 개수, 바깥 이미지 표시, `![]()` 입력                                                                                     |
| 바깥 이미지 경로(브라우저)               | PASS | 붙여넣기 → 링크·제거·링크 해제를 한 번에 알림. `![](바깥)` 입력 → 링크·저장됨. Markdown 원문 → 안내·저장 안 보냄·초안 남음 → 새로고침·초안 복원 → 이미지 요청 없이 표시 → 원문을 링크로 고침 → 저장 200, 서버 본문에 링크                   |
| 메뉴 연 동안 다른 사용자 편집(브라우저)  | PASS | 실시간 켬, 블록 수식 메뉴를 연 채 test-user-2가 맨 앞 문단 추가·첫 문단에 글자 넣기 → 손잡이가 수식에 그대로 → 지우기 → 수식만 지워짐. 이미지 메뉴를 연 채 test-user-2가 그 이미지를 지움 → 메뉴 닫힘, 다른 블록 그대로. API 로그 오류 없음 |
| 끌기 취소 뒤 바깥 드롭(블록 수식·이미지) | PASS | 손잡이 dragstart → dragend(취소) → `view.dragging` 지워짐 → 다른 앱 글 드롭 → 드롭 위치에 글만 들어가고 수식·이미지는 그대로                                                                                                                |
| 손잡이 열기 방식                         | PASS | 마우스 누르기·끌기 → 메뉴 안 열림, 클릭 → 열림·메뉴에 초점, Enter → 열림, 터치는 뗄 때 열림                                                                                                                                                 |

2차 독립 검토(`4603d41..0490247`) 7건:

| 항목                                                                                                                                                   | 처리                                                                                                                                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. 같은 문단에 글자로 남은 `![a](javascript:x)` 뒤에 `![b](주소)`를 쓰면 앞 글자가 설명으로 빨려 들어감                                                | 고침. 설명에 `]`를 받지 않음, 시험 추가                                                                                                            |
| 2. 붙인 HTML을 늘 다시 써서 머리의 `<style>`(표 프로그램의 굵게)과 `<td>`만 있는 조각이 깨짐                                                           | 고침. `<template>`로 읽고 고칠 것이 없으면 받은 HTML 그대로. 시험과 브라우저(Chrome 형식 붙여넣기: 표·굵은 칸 유지, 칸 안 바깥 이미지는 링크·알림) |
| 3. 충돌 화면의 "내 것을 사본으로"가 검사 없이 노트를 만든 뒤 저장 400                                                                                  | 고침. 노트를 만들기 전에 같은 검사와 안내                                                                                                          |
| 4. 자동 링크·붙여넣기 규칙이 `ftp:`·`tel:` 등 서버가 받지 않는 링크를 만듦(전부터)                                                                     | 고침. 링크 허용을 서버 규칙(`isAllowedHref`)으로. 시험과 브라우저(`ftp://`는 글자, `https://`는 링크, 저장됨)                                      |
| 5. 실시간 편집은 바뀐 최상위 블록을 통째로 바꿔 넣어, 같은 목록·표 안의 다른 곳을 고쳐도 메뉴가 닫힘 / 한 순간 트랜잭션 둘이면 둘째 위치 대응을 빠뜨림 | 앞은 동작 유지(엉뚱한 블록을 조작하지 않고 닫음)하고 설명·시험 이름을 바로잡음. 뒤는 고침(위치 대응을 이어 붙이고 한 번만 씀)                      |
| 6. 원문 안내·저장 거부 안내가 내용을 고친 뒤에도 남음                                                                                                  | 고침. 원문 모드에 들어갈 때 다시 계산·나가면 지움, 고칠 것 없이 저장되면 안내를 지움. 브라우저 확인                                                |
| 7. 굵게 등으로 나뉜 한 링크를 여러 개로 셈                                                                                                             | 고침. 이어진 같은 주소는 한 번, 시험 추가                                                                                                          |

3차 독립 검토(`591da00`):

| 항목                                                                                                                       | 처리                                                                                                       |
| -------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| 1. 링크 허용을 서버 규칙으로 바꾼 뒤 주소가 없는 링크(서버가 빈 주소로 받음)가 든 문서를 열면 편집기가 멈춤                | 고침. 주소가 문자열일 때만 검사. 시험(전 코드에서 TypeError 재현)                                          |
| 2. 자동 저장이 막힌 뒤 내용을 서버 것과 같게 되돌려도 저장 거부 안내가 남음                                                | 고침. 내용 때문에 막은 안내만 지움(로그아웃 등 다른 안내는 남김), 충돌에서 서버 것을 고르면 지움. 브라우저 |
| 3. 원문을 들어올 때 그대로 되돌리면 문제가 남았는데 원문 안내가 사라지고, 중간에 고친 내용이 저장 대상에 남음(뒤는 전부터) | 고침. 그때는 편집기 내용으로 검사하고 알림                                                                 |
| 덧붙여: 덧붙인 트랜잭션의 위치 대응, 한 순간 사이에 고정 블록이 바뀌는 경우, 저장 거부 안내의 링크 수                      | 고침(`mapBlockStartThrough` 시험, 나뉜 링크는 한 번)                                                       |

4차 확인(`7993999`) 3건: 주소 없는 링크를 저장 전 검사가 막던 것(서버처럼 빈 주소로 봄, 시험), 내용을 되돌려도 "다시 시도" 등 다른 저장 안내가 남던 것(로그아웃 안내만 남김), 복원 제안 중인 초안을 지우던 것(고를 때까지 남김)을 고쳤다(`102bd33`). 브라우저에서 막힘 → 초안 남음 → 되돌림 → 안내 지워짐을 다시 확인.

### 77.1 합친 후보 확인 (main + PR #21 + PR #23, 로컬 브랜치 `rc/local-3`, push 안 함)

GitHub에서 새로 clone한 main `947b874`에 PR #21(`818b543`, 앞당겨 감기)과 이 브랜치(`d388e02`, PR #22 포함)를 합쳤다. 76.1과 같이 `docs/decisions.md`(76차와 77~79차)·`docs/development.md`(75절과 76~77절)만 충돌하고 양쪽을 다 남겨 풀었다.

| 단계                                                   | 결과 | 내용                                                                                                                                                                                                    |
| ------------------------------------------------------ | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 웹 lint·타입·prettier·vitest                           | PASS | lint 0, svelte-check 0, vitest 144건(`d388e02`)·150건(`591da00` 합친 뒤)                                                                                                                                |
| 제품 이미지 빌드·셀프호스트 실행(`--profile keycloak`) | PASS | bootstrap·migrate 종료 0, `/healthz` ok, 웹 200, 이미지 digest 고정(PostgreSQL `18.3-bookworm`), 내려주는 번들에 이 브랜치의 편집기 코드                                                                |
| 편집(브라우저, Keycloak 표준 OIDC)                     | PASS | 새 사용자 로그인 → 워크스페이스·문서 만들기 → `/체크`로 체크리스트, Enter로 다음 항목, Alt+Shift+↑로 옮기기, 손잡이 클릭 메뉴로 복제, 바깥 이미지가 든 HTML 붙여넣기 → 알림·링크로 저장                 |
| 초안·리비전 복원                                       | PASS | 저장 전 새로고침 → "초안 복원" → 저장(r8) → 기록에서 r7 복원 → r9(초안 문장 없음)                                                                                                                       |
| 백업 → 새 설치에 복구                                  | PASS | 표 47개·마이그레이션 56 백업 → 새 DB·검색 비밀 값의 새 설치(같은 Keycloak)에 복구 → 다시 로그인 → r9·리비전 9개·본문 그대로 → 편집·저장 r10                                                             |
| 2차 검토 수정(`591da00`) 합치고 올리기                 | PASS | 후보에 다시 합침(충돌 없음) → 복구한 설치에서 `up --build`(migrate 종료 0) → 다시 로그인 없이 그대로 → Chrome 형식 HTML 붙여넣기(나쁜 링크·바깥 이미지) → 알림·저장 r11, 손잡이 클릭 메뉴로 항목 지우기 |
| 3차 검토 수정(`7993999`) 합치고 올리기                 | PASS | 다시 합침(충돌 없음), vitest 153건 → `up --build` → 편집·저장(r13) → Markdown 원문에 바깥 이미지 → 저장 안 보냄·안내 → 링크로 고쳐 저장(r14)·안내 지워짐 → 새로고침 전 초안 → "초안 복원"·저장(r15)     |
| 4차 수정(`102bd33`) 합치고 올리기                      | PASS | 다시 합침, vitest 153건 → `up --build` → 주소 없는 링크가 든 문단 저장(r16) → 새로고침 뒤 열림 → 편집·저장(r17)                                                                                         |

브라우저 확인은 77절과 같이 최소화된 창의 DOM·스크립트 이벤트로 했다(글자 입력·키는 브라우저 입력으로 보냄). Keycloak 로그인 세션이 남아 있어 복구 뒤 옛 앱 세션이 401이 되는 것은 이번에 따로 보지 못했다(복구 결과 "everyone signs in again", 76.1에서 확인). 확인 뒤 두 설치의 컨테이너·볼륨, 후보 이미지, clone, 비밀 값·백업 사본을 지웠다.

## 78. 서체 넣기 (2026-10-18, D-86, `feat/fonts`)

사용자 승인(O-37 → D-86): Noto Sans KR 2.004·Noto Serif KR 2.003·D2Coding 1.4.0의 Regular·Bold 여섯 파일을 받은 그대로 제품에 넣는다.

- 파일·고지: `apps/web/src/assets/fonts`(서체 여섯, `LICENSE-Noto-CJK.txt`, `OFL-D2Coding.txt`). `cargo xtask notices`가 폴더 구성과 SHA-256을 확인하고 서체 고지를 `THIRD_PARTY_NOTICES.txt`에 싣는다. 폴더에 다른 파일을 두거나(운영체제의 숨김 파일 `.DS_Store` 등은 빼고) 서체를 한 바이트 바꾸면, 또는 실제 웹 번들에 승인하지 않은 서체가 들어가면(예: 다른 패키지 CSS의 KaTeX 서체) `cargo xtask check`(고지 단계)가 실패한다(셋 다 확인함).
- 불러오기: `@font-face`(swap, 미리 받기 없음). 토큰 `--font-sans`(Noto Sans KR), `--font-serif`(Noto Serif KR), `--font-code`(D2Coding, 코드·원문 입력·리비전 비교), `--font-mono`(시스템 고정폭, 짧은 UI 표시), `--font-doc`(문서 본문, 기본 고딕).
- 문서 본문 명조: 표시 설정의 "문서 본문 서체"(이 브라우저의 `localStorage`, 계정·API·문서 형식 변경 없음). 서체만 바꾸면 계정 설정을 쓰지 않고 바로 적용한다.
- 굵기: 승인한 파일이 400·700뿐이라 UI의 500(`font-medium`)은 Regular, 600(`font-semibold`·제목)은 Bold로 그린다(합성 굵기 없음). 그래서 첫 화면은 보통 Regular·Bold 두 파일을 받는다. 충돌 화면의 두 본문 비교는 코드가 아니라 문서 글이라 시스템 고정폭 그대로다.
- 전송: 이미지의 웹 빌드 단계가 `/assets`의 js·css·svg·otf·ttf(1 KB 넘는 것)에 br·gzip 사본을 만들고 서버가 받는 쪽에 맞춰 그대로 보낸다(통합 테스트 `assets_are_served_precompressed_when_available`).

| 검증                                | 결과    | 내용                                                                                                                                                                                             |
| ----------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 받은 파일                           | PASS    | 태그에서 받은 여섯 파일의 SHA-256이 O-37 기록과 같음                                                                                                                                             |
| 글자 범위(cmap)                     | PASS    | 한글 11,172자 모두, Noto KR 한자 8,138·히라가나 93·가타카나 96, D2Coding 한자 4,620. 일본어 신자체 일부(辺·変·読·様·黒·対·込)는 없음 → 그 글자만 시스템 일본어 서체                              |
| 요청(개발 서버, 브라우저)           | PASS    | 코드 없는 문서: Noto Sans KR 400·700만 같은 출처에서 받음(CDN·조각 요청 없음). 인라인 코드를 넣으면 D2Coding 400만 더 받음. 명조를 고르면 Noto Serif KR 400만 더 받음(굵은 글씨가 있을 때만 700) |
| 문서 본문 서체 설정                 | PASS    | 표시 설정에서 명조 저장 → 새로고침 뒤 문서 본문만 명조, UI는 고딕. 단위 시험(`doc-font.test.ts`). 서체만 바꾸면 계정 API 호출·새로고침 없음                                                      |
| 고지·구성 확인                      | PASS    | 다른 파일 추가·서체 변경·번들에 KaTeX 서체 유입 시 `cargo xtask notices --check` 실패, 되돌리면 통과                                                                                             |
| `cargo xtask check --skip-licenses` | PASS    | 모든 단계(vitest 154건)                                                                                                                                                                          |
| 압축 사본 통합 시험                 | PASS    | `assets_are_served_precompressed_when_available`(CI integration)                                                                                                                                 |
| 실제 화면의 글자 모양               | NOT RUN | 브라우저 창이 최소화돼 화면 캡처를 못 함. 요청·적용 서체는 DOM(`document.fonts`, 계산된 font-family)으로만 확인                                                                                  |

## 79. 전체 무결성 점검과 공동 편집 실사용 확인 (2026-10-19, `fix/audit-integrity`)

사용자 지시: 저장소 전체를 화면부터 DB까지 따라가며 권한 누수, 저장·ACK·ETag·초안·실시간·권한 회수, 정본 데이터, 달력·시간대, 파일 수명·백업, 내보내기·미리보기·색인, 화면 상태·i18n, 개발·셀프호스트 설정 분리를 점검하고 데이터 손실·누수·실패부터 최소 수정한다. 같은 날 개발 설치본(§12.1)에서 두 브라우저 프로필로 공동 편집을 실제 서버·웹소켓·DB로 확인했다.

- 방법: 영역 다섯(권한·저장과 실시간·날짜와 시간대·파일과 휴지통·백업·내보내기·API 계약과 화면 상태)을 읽기 전용 검토자가 나눠 보고, 고친 항목은 모두 코드를 직접 다시 읽어 확인했다. 쓰는 사람은 한 명(파일 겹침 없음). 고친 결함마다 회귀 시험을 더하고, 고치기 전 코드(또는 0057 전 DB)에서 그 시험이 실패하는 것을 확인했다.
- 공동 편집 확인에서 나온 결함(1~3)이 먼저다. 화면에 글자가 보이는 것으로 판단하지 않고 `document_revisions`·`document_collab_states`와 새 세션의 API 읽기로 대조했다.

| #   | 결함                                                                                                                                                                                                                                              | 고침                                                                                                                                                                                 | 시험                                                           |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| 1   | 실시간 문서가 조용해져도 리비전이 남지 않음(실측 15분 넘게 리비전 2에 머묾). 혼자 편집하는 연결이 서버에서 아무것도 받지 못해 33초마다 다시 붙고, 다시 붙을 때의 동기화 답(Yjs는 지움 기록을 늘 담음)을 편집으로 쳐서 60초 조용함이 매번 처음부터 | 참여자 표시를 보낸 연결에도 돌려줌(참조 서버와 같음). 실제로 문서를 바꾼 변경만 편집으로 침                                                                                          | `reconnecting_without_changes_is_not_an_edit`                  |
| 2   | 연결이 닫히기 직전 100ms 안에 보낸 편집이 버려짐(나간 연결의 모아 둔 변경을 건너뜀)                                                                                                                                                               | 나가기·정리 전에 모아 둔 변경을 먼저 넣음                                                                                                                                            | 같은 시험(쓰고 곧바로 닫기)                                    |
| 3   | 작성자를 독자로 바꿔도 열린 연결은 "연결됨"인 채 그 사람의 입력을 조용히 버림. 멈춘 실시간 편집기도 계속 입력됨(`editable`을 처음에만 읽음)                                                                                                       | 다음 확인(30초)에 4403으로 닫고, 끊긴 사이 독자가 된 편집기가 다시 붙을 때(`write=1`)도 4403. 다시 연결하면 지금 권한으로 다시 엶(독자는 읽기 전용). 편집기가 `editable` 변화를 따름 | `live_editing_guards`, `turning_realtime_off_keeps_live_edits` |
| 4   | 다시 붙기를 서버가 업그레이드 전에 거절하면(세션 끝·권한 없음·실시간 꺼짐) 브라우저는 1006만 받아 끝없이 다시 붙음                                                                                                                                | 1006이면 문서를 다시 읽어 실시간이 꺼졌으면 일반 편집으로, 401·403·404면 멈춤                                                                                                        | 브라우저(아래)                                                 |
| 5   | 일반 편집 중 실시간이 켜지면 저장이 409로 끝나고 초안은 실시간 편집기가 보여 주지 않는 곳에만 남음                                                                                                                                                | 실시간이 켜졌는지 확인해 이유를 알리고 자동 저장을 멈추며 "내 노트로 보존"을 보임                                                                                                    | 브라우저(아래)                                                 |
| 6   | 휴지통에 넣은 모듈의 문서를 공유받은 사람과 공개 링크는 계속 읽고 고침(0053이 `app_item_access`만 막음)                                                                                                                                           | 0057: `app_document_base_access`·`app_public_document`가 모듈 휴지통도 봄(항목 휴지통과 같음)                                                                                        | `module_trash_hides_its_items_and_restores_only_the_module`    |
| 7   | 문서를 다른 워크스페이스·개인↔워크스페이스로 옮겨도 첨부는 옛 자리에 남아 옛 관리자가 휴지통 묶음을 보고 되살리거나 영구 삭제하고 감사 기록을 읽음. 저장 용량·내보내기·색인도 옛 자리                                                             | 0057: 첨부(휴지통 것 포함)·열린 파일 휴지통 묶음·열린 업로드를 문서와 함께 옮기고 파일 색인을 다시 만듦. 0058: 업로드 마치기와 잠금 순서를 맞춤(교착 방지)                           | `moving_a_document_takes_its_attachments_along`                |
| 8   | 기록을 고치면 영구 삭제된 파일의 연결(이름·버전·SHA-256, D-35)이 사라지고, 휴지통에 있는 파일을 연결한 기록은 고칠 수 없음(400)                                                                                                                   | 0057: 남은 연결은 그대로, 이미 연결된 파일·버전을 다시 보내면 그 연결을 그대로 씀. 새로 휴지통 파일을 연결하는 것은 여전히 거절                                                      | `analysis_records_pin_file_versions_and_follow_item_access`    |
| 9   | 홈이 모든 알림(기한·만료·댓글 등)을 "나를 언급했습니다"로 보임                                                                                                                                                                                    | 알림 화면과 같은 종류별 문장(공용 함수)                                                                                                                                              | `notification-text.test.ts`                                    |

공동 편집 실사용 확인(개발 설치본, `cargo xtask dev --oidc`, 내장 브라우저 = test-user, 별도 프로필 헤드리스 Chrome = test-user-2):

| 확인                                     | 결과    | 내용                                                                                                                                                                                                      |
| ---------------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 동시 입력·삭제 수렴                      | PASS    | 양쪽 동시 입력·동시 삭제 뒤 두 화면이 같음                                                                                                                                                                |
| 원격 커서·참여자                         | PASS    | 서로의 커서와 이름표가 보임(참여자 목록 화면은 따로 없음)                                                                                                                                                 |
| 체크리스트 Enter·블록 이동·@ 참조        | PASS    | 상대에게 그대로 전달                                                                                                                                                                                      |
| 끊김·재연결 수렴                         | PASS    | 한쪽 웹소켓만 실제로 끊고 양쪽이 고친 뒤 다시 붙으면 두 문서 JSON의 SHA-256이 같음                                                                                                                        |
| 저장·새로고침·서버 재시작 뒤 유지        | PASS    | 리비전(조용함 저장) 확인 → 양쪽 API 다시 읽기 → 서버 재시작 → test-user-2는 쿠키를 지우고 Keycloak으로 새로 로그인해 같은 리비전·내용을 읽음. 고친 판에서 혼자 편집해도 연결 하나로 60초 뒤 리비전이 남음 |
| 다시 실행해도 중복 없음                  | PASS    | 두 번 다시 실행: 사용자·워크스페이스·문서·멤버 수와 Keycloak 사용자 id가 같음, 준비 출력의 워크스페이스·문서 id도 같음                                                                                    |
| 권한 회수(작성자 → 독자, 연결 중)        | PASS    | 서버는 그 뒤 입력을 받지 않음(실시간 상태·리비전·본문에 없음). 고친 판: 30초 안에 안내와 함께 읽기 전용, 다시 연결하면 읽기 전용 화면. 구성원 제거(4403)는 통합 시험으로만 확인                           |
| 실시간 끄기(한쪽이 끊긴 동안)            | PASS    | 끄는 순간 다른 쪽의 실시간 편집이 리비전으로 남음. 끊겼던 쪽은 다시 붙으려다 일반 편집으로 바뀌고 다시 붙기가 멈춤                                                                                        |
| 실시간 켜기(일반 편집에 저장 안 한 변경) | PASS    | 자동 저장 전에 켬 → 안내와 "내 노트로 보존" → 누르면 그 내용이 개인 노트로 저장됨                                                                                                                         |
| 방이 닫힐 때 리비전                      | 시험만  | 브라우저에서는 Vite 프록시 뒤 연결이 늦게 닫혀 조용함 저장으로 남는 것만 봄. 닫힐 때 저장은 통합 시험으로 확인                                                                                            |
| 한국어 IME·실제 휴대전화·화면 낭독기     | NOT RUN | 이 환경에서 할 수 없음                                                                                                                                                                                    |

남은 위험(이번에 고치지 않음):

- 끊긴 동안 친 실시간 편집은 그사이 실시간이 꺼지면 남지 않는다(다시 붙지 못해 서버에 간 적이 없음). 화면은 일반 편집으로 바뀐 뒤 서버 내용을 보인다.
- 권한을 잃은 뒤 다음 확인(30초)까지 보낸 편집은 들어간다(확인 간격은 정책 질문).
- 기한·만료·복습 알림의 "오늘"을 서버(UTC) 날짜로 정한다. 받는 사람의 시간대로 정하려면 두 알림 함수와 중복 방지 키를 다시 짜야 해 따로 한다. 문장에는 날짜가 함께 나온다.
- 같은 요청 id로 내용을 바꿔 다시 보내면(응답을 잃은 뒤 고쳐서 다시 저장) 서버는 처음 것을 돌려준다(댓글·일정·기록·시간·홈·스터디 입력). 클라이언트에서 id를 바꿀지 서버가 거절할지는 정책 질문.
- 휴지통의 "만료 비우기"는 참조 때문에 남겨 둔(D-69) 만료 묶음도 지운다(정책 질문).
- 실시간 편집 중 아직 리비전이 되지 않은 본문의 이미지 버전은 버전 삭제를 막지 않는다(그 사이에 지우면 리비전에서 404).
- 문서를 옮기면 첨부의 저장 용량이 새 워크스페이스 몫이 되지만 옮길 때 그 한도를 확인하지 않는다(넘으면 그 뒤 업로드가 실패). 한도로 막을지는 정책 질문.
- 화면 상태: 몇 화면이 불러오는 중·실패에도 "비었음"을 보이고 다시 시도 단추가 사이드바에만 있다. 시간 화면의 "기록 없음"은 보이지 않는다. 413(`too_large`)은 이유 없이 "저장하지 못했습니다"만 보인다. 첫 화면 실패는 브라우저 원문을 보인다.
- 업무 상세를 다시 읽으면(`fill`) 저장하지 않은 입력을 덮는다. 문서 초안은 사용자별로 나뉘지 않고 로그아웃해도 지워지지 않는다(같은 브라우저를 함께 쓰는 경우).
- 수정 화면이 없는 서버 기능: 강의 회차·스터디 단위 고치기, 업무 상위 바꾸기.
- 날짜·시간: 시간 기록 직접 입력이 여름시간에 없는 시각을 확인하지 않음, `describeWhen`이 12·24시간 설정을 따르지 않음, 시간대 확인이 끝점마다 다름. 반복 `count`가 여름시간으로 건너뛴 회차를 셈(정책 미정).
- 주석·문서: `trash.rs` 머리의 "자동 삭제는 O-19 뒤"와 위 §파일의 "D-56 보호 해당 없음"은 지난 상태다(0021·0036 이후 구현됨). 0027 머리 주석은 적용된 마이그레이션이라 고치지 않는다.

## 80. 제품 로직 집중 점검 (2026-10-19, `fix/product-logic`)

사용자 지시: §79 점검을 데이터 보존 → 사용자 격리 → 수명주기 → 상태 전이 순으로 이어 간다. 정상 경로뿐 아니라 동시 요청·응답 유실·연결 단절·재시작·사용자 전환을 본다. 먼저 지켜야 할 것을 정하고 코드·DB·브라우저 결과와 대조한다. 정책 미정과 구현상 사실을 나눈다.

지켜야 할 것(불변식)과 점검 시작 때의 사실:

| #   | 불변식                                                                         | 시작 때 사실                                                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | 입력한 내용은 저장되기 전에 사라지지 않는다                                    | 일반 편집기는 초안·나가기 경고가 있다. 실시간 편집기는 없다: 끊긴 동안의 입력은 실시간 꺼짐·권한 변경·다시 열기·새로고침에서 사라진다                                                                            |
| 2   | 같은 저장 요청을 다시 보내면 같은 내용은 한 번만, 바뀐 내용은 바뀐 대로 남는다 | 문서 저장·모듈·만들기 폼·체크리스트는 내용이 바뀌면 새 요청 id. 댓글·일정·기록·직접 시간·홈 빠른 추가·학습 단위·강의 회차·항목·문서 복제는 실패 뒤 고쳐도 같은 id → 서버가 처음 것을 돌려줘 고친 내용이 사라진다 |
| 3   | 한 사용자의 초안은 같은 브라우저의 다른 사용자에게 보이지 않는다               | 초안 키가 문서 id뿐이라 다른 사람이 같은 문서를 열면 앞사람의 초안을 제안받는다                                                                                                                                  |
| 4   | 권한을 잃은 뒤의 쓰기는 들어가지 않는다                                        | §79에서 고침(4403, 재연결 `write=1`). 다음 확인까지 최대 30초는 정책 질문                                                                                                                                        |
| 5   | 참조 중인 파일 버전은 지워지지 않는다                                          | 리비전·기록의 참조는 지킨다. 아직 리비전이 되지 않은 실시간 본문의 참조는 지키지 않는다                                                                                                                          |
| 6   | 휴지통 묶음은 보존 기간 안(또는 참조로 보존되는 동안) 되살릴 수 있다           | 주기 작업은 지킨다. 관리자의 "만료 비우기"는 참조로 보존된 만료 묶음도 지운다(정책 질문)                                                                                                                         |
| 7   | 정상 종료·재시작 뒤 저장된 내용과 실시간 상태가 유지된다                       | 서버가 SIGTERM을 받지 않아 컨테이너 정지 때 10초 뒤 강제 종료(실시간 방의 리비전 남기기·작업 정리를 건너뜀)                                                                                                      |
| 8   | 동시 수정은 다른 사람(다른 탭)의 변경을 조용히 덮지 않는다                     | 업무·일정·시간 기록은 판 번호로 409. 강의 회차·학습 단위 진도는 상태를 바꿀 때 화면의 옛 메모·위치를 함께 보내 다른 탭의 변경을 덮는다                                                                           |

고친 것(모두 고치기 전 실패·고친 뒤 통과를 확인한 회귀 시험이 있다):

| 불변식 | 고침                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | 근거(시험)                                                                                         |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1      | 실시간 편집기: 서버가 DB에 저장했다는 확인(메시지 100)에 내 편집이 들어 있을 때까지 이 사용자의 초안으로 남긴다(끊김·4409·4400·실시간 꺼짐·권한 변경·새로고침). 다시 열면 실시간 내용과 다른 초안을 "내 노트로 보존/버리기"로 고르게 한다(실시간 문서에 섞지 않음). 남은 초안은 사용자별 줄에 쌓아 하나씩 고르게 하고(덮거나 버리지 않음, 저장소가 가득 차 옮기지 못하면 원본을 지우지 않음), 고르는 동안의 새 편집은 따로 남는다. 일반 편집으로 바뀌면 줄의 초안을 제자리에 둔 채 복원·버리기를 고르게 하고 기존 충돌 화면이 이어 받는다 | `live-unsynced.test.ts`(다시 붙자마자 거절), `drafts.test.ts`, 브라우저 흐름 1·2·4·5               |
| 1      | 업무 상세: 체크리스트·반복·선행 업무를 바꾼 뒤 다시 채울 때 저장하지 않은 칸을 지킨다. 같은 칸을 양쪽에서 바꿨으면(태그·예상 시간 포함, 여러 번 다시 채워도) 옛 판 번호를 남겨 저장 때 충돌 화면                                                                                                                                                                                                                                                                                                                                          | `task-form-merge.test.ts`                                                                          |
| 2      | 만들기 요청 11곳: 같은 내용의 재시도는 같은 요청 id, 고친 내용은 새 id(공용 `RetryKey`). "내 노트로 보존"의 재시도도 같은 노트                                                                                                                                                                                                                                                                                                                                                                                                            | `retry-key.test.ts`, `comment-retry.test.ts`                                                       |
| 3      | 초안 키에 사용자 id. 로그인 전에는 두지 않고, 사용자 없는 옛 키는 제안하지 않고 지운다                                                                                                                                                                                                                                                                                                                                                                                                                                                    | `drafts.test.ts`, 브라우저 흐름 3                                                                  |
| 5      | 아직 리비전이 되지 않은 실시간 본문의 파일 참조도 붙잡는다(0059). 낡은 실시간 상태(복원 등)의 참조는 풀고, 휴지통 문서의 것은 리비전과 같게 본다(0060, 휴지통에 넣은 뒤의 마지막 저장도 0061)                                                                                                                                                                                                                                                                                                                                             | `live_body_pins_the_file_versions_it_points_to`, `trashed_document_keeps_live_file_pins`           |
| 7      | SIGTERM도 정상 종료 절차(실시간 방 리비전, 작업·DB 정리). 셀프호스트 앱에 `stop_grace_period: 30s`                                                                                                                                                                                                                                                                                                                                                                                                                                        | `server_shuts_down_cleanly_on_sigterm`, 개발 서버 정지 로그(`server.stopping` → `server.shutdown`) |
| 8      | 진도: 바꾸기 직전에 서버의 지금 내 진도를 읽어 그 동작이 바꾸는 칸만 바꾼다. 타이머: 쓰기는 더 새 쓰기가 없을 때, 읽기는 쓰기 도중이 아니었고 더 새 요청이 없을 때만 반영                                                                                                                                                                                                                                                                                                                                                                 | `progress-fresh.test.ts`, `timer-stale.test.ts`                                                    |

브라우저 흐름(개발 설치본, 내장 브라우저 = test-user, 별도 프로필 헤드리스 Chrome):

| 흐름                                          | 결과 | 내용                                                                                                                                               |
| --------------------------------------------- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. 끊긴 채 입력 → 새로고침(실시간 켬)         | PASS | 초안이 사용자·문서 키로 남고, 다시 연 뒤 실시간 문서에는 섞이지 않은 채 제안. "내 노트로 보존" → 그 글이 든 개인 노트, 초안 지워짐                 |
| 2. 끊긴 채 입력 → 실시간 끔                   | PASS | 일반 편집으로 바뀌며 초안 복원 제안 → 복원하면 저장(리비전 7). 전에는 이 입력이 사라졌다                                                           |
| 3. 같은 브라우저에서 사용자 바꾸기            | PASS | B의 저장되지 않은 초안이 남은 채 A로 로그인 → A에게 제안되지 않고 B의 글이 보이지 않음. B의 초안은 B의 키로 남음                                   |
| 4. 끊긴 채 입력 → 그사이 옛 리비전으로 되돌림 | PASS | 다시 붙으면 서버가 새 방에 섞지 않고, 그 입력은 고를 초안으로 제안됨(사라지지 않음)                                                                |
| 5. 끊긴 채 입력 두 번(새로고침 사이)          | PASS | 두 초안이 순서대로 줄에 남아 하나씩 제안됨. 일반 편집에서 고르지 않고 입력·자동 저장해도 줄의 초안은 남고, 복원하면 편집기로 들어온 뒤 줄에서 빠짐 |
| 정상 종료                                     | PASS | `docker compose stop` 신호(SIGTERM)로 개발 서버가 `server.stopping` → `server.shutdown`                                                            |

정책·설계로 남긴 것(구현상 사실과 나눔):

- 권한을 잃은 뒤 다음 확인(최대 30초)까지 보낸 편집은 들어간다(확인 간격은 정책 질문).
- 같은 요청 id로 다른 내용이 오면 서버는 처음 것을 돌려준다. 클라이언트가 내용이 바뀌면 새 id를 쓰도록 고쳤고, 서버가 거절할지는 정책 질문.
- 진도는 정확히 같은 순간의 두 쓰기면 나중 것이 이긴다(판 번호 확인은 API 변경). 칸반 옮기기도 판 번호 없이 바로 반영(설계). 직접 입력한 시간 기록은 겹쳐도 받는다, 보는 사람도 자기 진도를 쓴다, 반복 업무의 다음 회차는 완료가 아니라 날짜로 생긴다(설계 사실).
- 관리자의 "만료 비우기"는 참조로 보존된 만료 묶음도 지운다, 문서를 옮길 때 대상 저장 한도를 확인하지 않는다(정책 질문).
- 저장 확인을 받기 전에 탭을 닫으면, 그사이 다른 사람이 고쳤다면 다음에 열 때 초안을 제안받을 수 있다(사라지지 않고 제안될 뿐).
- 같은 브라우저를 함께 쓰면 다른 사용자의 초안이 저장소(개발자 도구)에는 남는다. 로그아웃해도 지우지 않는 것은 보존을 위한 선택.
- 받는 사람 시간대의 알림 날짜(§79)는 따로 한다.

병합 전 마무리(같은 날):

- 저장 확인(추가 검토 ①): 연결이 2초 이어진 것을 저장으로 보지 않는다. 방은 상태를 DB에 저장한 뒤에만 저장 확인(y-sync
  사용자 정의 메시지 100, 저장한 상태의 스냅숏)을 보내고, 새로 붙는 연결에도 지난 확인을 보낸다. 클라이언트는 자기
  변경이 그 스냅숏에 들어 있을 때만(`Y.snapshotContainsUpdate`, 넣기와 지우기 모두) 복구용 초안을 지운다. 시험:
  확인 없이는 아무리 기다려도 확인되지 않음(예전 방식은 2초 뒤 지움), 저장 전에는 확인이 오지 않고 저장 뒤의 확인이
  편집을 담음, 붙자마자 지난 확인을 받음(`rooms_acknowledge_persisted_state_with_a_snapshot`).
- 탭마다 초안(②): 지금 편집의 초안은 탭마다 따로 둔다(탭 id는 sessionStorage, 새로고침해도 같음). 닫힌 탭의 초안만
  공용 줄로 넘겨받는다(살아 있는 탭은 Web Locks 잠금으로 판단, Web Locks가 없으면 넘겨받지 않음). 시험: 둘째 탭의
  저장·지움이 첫 탭 초안을 지우던 것을 재현 → 고친 뒤 남음, 닫힌 탭의 것만 넘겨받음.
- 줄로 옮기지 못한 초안(④): 저장소가 가득 차 옮기지 못하면 그 초안을 고르게 하고, 고를 때까지 이 탭의 새 편집이 그
  자리를 덮거나 지우지 않는다(그동안 끊긴 새 편집은 이 브라우저에 남지 않는다; 연결된 편집은 서버로 간다).
- 같은 초안인지(⑤): 제목과 본문 모두로 본다(본문만 같고 제목이 다른 초안도 고르게 한다).
- 실시간 파일 참조의 잠금(③, 0062): 리비전 참조처럼 가리키는 버전 행을 공유 잠금한다. 상태 저장 트랜잭션을 열어 둔
  채 다른 연결에서 이전 버전 정리를 돌리면 그 버전이 지워지던 것을 재현(1판 삭제) → 고친 뒤 건너뛰고, 저장이 끝난
  뒤의 정리도 참조를 보고 남김(`live_ref_store_and_old_version_cleanup_do_not_race`).
- 검증 범위: 위 "사라지지 않는다"는 실제로 확인한 경우에 한한다. 브라우저로 확인(저장 확인 방식으로 바꾼 뒤 다시):
  연결된 채 입력 → 저장 확인 뒤 초안 지움, 끊긴 채 입력 후 새로고침(두 번, 줄에 순서대로)·옛 리비전 복원(다시 붙을 때
  서버가 받지 않음)·실시간 끔, 일반 편집에서 고르지 않고 입력·자동 저장, 같은 브라우저 사용자 바꾸기, SIGTERM 정상
  종료. 시험으로만 확인: 4400으로 닫힐 때와 권한이 바뀌어 다시 붙을 때, 같은 사용자의 여러 탭, 저장소 가득 참, 진도·
  타이머·업무 상세·요청 id. 그 밖의 경우(다른 브라우저 엔진, 같은 탭 복제로 탭 id가 같아진 경우 등)는 확인하지 않았다.
- 백업 시험 격리: 원본 설치도 복구 대상처럼 자기 DB와 자기 저장소를 쓴다. 전에는 공유 테스트 DB에 다른 시험이 다른
  저장소에 쓴 파일 버전이 있으면 실패했고(파일 시험 → 백업 시험 순서로 재현, `NotFound`), 백업 시험이 먼저 돌아서만
  통과했다. 고친 뒤 같은 순서로 통과.
- 실시간 본문에 이미지 주소를 넣은 직후(주기적 상태 저장 3초 전) 그 버전을 지우는 경합: 재현됨(0.3초 뒤 삭제가
  204). 버전 삭제가 열린 방을 먼저 저장하게 하고(참조가 DB에 남음), 같은 프로세스의 주기 작업(이전 버전 정리·휴지통
  만료)도 매번 먼저 저장하게 했다(`deleting_a_version_right_after_a_live_reference_is_refused`, 고친 뒤 409). 버전 삭제는
  호출한 사람이 볼 수 있는 파일의 워크스페이스 방만 동시에(합쳐 최대 2초) 저장하게 한다. 남는 틈:
  저장과 삭제 트랜잭션 사이 수 밀리초에 들어온 편집, 주기 작업만 따로 도는 프로세스(그 프로세스에는 방이 없음).

정책 승인 전 바꾸지 않은 세 가지(지금 동작과 영향):

| 항목                                | 지금 동작                                                                                                                                                                                                         | 영향                                                                                                                  |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| 동시 진도 저장                      | 강의 회차·학습 단위 진도는 판 번호 없이 저장한다. 화면은 쓰기 직전 서버 값을 읽어 그 동작이 바꾸는 칸만 바꾸지만, 두 탭·기기가 거의 같은 순간에 쓰면 나중 쓰기가 앞의 메모·위치·상태를 덮는다                     | 개인 진도의 마지막 변경 하나가 조용히 사라질 수 있다(다른 사람의 자료는 아님). 막으려면 판 번호 확인(API 변경)이 필요 |
| 참조로 보존된 자료의 수동 영구 삭제 | 주기 작업의 휴지통 만료는 기록·문서(실시간 본문 포함)가 가리키는 파일 버전이 든 묶음을 남긴다(D-56·D-69). 관리자가 그 묶음을 직접 영구 삭제하거나 "만료 비우기"를 누르면 참조와 상관없이 지운다(파일 내용까지)    | 기록에는 이름·버전·SHA-256 연결만 남고(D-35), 문서 본문의 그 이미지는 404가 된다. 되돌릴 수 없다                      |
| 권한 철회 지연                      | 실시간 연결은 `FVOCI_COLLAB_RECHECK_SECS`(기본 30초)마다 권한을 다시 본다. 그 사이 쓰기 권한을 잃은 사람의 실시간 편집은 들어가고, 다음 확인에서 연결이 닫힌다(4403). 저장 API·다시 붙기는 요청마다 바로 확인한다 | 권한을 뺀 뒤 최대 30초 동안 그 사람의 실시간 편집이 문서에 남을 수 있다                                               |

## 81. 감사 후보 16건과 별도 검토 지적 대조 (2026-10-19, `fix/product-logic`)

사용자 지시: cf8508b 기준 감사 후보 16건을 최신 코드와 대조한다. 확정 결함으로 보지 않고 기존 해결·정책과 나눈 뒤
재현되는 것부터 고친다(파일 무결성·내보내기·잘못된 대상·충돌 우회 → 삭제·복원 → 인증 → 개인정보). 각 수정은 최소
변경과 고치기 전 실패·고친 뒤 통과 시험으로 보인다. 적용한 마이그레이션은 고치지 않고 새 마이그레이션(0063–0065)으로.

| #   | 판정              | 고침과 근거(시험)                                                                                                                                                                                                                                                            |
| --- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | 재현(좁음)        | 이어 올리기가 이름·크기·수정 시각만 보고 서버가 받은 조각을 건너뛰어, 같은 메타데이터의 다른 내용이면 옛·새 조각이 섞인 파일이 됐다. 브라우저가 보낸 조각 해시를 남기고 지금 내용과 같을 때만 건너뛴다(나머지는 다시 보냄). `upload.test.ts`                                 |
| 2   | 재현              | 계속 편집하는 실시간 문서는 쉬는 시간 리비전이 없어 워크스페이스 반출이 옛 본문을 담았다. 반출 직전에 그 문서들의 열린 방을 리비전으로 남긴다(최대 5초, 못 남기면 마지막 리비전). `workspace_export_includes_live_edits`                                                     |
| 3   | 재현              | 모듈 설정 저장 중 다른 모듈로 옮기면 남은 요청이 새 모듈로 가고 옛 결과가 새 화면에 들어갔다. 시작한 모듈과 입력을 붙잡는다. `module-settings-race.test.ts`                                                                                                                  |
| 4   | 재현              | 반복 규칙을 저장하면 일정 입력의 기준 판이 새 판으로 바뀌어, 그 사이 다른 사람의 변경을 충돌 없이 덮었다. 규칙 저장 하나만큼 오른 경우에만 기준을 따라간다. `event-rule-base.test.ts`                                                                                        |
| 5   | 재현              | 휴지통에 든 모듈이 있는 워크스페이스는 영구 삭제가 외래 키 위반으로 실패했다(보존 기간이 지나도 지우지 못함). 모듈을 지운 뒤 묶음을 지운다(0063). `purge_succeeds_with_a_trashed_module`                                                                                     |
| 6   | 재현(동시)        | 하위 업무를 지우는 동안 상위 업무를 지우면 하위 업무가 상위의 묶음으로 옮겨졌다. 잠근 뒤 다시 고른다(0063). `trashing_a_parent_while_its_subtask_is_being_trashed_keeps_the_subtask_batch`                                                                                   |
| 7   | 재현(복제)        | 직접 만들기는 다시 읽기에서 막혔지만, 복제는 휴지통에 든 모듈에 항목을 만들었다. 함수가 휴지통 모듈을 없는 것으로 본다(0063). `no_items_in_a_trashed_module`                                                                                                                 |
| 8   | 정책(데이터 모델) | 아래 표                                                                                                                                                                                                                                                                      |
| 9   | 재현(좁음)        | 체크리스트 붙여넣기 미리보기에서 응답을 잃은 뒤 줄을 고쳐 다시 보내면 같은 요청 id라 고친 내용이 버려졌다. 줄마다 `RetryKey`. `ChecklistForm.test.ts`                                                                                                                        |
| 10  | 재현              | 복구가 목록의 표만 믿어, 표가 빠진 백업도 그 내용 없이 복구를 끝냈다. 스키마가 백업하는 표마다 덤프가 있어야 한다. `backup_restores_documents_tasks_and_files_into_a_fresh_install`                                                                                          |
| 11  | 잠재(설정)        | 설정은 업로드 조각 × 10000까지의 파일을 허락하는데 복구는 늘 8 MiB 조각이라 약 78 GiB를 넘는 파일은 다시 올리지 못했다. 파일 크기에 맞춰 조각을 키운다. 단위 시험 `restore_parts_stay_within_the_storage_limit`(그 크기의 실제 복구는 하지 않음)                             |
| 12  | 재현              | 디스커버리·키 목록을 프로세스 동안 캐시해 제공자가 서명 키를 바꾸면 다시 시작할 때까지 로그인이 실패했다. 검증에 실패하면 한 번 다시 받아 같은 토큰을 다시 검증한다. `oidc_login_survives_a_signing_key_rotation`(그 시험만의 realm에서 키를 바꿈)                           |
| 13  | 재현              | 항목 시간 합계가 공유하지 않은 멤버의 기록 종류를 값 0인 줄로 드러냈다(D-32). 내 기록과 공유한 멤버의 것만 모은다(0064). 시험이 옛 줄을 기대하고 있어 함께 고침. `item_totals_group_entries_under_the_item_by_kind`                                                          |
| 14  | 설계(고치지 않음) | 검색 작업이 5초 안에 끝나지 않으면 대기열에 든 것으로 보고 행을 지운다. Meilisearch는 받은 작업을 디스크에 두고 순서대로 처리하며, 그 뒤의 실패는 대부분 내용 때문이라 다시 보내도 같다. 다시 보내면 느린 대기열에 같은 작업이 쌓인다. 색인을 잃으면 다시 만들며 모두 올린다 |
| 15  | 일부 재현         | 로컬 저장소가 합치기·쓰기 도중 실패하면 반쯤 쓴 임시 파일(파일 크기만큼)이 `tmp/`에 남았다. 실패하면 지운다. `local_complete_failure_leaves_no_temp_file`. 나머지는 아래 한계                                                                                                |
| 16  | 재현              | 문서의 `openssl rand -base64`는 절반쯤 `/`가 섞이고, DB 비밀번호는 접속 URL에 그대로 들어가 서버가 시작하지 못했다(`database other`). 예시를 `-hex`로                                                                                                                        |

별도 컨텍스트 검토가 이 브랜치에서 짚은 것:

| #   | 판정       | 고침과 근거(시험)                                                                                                                                                                                                                             |
| --- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2   | 재현       | 방에 들어올 때 보낸 세 메시지(맞추기 1단계·참여자·저장 확인)를 한 프레임에 묶었는데 y-websocket은 프레임의 첫 메시지만 읽는다. 메시지마다 프레임. 시험 클라이언트도 프레임마다 한 메시지를 요구                                               |
| 3   | 재현(모의) | 고를 초안 줄이 키 하나의 목록이라 두 탭이 거의 동시에 읽고 쓰면 한쪽 초안이 사라졌다(localStorage는 탭 사이에 원자적이지 않음). 초안마다 키(더한 순서). 옛 목록은 읽을 때 옮긴다. `drafts.test.ts`                                            |
| 4   | 재현(모의) | 탭 복제는 sessionStorage까지 복사해 두 탭이 같은 초안 키를 덮었다. 페이지를 열 때 탭 id의 잠금을 잡고, 살아 있는 탭이 쥐고 있으면 새 id. `drafts.test.ts`                                                                                     |
| 5   | 재현 안 됨 | "읽기 사용자에게 초안이 쌓인다": 읽기 전용 실시간 편집기는 처음 붙을 때도, 원격 내용을 받은 뒤에도 Yjs 문서에 로컬 갱신을 만들지 않았다(jsdom 탐침). 그래서 초안을 쓰지 않는다. 실제 브라우저에서는 보지 않았다. 고치지 않음                  |
| 6   | 재현       | 실시간 편집 중 옛 리비전으로 복원하면 리비전이 되지 않은 편집이 이력 없이 사라졌다(D-27). 쓸 수 있는 사람의 복원은 먼저 방을 리비전으로 남긴다. 기준이 지나면 기존 충돌 안내. `restoring_during_live_editing_keeps_the_live_edits_in_history` |
| 7   | 재현       | 파일 버전 삭제가 볼 수만 있는 사람의 요청에도 워크스페이스의 열린 방을 저장시켰다(뒤에 403). 쓸 수 있을 때만. `a_reader_cannot_make_rooms_persist_through_version_delete`(주기 저장 3초 전에 확인)                                            |
| 8   | 재현       | 문서를 다른 워크스페이스로 옮겨도 열린 방은 옛 워크스페이스로 남아, 새 워크스페이스에서 실시간을 꺼도 다음 권한 확인까지 열려 있었다. 이동이 방의 워크스페이스를 바꾼다. `a_moved_room_follows_its_document_to_the_new_workspace`             |
| 9   | 재현       | 실시간 상태·리비전의 파일 참조를 넣은 뒤 버전을 잠가, 그 틈에 이전 버전 정리가 버전을 지울 수 있었다. 먼저 잠그고 넣는다(0065). 시험용 지연 트리거로 틈을 넓혀 0064 DB에서 재현. `file_refs_lock_versions_before_they_are_written`            |

정책·데이터 모델이라 바꾸지 않은 것(지금 동작과 영향):

| 항목                     | 지금 동작                                                                                                     | 영향                                                                                                                                                                                            |
| ------------------------ | ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 8. 영구 삭제한 모듈의 키 | 모듈을 휴지통에서 영구 삭제하면 그 모듈의 지금·과거 키 예약도 지운다(`module_keys`가 모듈을 외래 키로 가리킴) | D-18 "과거 키도 남아 다른 모듈이 다시 쓰지 못한다"와 어긋난다. 다른 모듈이 그 키를 쓰면 문서에 남은 `KEY-12` 같은 글자 참조가 새 모듈을 가리킬 수 있다. 예약을 남기려면 데이터 모델 변경이 필요 |

남은 한계:

- 15번 나머지: 저장소 업로드를 시작한 뒤 그 id를 DB에 적기 전에 실패하면 빈 업로드(로컬은 빈 폴더, S3는 조각 없는 여러
  조각 업로드)가 남는다. 저장소 정리는 10번 실패하면 경고를 남기고 멈춘다(그 파일 내용은 저장소에 남음).
- 1번: 이 브라우저가 보낸 해시만 안다. 다른 브라우저에서 같은 파일을 이어 올리지는 않으므로(요청 id가 브라우저에 있음)
  범위 밖이다. 해시를 적지 못하면(저장소 가득 참) 받은 조각을 모두 다시 보낸다.
- 2번: 공개 링크·검색·문서 읽기 API는 여전히 마지막 리비전을 본다(쉬는 시간 60초 또는 방이 닫힐 때 갱신).
- 검토 6번: 복원하는 사람이 그 방에서 고치고 있고(그 실시간 내용을 보고 있고), 화면이 보던 리비전 바로 다음이 이 복원이
  방금 실제로 쓴 리비전뿐일 때만 그대로 복원한다(검토 3차: 본문이 같아 쓰지 않은 경우·방 밖의 사람은 충돌, 시험
  `a_room_reports_only_a_revision_it_wrote`·`a_restore_from_outside_the_room_still_conflicts_with_live_edits`). 그 전에 쉬는 시간
  리비전이 이미 남았으면(화면은 그 번호를 모른다) 충돌 안내가 나오고 새로고침 뒤 다시 복원해야 한다(이번 변경 전과 같음).
  방을 리비전으로 남긴 뒤 방을 닫기까지의 아주 짧은 사이에 온 편집은 연결이 4409로 닫히며 그 사람의 초안으로 남는다.
- 검토 8번: 이동을 커밋한 뒤 방의 워크스페이스를 바꾸기까지의 짧은 사이에는 옛 워크스페이스로 보인다(권한 재확인이 닫는다).
- 검토 6번(3차 남은 것): "그 방에서 고치고 있는지"는 연결이 아니라 사람으로 본다. 같은 사람이 한 탭(또는 기기)에서 실시간으로
  고치는 중에 실시간이 아닌 다른 탭에서 복원하면, 그 탭이 보지 못한 다른 사람의 실시간 편집을 충돌 없이 되돌린다(그 편집은 이력에
  남는다). 연결 단위로 보려면 복원 요청에 연결 id를 더해야 해서(API 계약 변경) 승인 대상으로 둔다.
- 12번: 서명 검증 실패에서만 한 번 다시 받는다(nonce·기한·대상 실패는 바로 거부). 토큰은 유효한 로그인 상태와 코드 교환을
  거쳐 제공자에게서 바로 받은 것이라 밖에서 반복시킬 수 없다.

별도 검토(2차, 위 고침 전체)가 짚어 고친 것: 실시간 편집 중 복원이 화면에서 매번 충돌로 끝나던 것(위 6번 고침이 만든
퇴행, `restoring_during_live_editing_keeps_the_live_edits_in_history`가 화면처럼 보던 리비전으로 보내 고치기 전 409를
재현), 반출·복원이 방을 기다리는 동안 DB 연결을 쥐던 것, 옛 초안 줄을 옮기다 실패하면 중복되던 것과 순서(`drafts.test.ts`),
그만둔 업로드의 조각 해시가 남던 것(`upload.test.ts`), OIDC가 모든 검증 실패에서 다시 받던 것, 0065 시험이 정해 둔 시간에
기대던 것(쓰는 쪽이 실제로 멈출 때까지 기다리고, 0064 DB에서 다시 실패를 확인). 마이그레이션 0063–0065는 앞선 정의와 표시한
줄만 다름을 검토자가 따로 대조했다. 3차 검토는 위 복원 기준 고침의 구멍 둘(쓰지 않은 리비전을 알림, 방 밖 사람에게도
적용)을 짚었고 고쳤다(위 "남은 한계"의 검토 6번). 4차 검토는 남은 고침이 맞다고 보았고, 같은 사람의 다른 탭 한계를 남겼다.

검증(6d939ef, 그 뒤 커밋은 문서와 주석뿐):

- `cargo xtask check`: 모두 통과(라이선스 포함). `cargo xtask test --oidc`: 새 컨테이너에서 42개 묶음, 235건 통과.
- release 이미지: 새로 받은 사본(1d04af0)에서 `deploy/docker/app.Dockerfile`로 만들고, `docs/try-it.md` 순서대로 시험
  설치와 다른 프로젝트·포트(28080·28081)에 띄움. bootstrap·migrate 0 종료, 마이그레이션 65, 앱·마이그레이션 역할이 슈퍼유저·
  RLS 우회가 아님, `/healthz` ok, 웹 200. 헤드리스 Chrome으로 OIDC 로그인(사용자는 발급자+subject로 식별) → 워크스페이스 만들기
  → 위키 문서 작성 → 자동 저장(리비전 2) → 새로고침 → 로그아웃(Keycloak 세션도 끝남) → 다시 로그인 → 다시 열기 PASS. 다른
  사용자는 그 문서 읽기·고치기 404, 워크스페이스 목록 비어 있음. 스택 정지(`server.stopping` → `server.shutdown`) 뒤 다시 띄워도
  문서·리비전 유지. 감사 기록에 워크스페이스·문서 만들기.
- 개발 설치본 브라우저(1d04af0, 마이그레이션 65): 옛 한 키 목록이 초안마다의 키로 옮겨짐(옛 키 없음), 연결된 채 입력 → 저장
  확인 뒤 탭 초안 지워짐, 끊긴 채 입력 후 끊긴 채 새로고침 두 번 → 줄이 하나씩 늘고(4→5→6) 두 초안 모두 그 입력을 담음(서버
  리비전에는 없음), 방 안에서 입력 직후 화면이 아는 리비전으로 복원 → 200, 그 입력은 이력(리비전 15)에 남음.
- 확인하지 않음(NOT RUN): 탭 복제(같은 탭 id)와 두 탭 동시 줄 쓰기는 모의 시험으로만, 한국어 IME, 실제 휴대전화, 화면 낭독기.

## 82. 남은 세 항목: 실시간 복원·검색 작업·모듈 키 예약 (2026-10-19, `fix/product-logic`)

사용자 지시: 908a46d에서 남은 세 항목을 최소 변경으로 마무리한다. 경합은 sleep 대신 명시적 동기화·실패 주입으로, 검색은
제어 가능한 응답으로 재현한다. 적용된 마이그레이션은 고치지 않는다(0066·0067 추가).

**① 실시간 복원.** 사람이 방에 있다는 것만으로 기준 리비전을 올리던 것(1f0fbdb·5ebc51b)을 걷어 냈다. 실시간 방이 열려 있으면
방이 체크포인트(리비전으로 남기지 않은 편집 남기기) → 복원 → 방 초기화를 명령 하나로 한다. 방은 명령을 하나씩 처리하므로
그 사이 편집·참여가 끼어들지 않는다.

- 체크포인트 결과를 나눈다: 남길 것이 없음 → 화면이 보낸 기준으로 복원. 남김 → 그 리비전 때문에 기준이 지나 복원은 기존
  충돌(409, 지금 내용 포함). 실패(리비전으로 남길 수 있는 편집자가 없음 등)·시간 초과(5초) → 복원하지 않고 409(지금 내용 없음,
  "아무것도 복원하지 않음"). 편집은 방에 그대로 남는다.
- 요청 처리기는 방의 답을 시간 제한 없이 기다린다(여기서 그만두면 방이 복원을 마쳤는데 실패로 답할 수 있다). 방이 바쁘거나
  명령을 처리하기 전에 닫혔으면 복원하지 않았다고 답한다.
- 복원 명령 뒤에 들어와 있던 편집은 방과 함께 버려진다. 방이 받았다고 확인하지 않았으므로 클라이언트는 4409로 닫힐 때 그
  편집을 초안으로 남긴다. 그 사이 저장 API의 제목 저장은 복원 SQL의 기준 확인이 충돌로 막는다.
- 화면: 충돌이면 서버의 지금 문서를 비교 기준으로 보여 주고, 사람이 다시 누를 때만 그 리비전을 기준으로 보낸다. 저절로 다시
  보내지 않는다.
- 시험(고치기 전 실패 5건): 명시적 재시도, 체크포인트 실패(유일한 편집자를 읽기로 내림), 시간 초과(문서 행을 잠가 붙잡고
  `pg_blocking_pids`로 이 시험의 잠금을 기다리는 질의를 확인), 복원 중 편집(항목 행을 잠가 복원 SQL만 붙잡고, 다른 연결이 그
  편집을 본 뒤 저장을 시킴: 고치기 전에는 저장 확인을 받은 뒤 방 초기화로 사라졌다), 파일 버전 붙잡기. 화면은
  `revision-restore-conflict.test.ts`.
- 한계: 시간 초과가 체크포인트의 커밋 직후에 나면 리비전은 남았는데 복원은 하지 않는다(안전한 쪽). 방은 다음 체크포인트에서
  본문이 다른 길로 바뀐 것으로 보고 다시 열게 한다. 복원 중 편집 시험의 "고친 뒤" 쪽은 방이 그 편집을 받아 들이지 않음을 5초
  상한까지 기다려 확인한다.

**② 검색 작업(0067).** 대기열 행에 Meilisearch가 받은 작업 번호와 그때 보낸 세대(`queued_at`)를 남긴다. 성공을 확인한 뒤에만
지우고, 그 사이 더 새 변경이 오른 행은 남겨 앞 작업이 끝난 뒤 다시 보낸다(옛 작업의 성공이 새 변경을 지우지 않음). 기다리는
행은 다시 꺼내지 않고 실패 횟수도 쓰지 않는다. 실패·취소·없는 작업은 다시 보낸다. 확인 요청 자체가 실패하면 기다림으로 둔다.
시험은 작업 상태를 시험이 정하는 가짜 Meilisearch와 이 시험만의 DB(`fresh_database`를 공용 시험 모듈로 옮김, 준비 약 0.3–0.5초)로
대기 → 다시 꺼내도 안 보냄·실패 횟수 그대로 → 그사이 제목 변경 → 성공 뒤 새 세대를 다시 보내고 지움 → 실패면 다시 보냄을 본다.
Meilisearch 데이터를 지워 작업 번호가 처음부터 다시 매겨지면 색인을 새로 만들며 모두 다시 올라(`queued_at` 바뀜) 옛 번호의
성공이 새 세대를 지우지 않는다.

**③ 모듈 키 예약(0066, D-18).** 모듈을 휴지통에서 영구 삭제해도 그 모듈의 키 행(지금 키·과거 키)을 지우지 않고 주인만 비운다
(주인 없는 행은 늘 과거 키). 만들기는 원래 모든 키 행을 거절했고, 이름 바꾸기도 주인 없는 예약 키를 거절하게 했다. 글자 참조
`KEY-12`는 예약 키에서 아무것도 가리키지 않는다(새 모듈을 가리키지 않음). 워크스페이스 영구 삭제는 예약까지 지운다. 이
마이그레이션 전에 영구 삭제된 모듈의 키는 이미 지워져 되살리지 않았다. 시험: 영구 삭제 뒤 지금 키·과거 키로 만들기·이름
바꾸기가 409(고치기 전 201), 워크스페이스 영구 삭제 204.

별도 검토가 짚어 고친 것(모두 고치기 전 실패 확인):

- 여럿을 한 번에 보낸 색인 작업이 기다림 뒤에 실패하면 같은 묶음으로 다시 꺼내져 문제 있는 한 건이 모두를 계속 막았다. 앞서
  꺼낸 행은 하나씩 보낸다(꺼내기가 `attempts`를 돌려줌). 0067을 고쳤다: 밀어 올리거나 배포한 적이 없고 이 컴퓨터의 시험 DB에만
  적용됐다(시험 DB는 다시 만들었다).
- 체크포인트에서 DB 본문이 방이 아는 것과 다르면 방은 편집을 남기지 않고 닫는데 이를 성공으로 보고 복원했다. 보존이 확인되지
  않았으므로 복원하지 않는다(시험: 방 밖에서 새 리비전으로 본문을 바꾸고 그 리비전을 본 사람이 복원).
- 충돌 뒤 화면의 문서가 더 새 리비전이 되면 옛 충돌 응답 대신 그 문서를 기준으로 한다.
- 시험 보강: 권한 재확인 간격을 고정, 편집이 사라지지 않았음을 새로 붙은 연결로 확인, 검색 시험의 새 DB를 지우고 꺼내기 오류를
  드러냄, 다시 복원의 요청 id가 바뀜.

고치지 않고 남긴 것(근거):

- 복원 SQL이 잠금을 오래 기다리면 그동안 그 방의 편집·참여도 기다린다(체크포인트만 5초 상한). 상한을 두면 결정적으로 시험하기
  어려운 긴 시험이 필요하다. 복원은 커밋 전에만 실패하므로 결과가 모호해지지는 않는다.
- 닫히는 중인 방에 복원이 오면 바로 복원으로 넘기지 않고 "다시 하라"(409)로 답한다(드문 경합, 자료 영향 없음).
- 열린 방이 없을 때(바로 복원) 리비전으로 남지 못하고 남은 실시간 상태(방이 닫힐 때 남기지 못한 것)는 복원이 반영하지 않는다.
  이번 변경 전부터의 동작이다. D-27 보장은 방이 열려 있는 동안이다.

마지막 검토가 짚은 것: 하나씩 다시 보낼 때 건마다 최대 5초를 기다려, 큰 묶음이면 꺼낸 행의 표시 시간(2분)을 넘겨 다시 꺼내지고
실패 횟수가 늘 수 있었다. 하나씩 보낸 것은 기다리지 않고 작업 번호를 남겨 다음에 확인한다(시험: 느린 Meilisearch에서 두 건을 다시
보내는 데 고치기 전 10.6초 → 고친 뒤 곧바로, 건마다 다른 작업).

검증:

- f6265fc(최종 후보): `cargo xtask check` 모두 통과, `cargo xtask test --oidc` 새 컨테이너에서 42개 묶음 240건 통과.
- d33e145(그 뒤 검색 한 건): 영향받는 검색·작업 묶음과 `cargo xtask check`(vitest 189건) 다시 통과.
- release 이미지(새로 받은 사본 d33e145): 마이그레이션 65의 기존 설치 위에 올려 0066·0067 적용, `/healthz`·웹 정상, 올리기 전 문서
  그대로, 새 문서 작성 → 자동 저장 → 새로고침 → 로그아웃·다시 로그인 → 다시 열기 PASS, 다른 사용자 404, 정상 종료.
- 시험 준비 비용: 시험 스택 기동 약 3초, 역할 준비 약 17초(빌드 확인 포함), 검색 시험의 새 DB 약 0.3–0.5초. 바뀐 크레이트만
  다시 빌드하는 묶음 실행 약 45초.

## 83. 제품 경험: 칸반·위키 탐색, 모듈별 연결, 개발 예제 (2026-10-19, `feat/product-experience`, 시작 `eef3523`)

칸반은 Kaneo의 열·카드·빠른 추가·상세 패널, Focalboard의 보기 전환을, 위키는 Outline의 트리·목차·읽기 흐름을 참고했다. 외부
소스·스타일·아이콘은 가져오지 않았다.

- **칸반**: 카드를 닫으면 기록을 쌓지 않고 뒤로 간다. 항목별 보기·거르기·정렬을 이 탭에서 기억한다. 카드마다 "옮기기" 메뉴(끌기의
  키보드·메뉴 대안)와 열마다 "업무 추가". 실패하면 기존 규칙대로 되돌리고 알린다.
- **위키·문서**: 트리 이동·만들기 실패를 알리고 목록을 다시 읽는다. 넓은 화면에서 본문 옆 목차(h1–h3, 입력·공동 편집을 따름).
- **편집기**: 1px 구분선처럼 손잡이보다 낮은 블록은 손잡이를 블록 가운데에 맞추고, 보이는 손잡이 줄 안에서는 hover 대상을
  유지한다. 전에는 손잡이로 가는 길에 다음 빈 문단이 잡혀 구분선을 끌 수 없었다(누를 때 고정은 늦다).
- **모듈 연결**(DB→API→화면→저장→다시 읽기): 다섯 종류의 전용 칸·기록은 서로 섞이지 않았다(API와 SQL 둘 다 종류 검사, 기록
  종류 짝 검사는 API). 화면이 없던 기존 API를 이었다: 강의 회차 공통 정보(번호·제목·챕터·길이·계획일·링크·자료)와 자료 표시,
  스터디 공통 단위 고치기, 체크리스트 문구 고치기, 기존 업무의 상위 업무 바꾸기.
- **함께 고치는 행(0069)**: 회차·공통 단위는 판 번호(`version`)를 두고, 고치기는 읽은 판을 보내 서버가 그 판일 때만 바꾼다(확인과
  바꾸기가 한 UPDATE 문장, 다르면 409이고 아무것도 바뀌지 않음). 처음에는 화면이 저장 직전에 다시 읽어 비교했는데, 그 사이의 저장은
  막지 못해 서버 조건으로 바꿨다. 409이면 입력은 남기고 기준만 지금 판으로 바꿔 알리며, 다시 저장하면 바꾼다(보관 등 다른 409는 그
  오류로). 체크리스트 항목 고치기는 보낸 칸(문구 또는 완료)만 바꾼다: 문구를 고치면 화면에 남은 옛 완료 상태를, 완료를 바꾸면 옛
  문구를 함께 보내 다른 사람의 변경을 되돌리던 것. 기록의 수 칸은 브라우저가 수로 읽지 못한 입력("1e", 값은 빈 글자로 옴)을
  `badInput`으로 일부러 비운 칸과 구분해 저장을 막는다(전에는 빈 칸으로 빼고 보내 서버가 칸을 통째로 바꾸며 저장된 값이 지워졌다).
- **결함 수정**: 강의 모듈만 보관해도 강좌 조회가 편집 가능이라 하던 것(쓰기는 409). 같은 원인으로 모듈만 보관한 항목 화면 전체
  (정보·업무·일정·기록·첨부·문서)와 보관된 항목·모듈의 항목 파일 목록(`can_write`)도 쓸 수 있게 보이던 것. 스터디·기록의 숫자 칸이 저장 전에 멈추던 것
  (`trim`), 항목 일정 저장이 옆 달력에 새로고침 전까지 보이지 않던 것, 기록 종류를 처음부터 못 읽으면 기록 영역과 오류가 함께
  사라지던 것.
- **개발 예제**: `cargo xtask dev --oidc`가 기존 개발 준비 다음에 `deploy/postgres/dev-module-seed.sql`을 실행한다(위 개발 실행
  설명). 다른 세션이 `eef3523` 기준으로 준비·검증한 패치를 받아 별도 옵션 없이 이 경로에 넣었다.
- **판단으로 둔 것**: 스터디 계획 재배치·개인 진도는 보관된 항목에서도 기록할 수 있다(개인 기록, 기존 진도 쓰기와 같음). 취미 활동
  기록의 날짜는 달력에 넣지 않았다(D-80의 취미 날짜 출처는 항목 일정).

검증(`feat/product-experience`, 실제 브라우저는 개발 설치의 헤드리스 Chrome에서 DevTools 실제 입력, 두 번째 테스트 사용자):

- 웹 단위: 각 수정마다 고치기 전 실패·고친 뒤 통과(스터디 숫자, 달력 갱신, 기록 오류·숫자 칸, 회차·단위 고치기와 충돌, 체크리스트
  문구, 문서 목차, 트리 실패, 칸반 닫기·맥락·메뉴·빠른 추가). 통합: `courses`(보관 모듈)·`files`(보관된 항목·모듈의 파일) 묶음
  고치기 전 실패·고친 뒤 통과. 최종 코드(0fc09af와 같은 트리)에서 `cargo xtask check` 통과, 새 테스트 DB의
  `cargo xtask test --oidc` 42개 묶음 242건 통과.
- 재사용한 테스트 DB(`--keep`)에서 `chunked_upload_resumes_checks_integrity_and_keeps_versions`가 정리 수 1로 실패하고 영구
  삭제한 파일의 두 번째 버전 객체가 저장소에 남던 것은 이 브랜치의 회귀가 아니라 main부터 있던 저장소 정리 결함이다: 지울 목록을
  비워진 표로 분석한 뒤에는 `app_system_storage_claim(1)`이 함께 오른 행을 한 번에 모두 꺼내고 작업은 하나만 지웠다. 고침과
  회귀 시험, 시험 격리(S3 시험의 새 DB, 자기 객체 기준 확인)는 #30(`fix/storage-claim`, 마이그레이션 0068)에 있다. 이 브랜치에
  넣었던 잠금은 되돌렸다.
- 브라우저 PASS: 구분선 위·아래로 끌기와 메뉴 지우기(실시간·일반 편집, 저장·새로고침 뒤 유지, 이웃 문단 그대로), 문단·블록 수식
  끌기, 스터디 단위 추가(분량 30)·내 분량 12와 메모·공통 단위 고치기 후 새로고침, 강의 회차 링크·자료 고치기 후 새로고침, 보관된
  강의 모듈에서 고치기·추가 단추 없음, 보관된 프로젝트 모듈의 항목에서 읽기 전용 표시와 일정·기록·정보 고치기·칸반 빠른 추가·옮기기
  메뉴·올리기가 사라지고 해제하면 돌아옴, 체크리스트 문구(완료 여부 유지), 상위 업무 바꾸기·되돌리기(마우스·키보드), 종일 일정 추가
  직후 달력 표시, 기록 종류 조회에 403 한 번 주입 → 안내와 다시 시도, 연구 문헌 DOI·취미 활동 시간 고친 뒤 새로고침, 예제 seed
  재실행 전후 행 수 같음·사용자 수정 유지, 칸반 메뉴 이동·빠른 추가·오프라인 되돌림, 목차 이동.
- 브라우저 PASS(이어서, 서로 다른 두 사용자가 각자 로그인한 브라우저 두 개): 구분선을 한 사람이 위로 끌면 다른 사람 화면에 바로
  반영되고 반대로도 같으며 둘 다 새로고침 뒤 같은 순서(서버 실시간 방). 독자로 바꾼 사용자는 프로젝트·스터디·강의 항목에서 편집
  단추가 없고(개인 진도 단추만), 문서 편집기는 읽기 전용, 서버는 강좌 수정·업무 만들기를 403으로 거부. 이미지 블록을 손잡이로
  위·아래로 끌기(일반 편집, 저장·새로고침 뒤 유지).
- 개발 예제 seed: 두 개를 동시에 돌려도 한 벌, 다시 돌려도 사용자가 고친 제목·기록과 휴지통에 넣은 항목을 덮거나 되살리지 않음,
  `dev_seed` 표시가 없는 DB(운영과 같은 조건)에서는 "run the existing dev seed first"로 거부하고 아무것도 만들지 않음. 기존 개발
  설치는 다음 `cargo xtask dev --oidc`에서 예제가 한 번 더해진다(자기 표시 `dev_module_seed_v1`).
- 브라우저 PASS(같은 절차를 모듈마다): 다섯 모듈 각각 모듈 화면에서 새 항목 만들기 → 상세 → 정보 고치기(목표와 그 종류의 전용
  칸) → 저장 → 새로고침 → 다시 읽기. 그 종류의 전용 칸만 보이고(프로젝트·강의 없음, 연구 6, 스터디 2, 취미 1), 기록 종류도 그
  모듈 것만(강의 없음), 강좌·스터디 패널은 그 종류에만. 칸반: 필터가 칸반·목록·달력·간트를 오가도 유지, 목록에서 카드를 열고
  닫아도 보기·필터·스크롤 유지, 마우스 끌기는 저장 응답(200)과 알림 뒤 새로고침해도 그 열. 위키: 트리 Alt+→/←로 들여쓰기·내어쓰기
  뒤 새로고침해도 유지, `@`로 넣은 참조가 대상 문서의 "이곳을 가리키는 문서"에 보임, `/` 메뉴 체크리스트와 체크가 새로고침 뒤
  유지, 즐겨찾기 → 홈 → 같은 문서, 검색(Ctrl/⌘+K) → 같은 문서(같은 ID·리비전).
- 통합 후보(main 6b5f127에 #30·#31·이 브랜치를 차례로 병합한 로컬 브랜치, 충돌은 `StudyPanel.svelte`의 숫자 상태와
  `done(requestId)`, `ChecklistForm` 시험 둘, 개발 문서 seed 설명 — 양쪽을 모두 남김): `cargo xtask check` 통과,
  `cargo xtask test --oidc`는 재사용한 테스트 DB와 새 DB 모두 42개 묶음 243건 통과. 그 서버에서 다섯 모듈 흐름, 칸반(필터 유지·
  카드 열고 닫기·마우스 끌기·Alt+→/←), 위키(트리·백링크·즐겨찾기·검색·현재 위치), 독자로 바꾼 사용자(편집 단추 없음·서버 403·문서 읽기 전용), 서로 다른 두 사용자의 구분선 이동 동기화를 다시 돌려 PASS.
- 최종 후보 `e83082b`(#30·#31을 병합한 main `45a4b1d`를 이 브랜치에 병합한 뒤 0069·기록 수 칸 수정): `cargo xtask check` 통과,
  새 테스트 DB의 `cargo xtask test --oidc` 42개 묶음 246건 통과(새 회귀 시험: 두 작성자가 같은 판으로 차례로·동시에 저장 →
  하나만 204·다른 하나 409·앞의 값 유지, 회차·단위 각각. 두 사람이 문구 고치기와 완료 바꾸기를 양쪽 순서로 → 둘 다 남음).
  브라우저 PASS(그 SHA의 개발 설치, 두 사용자 브라우저): 회차·공통 단위 고치기를 둘이 함께 열고 한 사람이 저장한 뒤 다른 사람이
  저장 → 409 안내·입력 유지·목록은 앞사람 값 → 다시 저장하면 바뀜·새로고침 뒤 유지. 체크리스트를 한 사람은 문구, 다른 사람은 옛
  화면에서 완료 → 다시 읽으면 둘 다 남음(양쪽 순서, 보낸 본문은 각 칸 하나). 기록 시험 점수 8에 실제 키 입력 "1e" → 저장 요청
  없음·안내·입력 유지·다시 열면 8. 같은 화면에서 고치기 전 코드로 되돌리면 저장 요청이 나가 점수가 지워짐(재현). 일부러 비운 칸은 빠짐.
  위 `bcdb098`의 통합 후보 결과는 그 SHA의 것이며, 이후 바뀌지 않은 범위(칸반·위키·편집기·모듈 흐름)는 그 증거를 그대로 쓴다.
- NOT RUN: 실제 Zotero(계정 없음), 실제 휴대전화.

모듈별 연결 상태(DB → API → 화면 → 저장 → 다시 읽기):

| 모듈     | 정상 연결                                                       | 고친 것(분류)                                                                                                                                       | 남은 것·판단                                                                                     |
| -------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 프로젝트 | 정보, 기록 4종, 업무·보기·문서·일정·시간                        | 체크리스트 문구·상위 업무(UI 연결 누락), 보관 모듈 화면·항목 파일(실제 결함), 일정 → 달력(실제 결함), 체크리스트 문구·완료가 서로 되돌림(실제 결함) | —                                                                                                |
| 연구     | 정보 6칸·키워드, 기록 6종, 투고 날짜 달력, Zotero 가져오기 화면 | 기록 숫자 칸(실제 결함)                                                                                                                             | 실제 Zotero NOT RUN                                                                              |
| 스터디   | 정보 2칸, 기록 4종, 계획·개인 진도(사람마다 따로)·재배치        | 공통 단위 고치기(UI 연결 누락), 숫자 입력·함께 고칠 때 덮어씀(실제 결함)                                                                            | 재배치·개인 진도는 보관 중에도 기록(개인 기록, 기존과 같음)                                      |
| 강의     | 강좌 정보, 회차, 개인 진도(사람마다 따로), 참여자 집계          | 회차 공통 정보·링크·자료(UI 연결 누락), 보관 모듈의 편집 표시·함께 고칠 때 덮어씀(실제 결함)                                                        | 배속 예상 시간은 모델 없음(미구현)                                                               |
| 취미     | 정보 1칸, 기록 4종                                              | 기록 숫자 칸(실제 결함)                                                                                                                             | 활동 기록 날짜는 달력에 넣지 않음(D-80: 취미는 항목 일정)                                        |
| 공통     | 권한: 독자는 편집 단추 없음·서버 403(권한으로 숨김 확인)        | 기록 첫 조회 오류·수로 읽지 못한 기록 수 칸이 저장값을 지움(실제 결함), 개발 예제(예제 부재)                                                        | 저장소 정리 누수는 main의 결함으로 #30, `@` 참조 제안은 공백이 들어가면 닫힘(목록에서 고르면 됨) |

남은 리뷰 다섯 건(2026-10-20, 이 절의 범위 안에서 좁게 확인):

| 건                                                   | 판정                                                                                                                                                                                                                                                                                                 | 처리                                                                                                                        |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 항목을 바꾸면 이전 항목의 저장한 보기를 지울 수 있음 | 재현 안 됨. 실제 앱에서 A에 보기를 저장해 고른 뒤 앱 안 검색으로 B로(같은 문서) → 업무 판이 다시 만들어짐(고른 상자에 붙인 표식이 사라짐), 선택 비고 지우기 단추 없음, 지우기 요청 없음, A의 보기 그대로. `ItemScreen`이 항목 id가 바뀌면 항목을 비우고 불러오는 중 화면을 그려 판이 다시 만들어진다 | 바꾸지 않음(확인용으로 만든 보기는 지움)                                                                                    |
| 일정 목록의 옛 읽기가 새 목록을 덮음                 | 재현(시험: 새 읽기 먼저·옛 읽기 나중 → 옛 빈 목록이 덮음)                                                                                                                                                                                                                                            | 마지막으로 보낸 읽기만 반영. 달력(`CalendarScreen`)은 effect마다 취소해 이미 안전                                           |
| 옮기기 실패 뒤 목록이 비면 실패 알림이 사라짐        | 재현(개인 노트 화면 시험)                                                                                                                                                                                                                                                                            | 트리가 빈 목록 글을 스스로 그리고 알림은 줄 밖에 둔다(세 화면)                                                              |
| 구분선 손잡이 줄에 처음 들어올 때 손잡이가 달아남    | 재현(실제 브라우저: 구분선을 고르고 마우스를 편집기 밖에 둔 뒤 손잡이 줄 아래쪽 끝으로 들어오면 다음 문단으로 옮겨 감, 위쪽 끝·가운데는 그대로)                                                                                                                                                      | 손잡이가 보이면 마우스로 띄운 것이든 선택으로 띄운 것이든 그 줄 안에서는 다시 찾지 않음. 고친 뒤 세 경우 모두 구분선에 남음 |
| 목록을 다시 읽지 못했는데 "다시 불러왔다"고 알림     | 재현(시험). 옮기기는 됐는데 다시 읽기만 실패하면 "옮기지 못했다"로도 보였다(다시 읽기가 `finally` 안이라 그 오류가 옮기기 결과를 덮음)                                                                                                                                                               | `moveThenReload`가 어느 쪽이 실패했는지 알리고 알림 문구를 그에 맞춤                                                        |

## 84. 개발 Keycloak 컨테이너가 실수로 다시 만들어진 일 (2026-10-20)

- **일어난 일**: 문서 검색 명령의 패턴을 큰따옴표로 감싸 그 안의 `` `cargo xtask dev --oidc` ``가 셸 명령 치환으로 이 체크아웃에서 실행됐다. `compose up`이
  개발 Postgres·Meilisearch는 그대로 두고(정의가 같음), 정의가 바뀐 개발 Keycloak 컨테이너를 다시 만들었다(새 `kcdata` 볼륨, 고정 포트
  8180). 8180은 다른 체크아웃의 개발 환경이 쓰고 있어 새 컨테이너는 만들어진 채 시작하지 못했고 명령은 그 자리에서 끝났다(DB 준비·
  seed·서버 실행 전).
- **잃은 것**: 예전 Keycloak 컨테이너 안의 realm·사용자·세션(그 컨테이너는 데이터 볼륨 없이 만들어졌다, 위 12.1). 그 컨테이너의 발급자
  포트(52977)와 테스트 사용자 id는 개발 DB의 어느 사용자와도 맞지 않아(DB 사용자 넷은 예전 동적 포트 52174·52514·55898의 발급자),
  이 일로 끊긴 DB 사용자 연결은 없다.
- **그대로인 것**: 개발 DB(마이그레이션 56, 사용자 4, 워크스페이스 4)와 검색 볼륨, `target/dev-files`(DB가 참조하지 않는 옛 객체
  17개 포함, 지우지 않음), 새로 만들어진 채 멈춘 Keycloak 컨테이너와 빈 `kcdata` 볼륨(시작하지 않음).
- **로그인 복구에 대해**: 개발 DB 사용자는 동적 포트 발급자 + 그때의 subject로 식별된다. 지금의 고정 포트 realm으로는 그 사용자로
  들어갈 수 없지만, 그 발급자 주소(포트)와 같은 subject를 가진 사용자를 둔 Keycloak을 따로 띄우면 이어질 수 있다. 확인하지 않았으므로
  복구 불가라고 단정하지 않는다. 사용자의 발급자·subject를 바꾸거나 데이터를 초기화하지 않는다.
- **앞으로**: 검색 패턴은 리터럴로(AGENTS.md), 오래된 개발 환경을 다시 띄우기 전에 볼륨에 없는 상태를 떠 둔다(12.1). 이후 개발 확인은
  정상인 별도 체크아웃(다른 Compose 프로젝트·포트·볼륨)에서 한다.

## 85. 앱이 만드는 UUIDv7과 셀프호스트 Supabase DB (2026-10-20, D-88, `feat/supabase-selfhost`·`feat/app-uuidv7`)

### 85.1 id는 앱이 만든다

- 호출부를 추가·정리할 때의 인자 계약과 오류 처리 구분은 [Rust→SQL UUID 인자 계약](sql-id-contract.md)을 따른다.
- **규칙**: 새 행의 id(UUIDv7)는 서버가 `fvoci_server::db::new_ids(n)`로 만들어, 행을 만드는 정의자 함수의 마지막 인자
  `p_ids uuid[]`로 넘긴다. `new_ids`는 `Uuid::now_v7()`을 쓰고, 한 프로세스에서 만든 값은 만든 순서대로 커진다.
  - 함수는 `app_id(p_ids, k)`로 차례로 꺼내 쓴다. 실패는 두 가지로 나눈다.
    - 목록이 짧음: SQLSTATE `FV001`
    - 빈 값·UUIDv7 아님·잘못된 위치: `FV002`, 서버 버그로 500
  - 표에는 id 기본값이 없다(빠지면 NOT NULL 위반). `uuid_extract_version(id) = 7` CHECK가 있다.
  - DB에는 UUID를 만드는 함수·트리거·기본값이 없다(`tests/ids.rs`가 카탈로그로 확인).
- **개수**
  - 함수마다 주석에 쓰는 id 수가 있다(예: 업무 만들기 2개 = 업무·감사).
  - 만드는 행 수가 데이터에 따라 달라지는 함수는 `<함수>_ids`가 같은 트랜잭션에서 상한을 돌려준다(`db::needed_ids`).
    대상은 댓글 알림, 권한을 잃은 담당자 정리, 문서 이동의 자식 감사, 항목 복사, 반복 회차, 휴지통 비우기, 기한·리마인더 알림이다.
  - 권한이 없으면 0이고, 대상이 없어 NULL이면 0으로 본다. 어느 쪽이든 본 함수가 권한 오류나 404를 낸다. 남는 id는 쓰지 않고 버린다.
- **상한과 실제 변경 사이의 경합**: READ COMMITTED에서는 상한을 본 문장과 본 함수가 서로 다른 스냅숏을 본다. 그사이 다른
  트랜잭션이 대상을 늘리면(새 답글, 새 담당자 등) 함수는 `FV001`로 멈춘다. DB가 조용히 채우지 않는다.
  - HTTP: `FV001`을 409 "다시 시도"로 돌려주고 경고 로그(`db.new_ids_short`)를 남긴다. 요청의 트랜잭션은 커밋되지 않고 통째로
    되돌아간다. 다시 보내면 상한 조회부터 새 id 생성·전체 작업을 처음부터 한다.
  - 작업 프로세스: 같은 매핑을 거치지 않는다. 그래서 `worker::system_call`이 `FV001`이면 트랜잭션을 통째로 되돌리고, 상한부터
    새 id로 처음부터 다시 한다(최대 3번, `job.new_ids_short` 경고). 실패한 트랜잭션 안에서 본 함수만 다시 부르지 않는다. 반복
    회차·휴지통 정리처럼 함수 안에서 대상마다 예외를 잡는 작업은 그 대상만 경고하고 다음 주기에 다시 한다.
  - 시험(`tests/ids.rs`)
    - 상한을 0으로 속인 작업은 정해진 횟수 뒤 오류로 끝나고 아무 알림도 남기지 않는다.
    - 맞는 상한으로는 한 번만 만든다.
    - `FV001`은 409, `FV002`는 500이다.
    - 없는 그룹 지우기·멤버 빼기는 404다.
- **제목 변경 감사**: AFTER UPDATE 트리거는 앱이 만든 id를 받을 수 없어 지웠다. 제목을 바꾸는 두 길(저장·실시간 편집의
  `write_revision`, 리비전 복원)이 리비전을 남긴 뒤 `app_audit_document_rename(id, 문서, 리비전)`을 부른다. 이전·새 제목은 그 리비전과
  바로 앞 리비전에서 읽고(앱이 꾸밀 수 없음), 부분 고유 색인이 리비전마다 한 번만 남게 한다. 감사 상세에 `revision`이 더해졌다.
- **개발 준비 SQL**: `cargo xtask dev`가 UUIDv7 1000개를 만들어 세션 설정(`fvoci.dev_ids`)으로 넘기고, `deploy/postgres/dev-ids.sql`의
  `pg_temp.take(n)`·`take1()`이 꺼내 쓴다(모자라면 멈춤). 테스트 고정 자료(`tests/records.rs` 등)도 id·요청 id를 Rust에서 만든다.
- **마이그레이션**: 0070~0074가 도메인별로 새 서명 함수(103개)를 옛 서명 옆에 만들고 서버 호출을 옮겼다. 0075가 옛 서명 102개와
  id 기본값을 지우고 CHECK를 둔다. 기존 마이그레이션 17개의 `CREATE TABLE ... DEFAULT uuidv7()`도 지웠다(PostgreSQL 17에서 첫
  마이그레이션부터 설치되게). 실제 배포 전이라 checksum 보존은 요구하지 않는다는 사용자 지시에 따랐다.
- **기존 개발 DB**: 위 변경으로 이미 적용된 마이그레이션의 checksum이 달라져 그 전에 만든 개발 DB에서는 `db migrate`가 멈춘다. 다시
  만들기 전에 남길 데이터를 확인한다(2026-10-20 읽기 전용 확인: 이 체크아웃의 개발 DB는 마이그레이션 56·사용자 4·문서 21, `fvoci-dev`
  체크아웃은 마이그레이션 69·사용자 2·문서 17, 그 밖에 멈춘 `fvoci`·`relcheck` 프로젝트의 볼륨). 지우기는 사용자 확인 뒤에 한다.

### 85.2 검증 (PR-B)

| 검사                                                          | 결과                                                                                                                                                                                  |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo xtask test`(PostgreSQL 18, 도메인마다·마지막)          | PASS 244(마지막). 도메인 전환 중 잡은 실패 3건: 답글 알림 상한이 방금 쓴 답글 작성자를 세지 않음, `row_number()`가 bigint라 `app_id` 호출이 안 맞음, 옛 서명을 직접 부르던 테스트 2곳 |
| 빈 셀프호스트 Supabase(`supabase/postgres:17.6.1.136`)        | `db bootstrap` 2회·`db migrate` 2회 PASS(74개, 0020 없음). 함수 본문의 `uuidv7()` 0건                                                                                                 |
| `plpgsql_check`(같은 Supabase DB, fvoci의 plpgsql 함수 204개) | 오류 0건(테스트가 닿지 않는 분기의 함수 이름·인자 형 확인)                                                                                                                            |
| PG18 카탈로그 비교(main 경로 대 새 경로, 빈 DB)               | 차이는 id 기본값 27개 삭제·UUIDv7 CHECK 27개·제목 감사 고유 색인·트리거 삭제·새 도우미 함수뿐. 정책·RLS·표 권한·같은 이름 함수의 SECURITY DEFINER·search_path·실행 권한·소유자 같음   |
| 서버·테스트의 함수 호출 238개 인자 수                         | DB 서명과 모두 맞음                                                                                                                                                                   |
| 새 개발 준비 SQL(빈 PG18 DB, 격리 컨테이너)                   | PASS(항목 5·업무 7·기록 18, 감사 79건 모두 UUIDv7). 이 체크아웃·다른 체크아웃의 개발 DB에는 실행하지 않음                                                                             |

### 85.3 셀프호스트 Supabase 시험 구성 (PR-C)

- `cargo xtask test --supabase [--session-pooler] [--oidc]`
  - 공식 셀프호스트 구성(`supabase/supabase@bc27d26`의 `docker/`)을 `target/supabase/<rev>/`에 얕게 받는다. 저장소에는 넣지 않는다.
  - `deploy/supabase/compose.test.yaml`을 겹쳐 db·rest·supavisor만 띄운다. 이미지는 digest로 고정하고, 고정 컨테이너 이름을 지우고,
    `127.0.0.1` 빈 포트와 tmpfs 데이터를 쓴다. Compose 프로젝트는 `fvoci-<체크아웃>-<해시>-supabase`다.
  - 검색·S3(·Keycloak)은 시험 Compose에서 PostgreSQL만 빼고 띄운다. 값은 `deploy/supabase/test.env`의 시험 값이다.
  - 관리자는 Supabase의 `postgres`(superuser 아님)다. `cargo xtask down`이 이 프로젝트도 지운다.
- 빼는 서비스와 이유는 `docs/self-hosting-supabase.md` 6절에 있다. 운영자 안내도 그 문서에 있다.
- CI `supabase` job이 같은 명령(`--session-pooler --oidc`)을 러너에서 돌린다. Supabase 서비스를 호스트 포트로 열므로 base 이미지
  컨테이너 안에서 돌리지 않는다.
- Supabase에서만 도는 시험(`supabase_*`, 다른 구성에서는 NOT RUN)
  - 관리자가 PG17의 비-superuser `postgres`인지
  - Supabase 역할이 fvoci 스키마·표·함수에 권한이 없는지, GroundZero 역할이 그 멤버가 아닌지
  - RLS가 `auth.*`를 쓰지 않는지
  - PostgREST가 `fvoci` 프로필을 거부하는지
- 두 구성 공통 시험
  - `db_context`: 연결 재사용 뒤 사용자 문맥이 남지 않는지
  - `backup`: 저장소 잠금이 같은 세션에서 풀리는지, 복구 뒤 RLS가 그대로인지

### 85.4 검증 (PR-C, 2026-10-20~21)

대상 커밋은 이 저장소의 커밋이다. `7e52400` 뒤의 커밋은 문서만 바꾼다. 공식 Supabase 구성은 모두 `bc27d26`이다.

| 대상 커밋                                      | 검사                                                                                | 결과                                                                                                                                                                                                                          |
| ---------------------------------------------- | ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2bf121f`(로컬 macOS)                          | `cargo xtask test --supabase --session-pooler --oidc`                               | 직접 253·session 253 PASS. 첫 시도는 session 2건 FAIL(`XX000`): 시험 env의 풀 크기 20이 동시 연결보다 작음. 64로 늘린 뒤 통과                                                                                                 |
| `cbd1e44`(로컬, 서버 코드는 `36f7e15`와 같음)  | 브라우저: 서버·웹을 Supabase DB와 시험 Keycloak으로, 별도 프로젝트·포트 18080/15173 | 아래 순서로 진행했고 마지막에 모두 그대로였다. PG 17.6, 만든 행 id 13개 모두 UUIDv7, 제목 변경 감사 `revision: 2`                                                                                                             |
| `799389d`(CI 39435831294)                      | `supabase` job                                                                      | FAIL: supavisor가 시작 스크립트의 `ulimit -n 100000`에서 실패(러너의 열린 파일 한도 부족)해 재시작 반복. 정리 전 진단을 넣은 `8b59d3a`에서 로그로 확인했다. `--ulimit nofile=65536`으로 재현(exit 1), 100000으로 해결(exit 0) |
| `6f1e408`(CI 39440580620)                      | 전체 CI(`supabase` job 포함)                                                        | 모두 PASS. `supabase` job은 직접 255·session 255 PASS, NOT RUN 0                                                                                                                                                              |
| `6f1e408`(로컬 macOS)                          | `cargo xtask test --supabase --session-pooler --oidc`                               | 직접 255·session 255 PASS                                                                                                                                                                                                     |
| `7e52400`(로컬 macOS)                          | `cargo xtask test --supabase --session-pooler --oidc`                               | 직접 256·session 256 PASS, NOT RUN 0(관리자 멤버십 보존·연결이 끊긴 뒤 새 연결로 되돌리기 테스트 추가)                                                                                                                        |
| `7e52400`(로컬, 버리는 Supabase·PG18 컨테이너) | bootstrap·복구 거부 경로, 관리자 멤버십 보존                                        | 아래 표                                                                                                                                                                                                                       |
| —                                              | Supabase Cloud, transaction 모드(6543), Supabase와 함께 쓰는 앱 Compose 구성        | NOT RUN(범위 밖·제공하지 않음)                                                                                                                                                                                                |

`cbd1e44` 브라우저 검증 순서:

1. OIDC 로그인 → 초기 관리자 워크스페이스
2. 위키 문서 만들기 → 본문·제목 고치기(리비전 2)
3. 모듈·항목·업무 만들기 → 항목 설명 고치기
4. 로그아웃 → 서버 재시작 → 다시 로그인

`7e52400` 거부 경로·멤버십 보존 결과:

| 대상     | 경우                                                                             | 결과                                                               |
| -------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Supabase | 앱 역할에 `BYPASSRLS`를 붙임                                                     | 두 역할의 비밀번호가 그대로인 채 거부                              |
| Supabase | `supabase_admin`이 만든 역할                                                     | ADMIN OPTION 안내로 거부                                           |
| Supabase | 그룹 역할을 거친 ADMIN OPTION                                                    | 통과                                                               |
| Supabase | `postgres`의 복구 권한 점검                                                      | 통과                                                               |
| PG18     | 비-superuser 관리자의 복구                                                       | 파일을 올리기 전에 `session_replication_role` 안내로 거부          |
| Supabase | `postgres`가 스스로 준 SET 없는 멤버십(INHERIT 켬)이 있을 때 bootstrap 다시 실행 | 통과. `postgres`의 `fvoci_migrator` 멤버십 레코드가 실행 전과 같음 |
| Supabase | 같은 멤버십이 있을 때 복구가 버전 확인에서 멈춤(멤버십을 빌린 뒤)                | 거부. 멤버십 레코드가 실행 전과 같음                               |

## 86. UI·UX 점검: 모듈 흐름, 참고 제품 비교, 개발 예제 보강 (2026-10-21, `feat/ui-module-flows`, 시작 main `f9700d7`)

사용자 지시: 최신 main 기준의 별도 worktree에서 UI·UX와 모듈별 백엔드 연결을 점검하고 미완료 항목을 구현한다. Supabase 작업
(PR #33~#35, `feat/supabase-harness`)과 SQL 마이그레이션·DB 역할·RLS는 범위 밖이며 이번 변경에 서버 코드·마이그레이션은 없다.
이어서 Focalboard·Kaneo·Outline을 공식 이미지로 격리 실행해 같은 합성 예제로 흐름을 비교하라는 지시를 받았다.

### 86.1 현재 상태 대조

화면·API 호출·서버 라우트를 대조했다. 서버 라우트 139개 가운데 웹이 부르지 않는 것은 IdP가 돌려보내는 `/auth/callback`
하나이고, 웹 코드에 빈 클릭 처리·TODO·임시 목록은 없다. 그래서 "연결 누락"보다 실제로 써 보며 드러나는 결함을 찾았다.
분류의 "확인"은 다섯 모듈의 정보 편집→저장→새로고침→재조회 흐름을 확인했다는 뜻이고 모듈 기능 전체가 끝났다는 뜻이 아니다.

| 모듈·기능                | 화면 → API → 저장 → 다시 읽기                      | 이번 확인(개발 설치, 브라우저)                                                                                          | 분류                                                                      |
| ------------------------ | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 프로젝트                 | 정보·기록·업무(칸반·목록·간트·달력)·문서·일정·시간 | 칸반 빠른 추가·끌기(열 사이·열 안)·Alt+→·카드 편집·닫기 뒤 초점·새로고침, 요청 실패 주입 → 되돌림·안내 → 다시 시도 성공 | 정보 편집→저장→새로고침→재조회 확인(결함 4건 고침)                        |
| 연구                     | 정보 6칸·기록·투고 날짜                            | 분야 고치기 → 새로고침 유지                                                                                             | 정보 편집→저장→새로고침→재조회 확인. 실제 Zotero 연동은 미검증(계정 없음) |
| 스터디                   | 정보 2칸·계획·개인 진도                            | 목표 고치기 → 새로고침 유지                                                                                             | 정보 편집→저장→새로고침→재조회 확인                                       |
| 강의                     | 강좌·회차·개인 진도                                | 범위 고치기 → 새로고침 유지                                                                                             | 정보 편집→저장→새로고침→재조회 확인. 배속 기준 예상 시간은 미구현(83절)   |
| 취미                     | 정보 1칸·기록                                      | 활동 유형 고치기 → 새로고침 유지                                                                                        | 정보 편집→저장→새로고침→재조회 확인                                       |
| 문서 편집기              | 블록 손잡이·구분선                                 | `/` 구분선 넣기, 손잡이로 구분선 끌기·편집기 밖에서 놓기, 편집 영역 폭 변경                                             | 결함 2건 고침                                                             |
| 위키 트리                | 트리·현재 표시·Alt 이동·백링크                     | 세 단계 트리 펼침·현재 표시, Alt+←/→ 뒤 새로고침 유지, 백링크 → 이동 → 뒤로 가기                                        | 확인(트리·이동·백링크 흐름)                                               |
| 크기 조절 구분선(분할선) | —                                                  | 제품에 분할선이 없다(사이드바는 접기·펴기만)                                                                            | 해당 없음. 새로 만들지 않음                                               |

### 86.2 참고 제품 비교 환경

설치 문서·라이선스·설정 파일만 읽고 제품 소스·번들은 보지 않았다. 세 제품 모두 별도 Compose 프로젝트(`refcmp-*`)로 띄우고 포트는
127.0.0.1에만 열었으며, GroundZero의 DB·볼륨·Keycloak과 공유하지 않는다. 비밀 값은 로컬에서 만든 `.env`(권한 600)에만 있고
설정·스크린샷은 저장소 밖(세션 작업 폴더 `refcompare/`)에 둔다. 계정은 검토자가 만들지 않았고 조작 시점에 브라우저 패널에 이미 로그인된 세션을 썼다(사용자가 채팅으로 알려 준 비밀번호는 쓰지 않음)
(제3자 제품의 계정 생성·비밀번호 입력은 검토자가 하지 않음). 클릭해 동의하는 약관과 보안 설정 변경은 없었다.

| 제품       | 실행 판·이미지                                                                                                                                                                                                                                                                                                          | 라이선스                                                          | 비고                                                                                          |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Focalboard | 7.8.9, `mattermost/focalboard:latest@sha256:337346b132a7c1bd988b5fea59a9050ade42b9855399b0add5f95cf8f99ba800`(amd64 단일, arm64 Mac에서 에뮬레이션)                                                                                                                                                                     | Mattermost 컴파일본 MIT, 소스 AGPL-3.0(일부 설정·도구 Apache-2.0) | 저장소 README가 "not maintained". 공식 README의 `docker run`                                  |
| Kaneo      | 2.28.3, `ghcr.io/usekaneo/kaneo:2.28.3@sha256:414d153e5f01cc9d5bdeb148e6c43ac2efaf9787bb8295dddcf38e01d3b96d03`, `postgres:16-alpine@sha256:721873c34ceb9f8d8fc265984940dc982404c105f19ad51be9fdc5970a6080ea`                                                                                                           | MIT                                                               | 공식 Docker Compose 안내. 웹 포트만 5273(GroundZero 5173과 분리)                              |
| Outline    | 1.11.0, `docker.getoutline.com/outlinewiki/outline:1.11.0@sha256:08ee9ffdb847e1bd35ded30ee832090dd06a7f93f67422687db59e70bc7f5393`, `redis:latest@sha256:2c2dff791878316e3b083188be0d578423b24813cdda5de0e80ea4f7b3e6bfd6`(8.10.2), PostgreSQL은 GroundZero와 같은 `postgres:18.3-bookworm` 이미지의 별도 컨테이너·볼륨 | BSL 1.1(문서 서비스 상업 제공 금지, 2030-10-12 Apache-2.0 전환)   | 자체 로그인이 없어 인증용 Keycloak 26.5.4를 따로 띄움(realm `refcmp`, 가입 허용, 사용자 없음) |

막힌 것: Outline의 "New collection" 창이 이 자동 조작에서 열리지 않아 기본 Welcome 컬렉션에 예제를 만들었다. Outline의 `@` 제안 창이
합성 입력으로 뜨지 않아 문서 주소를 붙여넣어 내부 링크를 만들었다. Focalboard 첫 열 이름이 한 번 새로고침 뒤 원래대로였는데 앞선
입력이 섞인 탓으로 보여(다시 고치자 유지) 관찰에서 뺐다.

### 86.3 관찰과 설계

화면은 내부 비교 근거로만 쓰고 로고·아이콘·문구·화면 구성을 가져오지 않았다. 반영은 확정 결정(D-27·D-30·D-79·D-82)과 실제 흐름에
맞는 것만 했다.

| 하려는 일                | 참고 제품                                                                                                                                                       | 장단점                                    | GroundZero 설계                                                                                                                                                                | 검증                                                                                   |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| 열에서 바로 카드 만들기  | Focalboard: 열의 "+ New"가 빈 카드 상세 창을 열고 제목에 초점. Kaneo: 열의 "+"가 그 열 상태로 정해진 만들기 창(제목 초점, 우선순위·담당·날짜 칩, "계속 만들기") | 바로 쓸 수 있으나 창이 흐름을 가린다      | 열 아래 입력란은 그대로 두고, 열 때 입력란에 초점·Esc로 닫고 "업무 추가" 단추로 초점 복귀(전에는 초점이 사라지고 닫을 수 없었다). 만든 뒤 입력란이 남아 이어 만들 수 있다      | 컴포넌트 시험(고치기 전 실패), 브라우저 키보드만으로 열기·만들기·닫기                  |
| 다른 열에 만든 업무 알림 | —                                                                                                                                                               | —                                         | 만든 업무 제목을 넘겨 "새 업무: 옮겼습니다(진행 중)."(전에는 제목이 빈 ": 옮겼습니다")                                                                                         | 시험·브라우저                                                                          |
| 열 이동·순서             | Focalboard: 끌기만(카드 메뉴에 옮기기 없음). Kaneo: 끌기 + Space로 집고 방향키(오른쪽 키가 옆 열로 가지 않음, 낭독기 안내가 한국어 화면에서도 영어)             | 끌기는 자연스럽지만 키보드 대안이 약하다  | 기존 끌기·Alt+방향키·옮기기 메뉴·한국어 안내 유지(D-79). 끄는 중 창을 떠나거나(blur) 단추가 눌리지 않은 이동이 오면 취소(전에는 끌기가 남아 다음 클릭이 카드를 놓을 수 있었다) | 시험(대조: 누른 채 놓으면 옮겨짐), 브라우저 마우스 끌기. 실제 창 전환은 수동 확인 필요 |
| 카드 상세                | Kaneo: 오른쪽 패널, 주소 `?taskId`, 열면 초점이 "작업 삭제"에 먼저. Focalboard: 대화상자, Esc가 열린 속성 메뉴가 아니라 창 전체를 닫고 초점이 본문으로          | 지우기 단추 초점은 실수 위험              | 지금처럼 열면 "닫기"에 초점, 닫으면 카드로 초점(바꾸지 않음)                                                                                                                   | 브라우저                                                                               |
| 우선순위                 | Focalboard: 사용자 정의 선택 속성 → 카드 표시·"Group by". Kaneo: 만들기 창 칩(없음~긴급)                                                                        | 자유롭지만 정본이 흩어진다                | 고정 필드(없음~긴급)로 상세에서 고르고 카드 표시·거르기·정렬. 속성 정의·열 기준 바꾸기는 D-79(사용자 정의 열 넣지 않음)에 따라 관찰만                                          | —                                                                                      |
| 서버 무응답 중 저장      | 세 제품 모두 5초 넘게 진행·실패 표시 없음. Focalboard는 재개 뒤 빈 카드 창이 늦게 열림, Kaneo·Outline은 재개 뒤 저장                                            | 끊김을 모른다                             | 옮기면 바로 "저장 중", 3초 뒤 "응답 지연", 10초 뒤 서버 목록과 대조. 알 수 없으면 실패로 단정하지 않고 자동으로 다시 보내지 않는다(86.9)                                       | 시험·브라우저(서버 정지, 실패 주입)                                                    |
| 빈 상태·로딩             | Kaneo: 빈 프로젝트 안내+만들기 단추, 빈 열은 안내 없음. Outline: 홈 빈 목록 안내                                                                                | —                                         | 첫 목록 전에 빈 열·"업무가 없습니다" 대신 "불러오는 중…", 담당자 칸도 받기 전에는 "없습니다" 대신 "불러오는 중…"                                                               | 시험(고치기 전 실패)                                                                   |
| 작은 화면                | Focalboard: "Mobile web early beta" 배너, 가로 스크롤. Kaneo: 열 하나씩 가로 스크롤(빈 열이 화면을 채움). Outline: 사이드바를 메뉴 뒤로                         | —                                         | 열을 세로로 쌓는 지금 방식 유지(76절)                                                                                                                                          | 375px 보기                                                                             |
| 위키 트리·선택           | Outline: 게시할 때 위치 고르기, 트리에서 현재 문서 강조·자동 펼침                                                                                               | —                                         | 사이드바·본문 트리의 현재 표시·자동 펼침 유지                                                                                                                                  | 브라우저                                                                               |
| 문서 옮기기              | Outline: 사이드바 끌기(확인·되돌리기 안내 없음), 메뉴 "Move"는 검색으로 대상을 찾고 "Move to …"로 확인한 뒤 완료 알림                                           | 검색형 대상 고르기는 깊은 재배치에 빠르다 | 트리의 Alt+방향키·메뉴 유지. 검색으로 상위 문서를 고르는 창은 기존 API(`PUT /api/documents/{id}/parent`)로 가능하지만 새 흐름이라 이번에 넣지 않음(86.7·86.8 제안)             | —                                                                                      |
| 백링크                   | Outline: 문서 아래 "Documents"(하위)·"Backlinks" 탭, 누르면 링크가 있는 자리(`#d-…`)로                                                                          | 맥락까지 간다                             | "이곳을 가리키는 문서"에서 누르면 그 문서로, 뒤로 가기 복귀                                                                                                                    | 브라우저                                                                               |

### 86.4 고친 것

| 커밋      | 파일                                                                                              | 내용                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| --------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `b202930` | `documents/block-ops.ts`·`TiptapEditor.svelte`·`BlockControls.svelte`                             | Tiptap 3.32의 `setHorizontalRule`은 뒤 블록이 글줄 블록이 아니면(체크리스트·목록·표·수식) 그 블록을 노드 선택해, `/` 메뉴로 구분선을 넣고 이어 입력하면 그 블록 내용이 지워진 채 저장됐다(재현: 문단 뒤에 체크리스트가 있을 때). 그때는 구분선 아래 빈 문단에 커서를 둔다(`---` 입력 규칙은 원래 안전). 손잡이 자리를 편집 영역 크기 변화(ResizeObserver)에도 다시 계산한다(전에는 편집 영역 폭을 원래대로 되돌린 뒤 블록이 82px 올라가도 손잡이가 옛 자리에 남았다) |
| `50806b2` | `tasks/TaskBoard.svelte`·`TaskDetail.svelte`·`workspaces/CreateForm.svelte`                       | 86.3의 빠른 추가 초점·Esc·알림 제목, 끌기 취소, 불러오는 중 표시. `CreateForm`에는 선택 속성 `autofocus`·`oncancel`만 더함(폭이 좁으면 단추가 아래 줄로)                                                                                                                                                                                                                                                                                                             |
| `5f9fe5e` | `deploy/postgres/dev-module-seed.sql`                                                             | 개발 예제 보강(12.1의 v2)                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `7d4f353` | `tasks/move-sync.ts`(새 파일)·`TaskBoard.svelte`·`messages/tasks/*.json`                          | 칸반 이동의 저장 상태: 낙관적 이동과 서버 확정을 나누고 저장 중·응답 지연·확인 중·확인 못 함·저장되지 않음을 카드와 알림 영역에 보인다. 결과를 모르면 목록을 다시 읽어 대조하고, 다시 시도는 사용자가 누를 때만 서버 상태를 다시 확인한 뒤 보낸다(86.9)                                                                                                                                                                                                              |
| `1588b65` | `tasks/TaskBoard.svelte`·`messages/tasks/*.json`                                                  | 응답 없이 끝난 요청의 "응답 지연" 타이머가 남아 "확인 못 함"을 "응답 지연"으로 되돌리던 것. 지연 안내 문구를 "결과를 아직 알 수 없습니다"로                                                                                                                                                                                                                                                                                                                          |
| `5e63643` | main `726343c` 병합: `deploy/postgres/dev-module-seed.sql`·이 문서·`reference-products-survey.md` | seed v2를 D-88 서명과 `pg_temp.take*`로, 문서 85절(main)·86절(이 절) 정리(86.10)                                                                                                                                                                                                                                                                                                                                                                                     |
| `b722918` | `tasks/TaskBoard.svelte`·`move-sync.ts`·`messages/tasks/*.json`                                   | 다시 시도 응답의 판이 이 이동만으로 될 판보다 크면 "다른 변경을 덮었을 수 있음"을 알림(86.9)                                                                                                                                                                                                                                                                                                                                                                         |
| `3cb596b` | `tasks/TaskBoard.svelte`·`TaskDetail.svelte`·시험·seed 주석·조사 문서 날짜                        | CodeRabbit 지적: 겹친 대조, 실패 안내 순서, 담당자 목록 실패, `/` 메뉴 구분선 호출 경로 시험(86.11)                                                                                                                                                                                                                                                                                                                                                                  |

### 86.5 검증

| 검사                                               | 결과   | 내용                                                                                                                                                                                                                                    |
| -------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 웹 단위·컴포넌트 시험                              | PASS   | 56개 파일 259건(이번에 더한 시험: 구분선 3건, 칸반·상세 6건 중 대조 1건, 칸반 저장 상태 16건 86.9). 새 시험은 고치기 전 코드에서 실패하고(저장 상태는 컴포넌트 시험 11건, 순수 함수 시험은 새 파일이라 해당 없음) 대조 시험은 양쪽 통과 |
| `format:check`·`lint`·`check`(svelte-check 경고 0) | PASS   | 수정 뒤 다시 실행                                                                                                                                                                                                                       |
| `cargo xtask check`                                | PASS   | 86.6                                                                                                                                                                                                                                    |
| `cargo xtask test --oidc`                          | PASS   | 86.6                                                                                                                                                                                                                                    |
| 예제 seed                                          | PASS   | v1만 있는 개발 DB에 v2 추가(업무 12·위키 4·백링크 3), 다시 실행해도 행 수 같음, 새 임시 DB에서 v1+v2 한 번에, 예제 워크스페이스를 지운 경우(표시만), 실패 경우(경고·되돌림·표시 없음)는 되돌리는 트랜잭션 안에서 확인                   |
| 브라우저                                           | PASS   | 86.1 표의 흐름. 구분선 끌기는 손잡이로 목록 뒤로 → 저장 → 새로고침 유지, 사이드바에 놓기 → 변화 없음                                                                                                                                    |
| 실제 창 전환 중 끌기, 실제 휴대 기기, 실제 Zotero  | 미검증 | 자동화로 창 전환을 일으킬 수 없음(시험으로 대신), 기기·계정 없음                                                                                                                                                                        |

### 86.6 최종 검사

main `726343c`를 합친 뒤(86.10). 시험은 저장소 밖 임시 worktree를 그 SHA로 맞춰(분리 HEAD) 돌렸고, 개발 seed는 그 worktree의 따로 띄운 개발
설치(새 DB)에서 확인한 뒤 그 설치만 지웠다(다른 컨테이너·볼륨은 전후 목록이 같음).

| SHA               | 검사                                                                      | 결과                                                                                                                                             |
| ----------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `b722918`         | `cargo xtask check`                                                       | PASS. 건너뛴 단계 없음, Rust 단위 69건, 웹 시험 56개 파일 261건, svelte-check 2953개 파일 오류·경고 0, 라이선스 포함                             |
| `b722918`         | `cargo xtask test --oidc`                                                 | PASS. 45개 묶음 255건, 실패·무시 0. NOT RUN은 Supabase 시험 줄 하나(아래에서 따로 돌림)                                                          |
| `b722918`         | `cargo xtask test --supabase --session-pooler --oidc`                     | PASS. 직접 연결 258건, session pooler 258건, 실패·무시 0, NOT RUN 없음                                                                           |
| `b722918`         | 개발 seed 네 경우(새 DB, 다시 실행, 기존 행 보존, 지운 예제 워크스페이스) | PASS(86.10 표와 같은 방법)                                                                                                                       |
| `f61a3eb`         | 개발 seed 네 경우 + 프로젝트 항목을 지운 뒤 v2(되돌리는 트랜잭션 안)      | PASS. 새 DB 경고 없음 `tasks_added` 12·백링크 3, 다시 실행·보존 행 수 같음, 지운 워크스페이스는 `skipped`만, 항목을 지운 경우 설치 문서 링크 0건 |
| `84c1ef4`         | `cargo xtask check`                                                       | PASS. 건너뛴 단계 없음, Rust 단위 69건, 웹 시험 57개 파일 265건, svelte-check 2955개 파일 오류·경고 0, 라이선스 포함                             |
| `84c1ef4`         | `cargo xtask test --oidc`                                                 | PASS. 45개 묶음 255건, 실패·무시 0. NOT RUN은 Supabase 시험 줄 하나(아래에서 따로 돌림)                                                          |
| `84c1ef4`         | `cargo xtask test --supabase --session-pooler --oidc`                     | PASS. 직접 연결 258건, session pooler 258건, 실패·무시 0, NOT RUN 없음                                                                           |
| `3346f24`         | `cargo xtask check`                                                       | PASS. 건너뛴 단계 없음, Rust 단위 69건, 웹 시험 57개 파일 266건, svelte-check 2955개 파일 오류·경고 0, 라이선스 포함                             |
| `3346f24`         | `cargo xtask test --oidc`                                                 | PASS. 45개 묶음 255건, 실패·무시 0. NOT RUN은 Supabase 시험 줄 하나(아래에서 따로 돌림)                                                          |
| `3346f24`         | `cargo xtask test --supabase --session-pooler --oidc`                     | PASS. 직접 연결 258건, session pooler 258건, 실패·무시 0, NOT RUN 없음                                                                           |
| `718ea42`         | 문서만 바뀜                                                               | `prettier --check` 확인, 원격 CI 통과                                                                                                            |
| `1a1575d`         | `cargo xtask check`                                                       | PASS. 건너뛴 단계 없음, Rust 단위 69건, 웹 시험 57개 파일 267건, svelte-check 2955개 파일 오류·경고 0, 라이선스 포함                             |
| `1a1575d`         | `cargo xtask test --oidc`                                                 | PASS. 45개 묶음 255건, 실패·무시 0. NOT RUN은 Supabase 시험 줄 하나(아래에서 따로 돌림)                                                          |
| `1a1575d`         | `cargo xtask test --supabase --session-pooler --oidc`                     | PASS. 직접 연결 258건, session pooler 258건, 실패·무시 0, NOT RUN 없음                                                                           |
| `32f2e5f`         | 문서만 바뀜                                                               | `prettier --check` 확인, 원격 CI가 이 커밋에서 돎                                                                                                |
| `4d2ec2d`         | `cargo xtask check`                                                       | PASS. 건너뛴 단계 없음, Rust 단위 69건, 웹 시험 57개 파일 268건, svelte-check 오류·경고 0, 라이선스 포함                                         |
| `4d2ec2d`         | `cargo xtask test --oidc`                                                 | PASS. 45개 묶음 255건, 실패·무시 0. NOT RUN은 Supabase 시험 줄 하나(아래에서 따로 돌림)                                                          |
| `4d2ec2d`         | `cargo xtask test --supabase --session-pooler --oidc`                     | PASS. 직접 연결 258건, session pooler 258건, 실패·무시 0, NOT RUN 없음                                                                           |
| 이 줄을 적은 커밋 | 문서만 바뀜                                                               | `prettier --check` 확인. 위 세 검사는 다시 돌리지 않음(NOT RUN), 원격 CI가 이 커밋에서 돈다                                                      |

- 칸반 저장 상태(86.9)를 더한 뒤 `226f087`에서 다시 돌림: `cargo xtask check` PASS(건너뛴 단계 없음, Rust 단위 67건, svelte-check
  2952개 파일 오류·경고 0, 웹 시험 56개 파일 259건, 라이선스 포함), `cargo xtask test --oidc` PASS(새 테스트 DB, 42개 묶음 247건, 실패·무시 0,
  NOT RUN 줄 없음). 그 뒤 바뀐 것은 이 문단뿐이다. 아래 두 줄은 그 전 결과다.
- `cargo xtask check`(`8039756`, 이 절을 채우기 직전): PASS. fmt·clippy(`-D warnings`)·Rust 단위 67건·cargo deny·OpenAPI 최신·format:check·lint·svelte-check(2934개 파일, 오류·경고 0)·i18n·rhwp 고지·제3자 고지·웹 시험 55개 파일 243건·라이선스. 건너뛴 단계 없음.
- `cargo xtask test --oidc`(`5f9fe5e`, 새 테스트 DB): PASS, 42개 묶음 247건, 실패·무시 0, NOT RUN 줄 없음. 그 뒤 바뀐 것은 이 문서뿐이다. 첫 실행은 디스크가 가득 차(여유 315 MB) 테스트 바이너리 링크에서 멈췄고, 사용자 지시로 Docker 빌드 캐시(24.5 GB)를 지운 뒤 다시 돌렸다.

### 86.7 세부 기능 조사와 모듈별 보강 판단

사용자 요청으로 세 제품의 기능을 항목별로 실제 조작해 확인했다(같은 합성 예제, 설치 문서·화면만, 소스·번들 분석 없음). 제품별 조사 기록 전문은 [`docs/reference-products-survey.md`](reference-products-survey.md)에 있고, 스크린샷은 저장소 밖 `refcompare/shots/`에 둔다(저장소에 넣지 않음).

#### GroundZero에서 보강할 모듈

판단 기준: 확정 결정(D-xx)에 있는 기능의 사용 흐름을 좋게 하는 것만 이번에 반영. 새 기능·권한·외부 서비스는 제안으로 남긴다.

| 참고 제품에서 본 것                                          | GroundZero 지금                                            | 보강할 모듈                       | 판단                                                              |
| ------------------------------------------------------------ | ---------------------------------------------------------- | --------------------------------- | ----------------------------------------------------------------- |
| 열에서 바로 만들기 + 초점 + 계속 만들기 (Kaneo·Focalboard)   | 열 아래 입력란이 있었으나 초점·Esc 없음                    | 프로젝트(공통 칸반: 모든 모듈)    | 반영(86.4)                                                        |
| 카드 이동 되돌리기 ⌘Z (Focalboard)                           | 실패 시 자동 되돌림만, 사용자가 되돌리기는 다시 옮김       | 공통 칸반                         | 제안: 옮긴 뒤 알림에 "되돌리기"(같은 이동 API)                    |
| 달력 날짜 칸 "+"로 그 날짜 업무 만들기 (Focalboard)          | 달력은 보기만, 만들기는 업무 목록에서                      | 공통 달력(항목·모듈·워크스페이스) | 제안                                                              |
| 간트 막대 끌기·가장자리로 기간 바꾸기 (Kaneo)                | 상세에서 날짜 고치기, 선행 관계면 영향 미리보기·승인(D-31) | 프로젝트·연구·스터디·취미(간트)   | 제안: 끌기 끝에 기존 일정 미리보기·승인 경로를 거쳐야 함(D-31)    |
| 업무 활동 기록(누가 무엇을 바꿨나) (Kaneo)                   | 감사 기록은 관리자 화면(D-57)                              | 공통 업무                         | 사용자 결정 필요: 멤버에게 변경 이력을 보이는 것은 열람 범위 변경 |
| 열 단위 합계(예상 시간 합 등) (Focalboard)                   | 카드에 예상 시간, 항목 시간 합계(65절)                     | 프로젝트                          | 제안(표시만, D-32 예상·실제 비교와 연결)                          |
| 사용자 정의 열·필드·자동화(GitHub 이벤트) (Kaneo·Focalboard) | 상태 3열 고정                                              | —                                 | 범위 밖(D-79 "사용자 정의 열·자동화 넣지 않음")                   |
| 달력 구독(ICS)·외부 알림 연동 (Kaneo)                        | 앱 알림·선택적 이메일(D-55)                                | —                                 | 범위 밖(새 외부 표면, 사용자 결정)                                |
| Backlog(보드 밖 계획됨) (Kaneo)                              | 할 일 열                                                   | —                                 | 범위 밖(새 개념)                                                  |
| 검색으로 상위 문서 고르기 "Move to…" (Outline)               | 트리 Alt+방향키·메뉴로 한 단계씩                           | 공통 위키·모듈 문서               | 제안(기존 `PUT /api/documents/{id}/parent`)                       |
| 백링크 → 링크 자리로 스크롤 (Outline)                        | 문서 처음으로 연다                                         | 공통 문서                         | 제안                                                              |
| 기록: 본문 안 바뀐 곳 강조 (Outline)                         | 리비전 줄 단위 비교·복원(17절)                             | 공통 문서                         | 관찰(같은 목적은 이미 충족)                                       |
| 문서 아래 하위 문서 목록 (Outline)                           | 트리로만                                                   | 공통 위키                         | 제안(작음)                                                        |
| 안내 상자·접기 블록·쪽 나눔·오늘 날짜 (Outline)              | 문단·제목·목록·체크리스트·표·코드·수식·구분선·이미지·첨부  | 공통 편집기                       | 범위 밖(새 블록 종류는 D-27 목록 밖, 내보내기·Markdown 왕복 영향) |
| 검색 거르기: 작성자·기간·제목만 (Outline)                    | 모듈·담당·상태·날짜 거르기(38절)                           | 공통 검색                         | 제안(작성자·제목만)                                               |
| 템플릿 보드(회의·스프린트·회고 등) (Focalboard)              | 항목 템플릿·문서 템플릿(43·47절)                           | 프로젝트·스터디                   | 관찰(이미 있음)                                                   |
| 표 보기 + 개수 집계 (Focalboard)                             | 목록 보기                                                  | 공통 업무                         | 관찰                                                              |

### 86.8 남은 문제와 재현

- 저장 요청이 응답 없이 멈출 때 안내가 없던 것은 86.9에서 고쳤다. 요청 시간 한도(브라우저·앞단 기본값)는 API 공통 동작이라
  바꾸지 않았다. 그래서 서버가 계속 멈춰 있으면 카드는 "응답 지연"인 채 남고, 다시 확인을 눌러도 목록을 읽지 못한다.
- 문서 옮기기에서 검색으로 상위 문서를 고르는 창(Outline "Move"와 같은 일)은 제안으로 남긴다. 지금은 Alt+방향키·메뉴로 한 단계씩.
- 백링크를 눌렀을 때 링크가 있는 자리로 스크롤하지 않는다(문서 처음으로 연다).
- 검증하지 못한 것: 실제 Zotero 연동(계정 없음), 실제 휴대 기기, 실제 창 전환 중 끌기(자동화로 일으킬 수 없어 시험으로 대신).
  강의의 배속 기준 예상 시간은 미구현(83절).

### 86.9 칸반 이동의 저장 상태

사용자 지시: 낙관적 이동과 서버 저장 확정을 나누고, 시간 초과·취소만으로 실패라고 단정하지 않으며, 같은 카드의 중복 이동만 막고,
결과를 모르면 서버 상태를 다시 읽어 대조한다. 이동 API는 멱등이 아니므로 자동으로 다시 보내지 않는다. API를 바꿔야 하면 먼저 보고한다.

API는 바꾸지 않았다. `PUT /api/tasks/{id}/position`(`{status, before_id}`)에는 요청 id도 판 조건도 없다. 서버 처리
(`features/tasks.rs` `move_task`, 주석 "버전과 상관없이 바로 반영한다")와 `app_move_task`(0012)는 판을 받지 않고 행을 잠근 뒤 그대로 옮긴다.
원자적으로 거부하는 것은 `before_id`가 목표 열의 같은 항목 업무가 아닐 때(400)뿐이라, 옆 카드가 다른 열로 간 경우만 막고 이 카드가 그사이
옮겨지거나 고쳐진 경우는 막지 않는다. 그래서 같은 요청을 다시 보내면 그사이 다른 사람이 옮긴 것을 덮을 수 있고, 이는 처음 옮길 때도
같다(D-79의 마지막 저장이 이기는 이동). 다시 보내기는 사용자가 누를 때만, 서버 목록을 다시 읽어 이 요청이 반영되지 않았음을 확인한
뒤에만 한다. 대조는 `app_move_task`가 열이 바뀔 때만 판(`version`)을 올리고 같은 열 안의 순서는 올리지 않는 것에 맞춰 열·바로 뒤
업무·판을 함께 본다(`move-sync.ts`).

| 상태(카드 표시)      | 언제                                                                                     | 사용자가 할 수 있는 것                     |
| -------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------ |
| 저장 중…             | 옮기는 즉시(알림 영역 `role="status"`에도)                                               | 다른 카드는 그대로 옮길 수 있다            |
| 응답 지연            | 3초가 지나도 응답이 없을 때. 결과를 모른다고만 알린다                                    | 다시 확인                                  |
| 확인 중…             | 10초가 지나면 응답을 기다리면서 목록을 읽어 대조, 또는 결과를 알 수 없는 오류 직후       | —                                          |
| 저장 여부 확인 못 함 | 응답 없이 끝났고 목록도 읽지 못함. 카드는 옮긴 자리에 두고, 다음에 목록을 읽을 때 정한다 | 다시 확인                                  |
| 저장되지 않음        | 응답 없이 끝났는데 서버 목록이 이동 전 자리·판 그대로                                    | 다시 시도, 그대로 두기(서버 자리로 되돌림) |
| (표시 사라짐)        | 성공 응답, 또는 대조 결과 반영됨·다른 곳에서 바뀜·없어짐. 알림으로 결과를 알린다         | —                                          |

- 오류 나누기: 응답이 없음(`NetworkError`), 앞단이 대신 답한 502·503·504(`unreachable`), 예상 밖 응답(`unexpected`)은 "알 수 없음"으로
  대조하고, 앱 형식의 4xx·5xx는 트랜잭션이 커밋되지 않았으므로 실패로 그 카드만 서버 자리로 되돌린다(403·409는 기존처럼 화면을 다시 읽음).
  커밋 직후 응답만 실패하는 드문 경우도 앱 5xx로 보이므로 실패로 처리되며, 다음 목록 읽기에서 서버 상태로 맞춰진다.
- 같은 카드는 저장이 끝날 때까지 끌기·옮기기 메뉴·Alt+방향키를 막고(알림으로 이유를 알림), 다른 카드는 막지 않는다. 다시 읽은 목록 위에
  아직 저장 중인 다른 카드의 이동을 다시 얹어 되돌아가 보이지 않게 한다.
- 다시 시도는 목록을 다시 읽어 여전히 반영되지 않았을 때만 보낸다. 목표 업무가 그 열에 없으면 열 끝으로 바꾸고, 읽은 목록에서 이미 다른 곳에서
  바뀌었으면 보내지 않고 서버 상태를 보인다. 다시 보내는 동안 그 카드의 표시를 지우지 않아 같은 카드가 두 번 보내지는 틈이 없다.
- 읽은 뒤 보내기 전 사이의 변경은 막지 못한다(위 API 한계). 대신 응답의 판이 이 이동만으로 될 판(열이 바뀌면 읽은 판 + 1, 아니면 그대로)보다
  크면 그사이 다른 변경이 끼어 이 이동이 그 위에 덮였다고 보고, "저장했다"고 하지 않고 "그 변경을 덮었을 수 있습니다"를 알림 영역(`role="alert"`)에
  남긴 채 서버 목록을 다시 읽어 보인다. 자동으로 되돌리지 않는다(되돌리기도 같은 조건 없는 이동이라 또 덮을 수 있다).
  한계: 다른 사람이 같은 열 안에서만 순서를 바꾼 경우는 판이 오르지 않아 알 수 없고, 다른 사람이 같은 목표 열로 옮긴 경우도 판 차이로 구별되지
  않는다(결과 열은 같고 순서만 이 이동이 이긴다). 다른 사람이 제목만 고친 경우는 잃은 것이 없어도 알린다(보수적). 처음 옮기기는 이 확인을 하지
  않는다(낡은 화면에서 옮기면 다른 편집에도 알림이 떠서 범위를 넓히지 않음). 판 조건을 붙인 이동(예: `base_version`, 어긋나면 409)이 있어야만
  막을 수 있으며 이는 API 계약 변경이라 사용자 결정으로 남긴다.
  **이후(D-89, 87절):** 사용자가 승인해 이동 판 조건을 더했다. 다시 시도·되돌리기는 읽은 이동 판을 조건으로 보내고 서버가 같은
  트랜잭션에서 거절하므로, 위의 응답 판 비교와 "덮었을 수 있습니다" 안내는 없앴다. 대조는 열·바로 뒤 업무·이동 판으로 한다.
- 늦게 온 응답·목록은 차례 번호(카드별 `seq`, 목록 읽기 `reloadSeq`)로 거른다. 화면을 떠나거나 다른 항목으로 바뀌면 타이머를 모두 지운다.
- 같은 카드의 대조가 겹치면(10초 대조가 목록을 읽는 사이 요청이 응답 없이 끝나 다시 대조) 마지막 대조만 따르고, 판정에는 목록을 읽기 시작할
  때 요청이 진행 중이었는지를 쓴다. 그래서 요청 중에 읽은 목록이 늦게 와도 "저장되지 않음"으로 정하지 않는다. 명시적 실패의 안내는 다시 읽기
  전에 정해, 그사이 성공한 같은 카드의 다음 이동 안내를 늦게 덮지 않는다(86.11, CodeRabbit 지적).

재현과 확인(7가지 상황과 다시 시도 사이의 동시 수정). 시험은 `apps/web/tests/component/task-move-sync.test.ts`에 있고, 컴포넌트 시험 11건은 저장 상태를 고치기 전 코드에서, 동시 수정 시험 1건은 판 확인을 넣기 전 코드에서, 겹친 대조·실패 안내 순서 시험 2건은 `3cb596b` 전 코드에서, 빠른 추가 시험 3건은 각 고침 전 코드에서 실패한다(모두 22건).
브라우저는 이 브랜치의 개발 설치(`cargo xtask dev --oidc`, 예제 워크스페이스)에서 확인했다. 서버 정지는 이 작업이 띄운 개발 서버 프로세스에만
했고, 정지 전에 정해진 시간 뒤 `kill -CONT`를 먼저 걸어 두어 어떤 경우에도 재개되게 했다.

| 상황                              | 시험 | 브라우저                                                                                                                                                                                       |
| --------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 정상 저장                         | PASS | PASS: 저장 중 → 표시 사라짐                                                                                                                                                                    |
| 느린 성공                         | PASS | PASS: 서버 `kill -STOP` → 0초 저장 중, 3초 응답 지연, 10초 확인 중 → `kill -CONT` 뒤 저장을 확인했습니다                                                                                       |
| 명시적 실패                       | PASS | 이전 판 `50806b2`에서 확인(86.1, 요청 실패 주입 → 되돌림·안내 → 다시 시도). 이번 판은 시험으로 대신                                                                                            |
| 응답 유실, 실제로는 저장됨        | PASS | PASS: 요청은 보내고 응답만 버리게 `fetch`를 감싸 → 확인 중 → 저장을 확인했습니다                                                                                                               |
| 응답 유실, 저장 안 됨             | PASS | PASS: 보내지 않고 실패하게 감싸 → 저장되지 않음, 다시 시도·그대로 두기 → 다시 시도 → 저장                                                                                                      |
| 연속 이동(같은 카드·다른 카드)    | PASS | 시험으로 대신(같은 카드는 막히고 다른 카드는 옮겨짐, 늦은 목록은 무시)                                                                                                                         |
| 화면 이탈                         | PASS | PASS: 서버 정지 중 다른 화면으로 이동 → 새 오류 없음, 재개 뒤 서버에 이동 반영                                                                                                                 |
| 다시 읽은 뒤 확정                 | PASS | PASS(위 응답 유실 두 경우). 목록을 읽지 못한 뒤 "확인 못 함"이 남고 다시 확인으로 정해지는 것은 시험으로 대신                                                                                  |
| 다시 읽은 직후 다른 사용자가 수정 | PASS | 시험으로 대신(다시 시도 직전 목록은 이동 전 판 1, 응답은 판 3 → 덮었을 수 있음 안내, 확정 문구 없음, 목록 다시 읽음). 이 브랜치의 개발 서버는 main 병합 전 코드·DB라 브라우저 재현은 하지 않음 |

스크린샷(저장소 밖, 세션 작업 폴더 `refcompare/shots/groundzero/`): `18-move-saving`, `19-move-slow-3s`, `20-move-server-paused-checking-10s`,
`21-move-after-resume-confirmed`, `22-response-lost-but-saved-confirmed`, `23-not-saved-retry-keep`, `24-after-retry-saved`.

### 86.10 Supabase PR #33~#35와의 통합 확인

이 브랜치는 main `f9700d7`에서 시작했고 그 뒤 main에 #33·#34·#35(D-88, 앱이 만드는 UUIDv7)가 병합됐다. 원격 상태(2026-10-20T17:23Z 읽음):
#33 `7a74455`, #34 `b1c5849`, #35 `096264d1a3d127d4081d0853aa83bf8db133594a` 모두 MERGED, origin/main `726343c0b3020f8a990c093ae3a5dfc264ea26a9`.
이 브랜치·PR 브랜치를 바꾸지 않고 저장소 밖 임시 worktree(`tmp/int-ui-main`, 올리지 않음)에서 main `726343c`에 이 브랜치를 병합해 확인했다.

- 문서 충돌: main이 85절(D-88·Supabase)을 먼저 썼다. 병합본에서는 main의 85절을 그대로 두고 이 절을 86절로 바꾸며 절 안의 85.x 참조, 12.1의
  "85절", `reference-products-survey.md`의 85절·85.2·85.3·85.7 참조를 함께 고쳤다. main의 85.1 참조(12.1, `docs/sql-id-contract.md` 앵커)는 그대로다.
- 예제 seed: 이 브랜치의 v2 블록은 옛 서명(`app_create_task` 4인자, `app_update_task` 13인자, `app_create_wiki_document` 7인자,
  `app_record_document_references` 2인자)과 DB의 `uuidv7()`을 쓴다. main의 0075가 옛 서명을 지웠으므로 main과 합치면 v2는 경고를 남기고 건너뛴다
  (예외 처리 때문에 개발 실행은 멈추지 않는다). 병합본에서는 v1과 같은 방식으로 xtask가 넘긴 id 목록(`pg_temp.take1()`·`take(n)`, 개수는 각 함수
  머리말과 `app_update_task_ids`·`app_record_document_references_ids`)을 쓰게 고쳤고 DB에서 id를 만드는 대체 경로는 넣지 않았다.
  이 고침은 옛 서명이 있는 이 브랜치에는 넣을 수 없으므로 main을 이 브랜치에 합칠 때 같은 커밋에 넣어야 한다.
- 사용자 지시로 main `726343c`를 이 브랜치에 합쳤다(`5e63643`). seed·조사 문서·이 문서는 임시 브랜치 `b0834ab`와 같게 풀었고,
  `git diff b0834ab 5e63643`은 86.6의 한 문단(`2af0416`)뿐이다. 그래서 임시 브랜치에서 고친 것은 모두 이 브랜치에 들어 있다.

통합 검증(병합본 `8454f1d` = main `726343c` + 이 브랜치 `1588b65`). 저장소 밖 임시 worktree의 개발 설치를 따로 띄워(Compose 프로젝트
`fvoci-groundzero-int-…-dev`, 서버 127.0.0.1:8082, Keycloak 8182, 새 DB) 확인했고, 이 브랜치의 개발 DB와 비교 환경은 쓰지 않았다.

| 검사                                   | 결과    | 내용                                                                                                                                     |
| -------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 옛 서명이 없음                         | 확인    | `to_regprocedure`로 옛 4개 서명은 없고 새 `uuid[]` 서명은 있음                                                                           |
| 새 DB에서 v1+v2 한 번에                | PASS    | 경고 없음. v2 표시 `tasks_added` 12, 위키 4건(운영 안내 › 설치 › 백업, 점검 기록) 부모 관계 맞음, 백링크 3건                             |
| 다시 실행                              | PASS    | 개발 실행을 다시 띄워 준비 SQL을 다시 돌림: 워크스페이스·업무·문서·링크·항목·모듈·감사·`instance_state`·알림 행 수가 모두 같음           |
| 기존 행 보존                           | PASS    | 예제 업무 제목을 고친 뒤 다시 실행: 고친 제목 유지, 행 수 같음                                                                           |
| 지운 예제 워크스페이스를 되살리지 않음 | PASS    | `app_delete_workspace`로 지우고 v2 표시를 지운 뒤 다시 실행: v2 표시가 `skipped`로만 생기고 그 밖의 행 수 같음, 워크스페이스는 지운 상태 |
| `cargo xtask check`(`8454f1d`)         | PASS    | 건너뛴 단계 없음. Rust 단위 69건, 웹 시험 56개 파일 259건, svelte-check 2952개 파일 오류·경고 0, 라이선스 포함                           |
| `cargo xtask test --oidc`(`8454f1d`)   | PASS    | 45개 묶음 255건, 실패·무시 0. NOT RUN 줄은 Supabase 시험(`supabase_*`) 하나                                                              |
| `cargo xtask test --supabase`          | NOT RUN | 이번 변경은 UI·개발 seed·문서뿐이고 Supabase 구성·시험은 main 그대로. 개발 seed는 Supabase 시험이 쓰지 않는다                            |

위 표는 임시 브랜치(`8454f1d`)의 기록이다. main을 이 브랜치에 합친 뒤의 결과는 86.6에 SHA별로 적는다.

### 86.11 CodeRabbit 검토 처리

CodeRabbit은 push마다 바뀐 부분을 자동 검토했고(`5caedc9`·`acc544d`·`2af0416`·`b722918`), 사용자 지시로 `b722918`에 전체 검토를 요청했다
(검토 범위 main `726343c`..`b722918`, 15개 파일, 2026-10-20T18:24Z 완료), 고친 뒤 `ccfe6d8`에 변경분 검토(`@coderabbitai review`, 18:51Z 완료)를 요청했다. 지적마다 지금 코드에서 다시 확인했다.

| 지적(검토 SHA)                                                                                  | 지금 코드에서                                  | 처리                                                                                                                                                                          | 시험·확인                                                                              |
| ----------------------------------------------------------------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| 담당자 목록 요청 실패 시 "불러오는 중…"이 남음(`5caedc9`)                                       | 유효                                           | `3cb596b`: 담당자 목록 실패를 따로 받아 로딩을 끝내고 그 영역에 경고. 업무 불러오기 실패 문구는 쓰지 않음                                                                     | 컴포넌트 시험 1건(고치기 전 실패)                                                      |
| seed 주석 "열마다 두 장 이상"이 프로젝트 밖 모듈과 다름(`5caedc9`)                              | 유효(설명이 사실과 다름)                       | `3cb596b`: 예제 수는 그대로 두고 seed 주석·12.1 설명을 "프로젝트는 열마다 둘 이상, 나머지는 한 장"으로 고침. 같은 열 안 순서 바꾸기는 프로젝트로 충분해 예제 행을 늘리지 않음 | seed 정의 대조(v1·v2의 모듈별 업무 목록)                                               |
| 조사 문서 작성일이 미래(`acc544d`)                                                              | 결함 아님(한국 시간 10-21, 검토기는 UTC 10-20) | `3cb596b`: 날짜가 한국 시간 기준임을 적음                                                                                                                                     | —                                                                                      |
| 요청 중 읽은 목록으로 "저장되지 않음"을 정할 수 있음(`2af0416`)                                 | 유효                                           | `3cb596b`: 같은 카드의 겹친 대조는 마지막 것만 따르고, 판정은 읽기 시작 때의 진행 여부로                                                                                      | 컴포넌트 시험 1건(고치기 전 실패)                                                      |
| 문서 절 번호 충돌(`2af0416`)                                                                    | 그 SHA에서 유효, 지금은 해결                   | `5e63643` main 병합에서 main 85절 유지, 이 절 86절                                                                                                                            | `git diff b0834ab 5e63643`                                                             |
| seed v2 옛 함수 서명(`2af0416`)                                                                 | 그 SHA에서 유효, 지금은 해결                   | `5e63643`: D-88 서명과 `pg_temp.take*`                                                                                                                                        | 격리 DB 네 경우(86.6)                                                                  |
| `/` 메뉴 구분선을 편집기 호출 경로로 시험(`5caedc9` 사소)                                       | 유효(시험 공백)                                | `3cb596b`: 실제 편집기에서 `/구분`을 골라 아래 체크리스트가 남는지 보는 시험                                                                                                  | 메뉴 항목을 옛 명령으로 바꾸면 실패함을 확인                                           |
| 명시적 실패 안내가 뒤에 성공한 이동 안내를 덮음(`b722918`)                                      | 유효                                           | `3cb596b`: 안내를 다시 읽기 전에 정함                                                                                                                                         | 기존 시험에 경고 없음 단언 추가(고치기 전 실패)                                        |
| 구분선과 빈 문단이 트랜잭션 두 번(전체 검토)                                                    | 유효                                           | `f61a3eb`: 한 명령 묶음으로(변경 알림 한 번)                                                                                                                                  | 시험 1건(고치기 전 알림 2번)                                                           |
| 지운 프로젝트 항목을 설치 문서가 참조(전체 검토)                                                | 유효(사소)                                     | `f61a3eb`: 항목이 있을 때만 참조, 없으면 글만                                                                                                                                 | 격리 DB에서 항목을 지운 뒤 v2를 되돌리는 트랜잭션 안에서 실행: 링크 0건·글만, 업무 8건 |
| 빠른 추가로 다른 열에 만든 카드가 확인 실패 중 목록에 없음(`ccfe6d8` 변경분 검토, 범위 밖 지적) | 유효                                           | `3346f24`: 만든 업무를 "할 일" 끝에 먼저 넣은 뒤 옮겨 그 카드에 저장 상태·다시 확인을 보임                                                                                    | 컴포넌트 시험 1건(고치기 전 실패)                                                      |
| 만들기 전에 시작한 목록 읽기가 늦게 와서 새 카드를 지움(`718ea42` 변경분 검토)                  | 유효(좁은 경우)                                | `1a1575d`: 새 카드를 넣을 때 그 전에 시작한 목록 읽기를 무효로 함                                                                                                             | 컴포넌트 시험 1건(고치기 전 실패)                                                      |
| 만드는 사이 보드를 떠나도 빠른 추가가 이어서 옮기고 타이머를 남김(`32f2e5f` 변경분 검토)        | 유효(사소)                                     | `4d2ec2d`: 만든 뒤 보드가 없거나 항목이 바뀌었으면 옮기지 않음. 업무는 이미 만들어져 "할 일"에 남는다(화면을 떠나면 늦은 일을 버리는 86.9 원칙)                               | 컴포넌트 시험 1건(고치기 전 실패)                                                      |

상충: 없음. seed 예제 수 지적은 두 수정안 가운데 설명을 고치는 쪽을 골랐다(검증된 예제 수를 바꾸지 않음). 검토 지적 밖에서 이 작업이 찾은 것:
`f61a3eb` 첫 판에서 seed v2의 괄호를 하나 더 닫아 격리 DB 실행이 문법 오류로 멈췄고(`cargo xtask check`는 SQL을 실행하지 않아 못 잡음),
같은 커밋을 고쳐 다시 확인했다(올리기 전).

## 87. 칸반 이동의 조건부 저장 (2026-10-21, D-89, `feat/move-conditions`, 시작 `feat/survey-flows` `367e918`)

사용자 지시: 조건부 이동 API를 만들고, 같은 열 안에서 순서만 바뀌어도 충돌을 알도록 이동 판 번호를 일관되게 올린다. 조건 비교와 이동은 한
트랜잭션에서 하고, 어긋나면 아무것도 바꾸지 않고 409를 돌려준다. 웹은 최신 상태를 다시 보이되 자동으로 다시 보내지 않는다. 다시 시도·되돌리기는
조건을 반드시 보내고, 조건 없는 기존 호출은 그대로 동작한다.

- 데이터(`0076_move_conditions.sql`): `tasks.move_version integer NOT NULL DEFAULT 0`. `BEFORE UPDATE` 트리거가 `status`나 `position`이 바뀔
  때마다 1 올리고, 그 밖의 갱신에서는 손으로 바꾼 값도 이전 값으로 되돌린다. 그래서 이동 함수·필드 고치기의 상태 바꾸기·열 다시 매기기
  (`app_renumber_lane`) 어느 길로 바뀌어도 같다. 필드 고치기의 판(`version`)과는 따로 둔다: 같은 열 순서 바꾸기는 상세 편집의 충돌을 만들지 않는다.
- 함수: `app_move_task(uuid, text, uuid, integer)`. 기존 3인자 함수는 지웠다(호출하는 곳은 서버 한 곳). 대상 행을 잠그는 기존 순서
  (`app_lock_task_rows`, `FOR NO KEY UPDATE`) 다음에 넘겨받은 판과 비교하고, 다르면 `FV003`을 올려 트랜잭션 전체가 되돌아간다. 판이 `NULL`이면
  비교하지 않는다(조건 없는 이동, D-79).
- API: `PUT /api/tasks/{id}/position`의 본문에 선택 필드 `base_move_version`. 어긋나면 409 `{"error":"move_conflict","current":{…}}`
  (필드 고치기의 409와 같은 모양, 지금 업무는 새 트랜잭션에서 호출자 권한으로 읽음). 업무 응답에 `move_version`이 더해졌다. 주지 않는 호출은
  응답에 필드가 하나 늘어난 것 말고는 같다.
- 웹: 다시 시도와 되돌리기는 보내기 직전에 읽은 목록의 이동 판을 조건으로 보낸다. 409면 그사이 다른 곳에서 옮겨져 저장하지 않았다는 안내(다시 시도는 "다시 시도를 저장하지 않았습니다",
  되돌리기는 "되돌리지 않았습니다")를 알림 영역에 남기고 목록을 다시
  읽어 보이며, 다시 보내지 않는다(항목은 다시 읽지 않음: 항목의 보관·권한이 바뀐 것이 아니다). 처음 옮기기는 지금처럼 조건 없이 보낸다(낡은
  화면에서 옮기면 다른 편집에도 거절되어 D-79 범위를 넓히지 않음). 대조(`move-sync.ts`)는 열·바로 뒤 업무·이동 판으로 하므로 다른 사람이
  같은 열 안에서만 순서를 바꾼 것도 "다른 곳에서 바뀜"으로 구별한다. 86.9의 응답 판 비교와 "덮었을 수 있습니다" 안내는 없앴다.
- 시험: 통합 시험 `conditional_moves_refuse_a_place_changed_since_it_was_read`(다른 사람이 옮긴 뒤 낡은 판 → 409·상태 그대로, 같은 열 순서
  바꾸기는 이동 판만 오르고 필드 판은 그대로, 응답 유실 뒤 같은 요청 다시 보내기 → 409·중복 이동 없음, 되돌리기 충돌, 필드 고치기의 상태
  바꾸기도 이동 판을 올림, 같은 판으로 동시에 두 이동 → 하나만 200, 조건 없는 이동은 그대로). 웹 컴포넌트 시험: 다시 시도·되돌리기가 읽은
  판을 보내고 거절되면 다시 보내지 않음, 처음 옮기기는 조건 없음.

## 88. 간트의 선행 관계 연결선 (2026-10-21, D-31, `feat/gantt-dependencies`, 시작 main `90edd23`)

사용자 지시: 이미 있는 선행 관계(`predecessor_ids`, 끝나야 시작 하나)를 간트에 연결선과 화살표로 보인다. 새 관계 종류·선으로
관계 만들기·일정 계산·권한은 바꾸지 않고 D-31의 미리보기·승인 경로를 그대로 둔다. API·DB·마이그레이션 변경 없음.

- 적용: 공통 `GanttChart`(항목 업무의 간트 보기). 간트가 있는 모듈(프로젝트·연구·스터디·취미)에 모두 같다.
- 좌표(`gantt.ts` `ganttLinks`, 순수 함수): 막대와 같은 rem 좌표계를 쓴다. 줄 높이를 1.75rem으로 정해(격자 줄 크기) 열 너비와 같은
  단위가 되게 했고, 연결선 층은 막대 줄들과 같은 격자 칸을 덮는 SVG(viewBox 단위 하나가 1rem)다. 선행 막대의 끝(끝 열 포함, 여백
  0.125rem 안쪽)에서 후속 막대의 시작으로 잇고, 마일스톤은 ◆ 꼭짓점 밖에서 잇는다. 날짜를 더하거나 빼지 않는다. 경로는 직각이다:
  후속이 뒤면 선행 끝에서 조금 나가(바로 다음 날처럼 틈이 좁으면 틈 가운데에서) 꺾어 들어가고, 같은 날·겹침·역전이면 선행 줄의
  위나 아래 경계선을 따라 돌아가 후속 시작 앞에서 들어간다. 마지막 구간은 늘 왼쪽에서 들어가며 화살촉이 후속 쪽이다. 표의 양 끝을
  넘지 않는다. 같은 관계는 한 번만, 자기 참조·목록이 빠진 데이터는 건너뛴다.
- 그리지 않는 경우: 한쪽 끝이 지금 그린 줄에 없으면(거르기, 날짜 없음, 표시 기간 뒤, 볼 수 없는 업무) 다른 업무나 가장자리에 잇지
  않는다. 끝이 표시 기간 뒤라 잘린 막대에서는 진짜 끝을 모르므로 잇지 않는다(`ganttLayout`이 `clippedEnd`를 표시). 이렇게 빠진 관계
  수를 간트 아래에 "관계 n개는 … 그리지 않았습니다"로 알린다. 이 간트가 받은 업무의 선행 목록으로만 세며, 숨은 업무의 제목·존재를
  알아내려고 따로 읽지 않는다(거르기로 빠진 후속 업무의 관계는 알 수 없어 세지 않음). 끄는 중의 미리 보기로 끝이 표 밖(첫 열
  앞·마지막 열 뒤)으로 나가도 같다: 가장자리에 붙여 잇지 않고 센다. Esc로 취소하거나 놓아 저장하면 다시 그린다.
- 겹침·조작: 선은 막대(z-2) 아래, 주말 음영·오늘 선 위에 그린다. 줄 이름 열은 sticky라 가로 스크롤 때 선이 그 아래로 지나간다.
  연결선 층은 `pointer-events-none`이라 막대 끌기·양 끝 조절·줄 단추 열기·가로 스크롤이 그대로다. 줄 번호를 격자에 명시했다
  (연결선 층이 같은 칸을 덮을 때 격자 자동 배치가 줄을 아래로 미는 것을 막음, 첫 화면 확인에서 찾음).
- 상태 일치: 끄는 중이거나 승인을 기다리는 막대는 미리 보기 자리(`draft`)로 선을 잇는다. 끌기 취소·미리보기 취소·승인·서버 실패 뒤에는
  다시 읽은 업무로 막대와 선이 함께 그려진다. 선은 저장 요청을 만들지 않는다.
- 접근성: SVG는 `aria-hidden`·`focusable="false"`(탭 정지점 없음). 선행 관계는 줄 단추의 설명("선행 KED-…")이 그대로 말한다. 줄 이름에
  초점을 두거나 가리키면 그 업무의 선을 굵게(2.5px)·진하게, 나머지를 옅게 한다(색만이 아니라 두께와 진하기). 가리킴과 초점은 따로
  두고 가리킨 줄을 먼저 따른다(가리켰다 떼도 초점이 있는 줄의 강조가 남음, `ef8b201` 뒤 고침). 선 색은 `foreground`
  기반이라 밝은·어두운 토큰 모두에서 배경·격자·오늘 선(`primary/40`, 세로 1px)과 구별된다. 앱에는 아직 어두운 화면 전환이 없어 `.dark`
  토큰을 페이지에 붙여 확인했다.
- 시험: 순수 함수 10건(단일·연쇄·여러 선행·후속·중복, 위·아래 줄, 바로 다음 날, 같은 날·역전·왼쪽 끝, 마일스톤, 거르기·날짜 없음, 표
  밖으로 끈 끝, 잘린 막대, 이상한 데이터, 줄 순서·열 너비), 보드 컴포넌트 8건(탭 정지점·낭독 중복 없음·줄 열기, 초점·가리킴 강조, 끄는
  중·Esc 취소와 저장 요청 없음, 후속을 첫 열 앞으로 끄는 중에는 선 없음·Esc로 되돌림, 미리보기 승인·취소·서버 실패 뒤 막대와 선 일치,
  그리지 못한 관계 안내, 거르기로 한쪽만 남음, 영어 단수·복수 안내).
- CodeRabbit 처리: 전체 검토(`10e1c71`) 지적 1건(영어 "1 dependencies")은 `ef8b201`에서 기존 복수 형식으로 고쳤다. `1d6c5e5` 검토의
  인라인 지적(잘린 선행 막대를 끄는 중의 선)은 전제(`draft.to >= layout.days`로 `clippedEnd` 갱신)가 코드에 없고 잘못 그리는 경우가 없어
  바꾸지 않았다(끄는 중에는 저장 전까지 선이 숨는 차이만 있음). 같은 검토 본문의 범위 밖 지적(후속을 첫 열 앞으로 끌면 선이 왼쪽
  가장자리에 붙고 화살촉이 뒤집힘)은 화면에서 재현해 유효함을 확인했다(합성 입력, 아래). 제안(`x2`를 0으로 제한)은 화면 가장자리에
  화살표를 남겨 이 절의 "가장자리에 잇지 않음" 규칙과 맞지 않아, 표 밖의 끝을 그리지 않는 것으로 고쳤다(`6cd42d2`, 시험 2건은 고치기 전
  실패).
- 화면 확인(격리된 개발 DB, 관계는 API로 추가): 데스크톱 1280·1024, 좁은 화면 375(브라우저 흉내, 실제 기기 아님), 가로 스크롤,
  루트 글꼴 20px(확대), 제목순 정렬, 거르기, 밝은·어두운 토큰, 끌어서 저장, 미리보기 창·취소·승인. 각 상태에서 막대 경계와 선 끝의 화면
  좌표 차이 0.5px 이하. 입력 방식: 끌어서 저장·미리보기 창은 자동화 도구의 마우스 끌기(브라우저 입력 경로, 앱의 `setPointerCapture`
  그대로), 초점 강조는 도구의 Tab 키다. 경계 밖 끌기의 재현·수정 확인은 끄는 중 상태를 붙잡으려고 페이지 안에서 `PointerEvent`를 직접
  보내고 그 페이지에서만 `setPointerCapture`를 빈 함수로 임시 대체한 합성 입력 시험이다(제품 코드에는 바꾼 것이 없고, 같은 대체는 jsdom
  시험에만 있다). 사람이 실제 마우스·터치로 조작한 확인은 하지 않았다.

## 89. 간트의 주 머리글 (2026-10-21, D-90, `feat/gantt-weeks`, 시작 #39 `7bfc89a`)

- 머리글은 월 → 주 → 날짜 세 줄이다. 세 줄과 막대 줄은 같은 열(날짜 번호 → rem) 좌표를 쓰고, 막대 줄 번호는 4부터다(연결선 층도).
- 주 칸(`gantt.ts` `ganttWeeks`, 순수 함수): 보이는 날짜 열을 사용자 한 주 시작(D-81)으로 나눈다. 표가 주 중간에서 시작·끝나면 보이는
  열 수만큼만 칸을 차지하고, 표기는 그 주의 실제 첫날·끝날을 쓴다. 월요일 시작이면 `isoWeek`로 ISO 주차 연도·번호를 붙인다(주의
  목요일이 속한 연도). 일요일 시작이면 번호 없이 날짜 범위만(D-90).
- 표기: 칸 폭이 10rem 이상이면 "2026년 43주 · 10/19–10/25", 4.5rem 이상이면 "43주 · 10/19~"(일요일 시작 "10/18~"), 그보다 좁으면 비운다.
  화면 낭독기용 글과 title은 늘 전체 설명("2026년 43주(ISO 주차, 월요일 시작): 2026년 10월 19일부터 …")이다. 보이는 날짜 영역
  (스크롤 영역 - 줄 이름 14rem)이 12rem보다 좁으면(휴대 폭) 칸이 넓어도 짧은 표기를 쓴다.
- 가로 스크롤: 월·주 표기는 `sticky left-56` 글자라 칸 앞부분이 줄 이름 열 아래로 들어가도 그 옆에 붙어 남는다. 칸은
  `overflow-clip`이어야 한다(`truncate`의 `overflow: hidden`은 스크롤 상자가 되어 sticky가 칸 안에서만 움직인다). 주 칸은
  `relative`라 sr-only 글이 스크롤 영역 밖으로 빠져 페이지를 넓히지 않는다.
- 브라우저 확인(내장 브라우저, 합성 데이터, `0c7b4e7`): 1280×800 월요일·일요일 시작의 연말 범위에서 주 칸·날짜 칸 오차 0px,
  경계선 어긋남 0, 페이지 가로 넘침 없음(페이지 스크립트 측정). 어두운 테마(`dark` 클래스), 375×812 에뮬레이션(실제 기기 아님),
  루트 글꼴 20px도 봤다. 스크롤은 페이지 스크립트로 바꾼 합성 조작이고 실제 포인터·터치·화면 낭독기는 시험하지 않았다.
- 경계: 주 경계는 머리글 날짜 칸의 진한 왼쪽 선과 막대 줄의 가는 세로선, 월 경계는 두꺼운 선(머리글 2px, 막대 줄 0.5)이다. 오늘은
  날짜 칸의 채운 원과 primary 세로선이라 주·월 경계와 굵기·모양이 다르다.
- 시험: `gantt-weeks.test.ts` 10건(요일, ISO 주차의 보통 주·연말·연초·53주·윤년, 월요일·일요일 주 칸의 부분 주·월·연 경계, 머리글
  렌더링과 열 정렬·짧은 표기·경계 선, 일요일 시작의 번호 없는 표기).

## 90. 선행 관계의 종류와 지연일 (2026-10-21, D-91, `feat/dependency-kinds`, 시작 #40 `60fb2ad`)

- 스키마(`0077_dependency_kinds.sql`): `task_dependencies`에 `kind`(FS·SS·FF·SF, 기본 FS)와 `lag_days`(0~3650, 기본 0)를 더했다.
  있던 행은 기본값으로 FS·0이 되고 기본 키(선행, 후속)는 그대로라 업무 쌍마다 하나, 새 id 없음. RLS·권한(`fvoci_app` 읽기만, 쓰기는
  정의자 함수)도 그대로다. 항목 복제 함수를 다시 만들어 종류·지연일을 복사한다(본문은 0072와 같고 관계 복사만 다름).
- 계산(`schedule.rs` `plan_shifts`): 변경은 날짜 옮기기 또는 관계 만들기·고치기다. 바뀐 곳에서 닿는 업무를 위상 순서로 보며, 업무마다
  이번 변경이 닿은 관계(선행이 옮겨졌거나 그 관계를 고침)의 "지금 요구 - 전부터 요구하던 것(채웠으면 0)" 중 가장 큰 만큼 민다(기간
  유지). 원인은 그 최댓값을 낸 관계(여럿이면 모두). 제약식과 SF의 하루 차이는 D-91.
- 저장할 때 다시 계산: 업무 고치기·관계 설정은 DB 함수가 기준 버전·후속 여부·날짜 모양을 확인한 뒤, 같은 트랜잭션(행·항목·관계
  잠금이 잡힌 채)에서 Rust가 옮기기 전 날짜(잠그기 전에 읽었지만 기준 버전이 맞으면 같은 값)와 바꾸기 전 관계(DB 함수가 잠근 뒤 읽어
  돌려줌)로 계획을 다시 세워 승인 목록과 비교한다. 다르면 트랜잭션을 버리고 409 `shift_conflict`. `version` 올리기로는 이 경우를 못
  잡는다: 그사이 새로 이어진 업무는 승인 목록에 없어 판을 보지 않는다(advisor 검토, 시험
  `changing_a_link_previews_causes_and_moves_approved_tasks_in_one_transaction`).
- API: `Task.predecessors`(업무·종류·지연일, `predecessor_ids`와 같은 순서), `ShiftProposal.causes`, `SchedulePlan{shifts, undated}`,
  `POST /api/tasks/{id}/schedule-plan`, `POST /api/tasks/{id}/predecessors/{pred}/schedule-plan`, 본문을 받는
  `PUT /api/tasks/{id}/predecessors/{pred}`(본문 없으면 옛 동작). 감사: `link_predecessor`(종류·지연일 포함), `change_predecessor`(이전·새),
  `reschedule`(이유 `dependency_changed`). 관계 설정의 409는 두 모양이다(계약 `PredecessorConflict` = `TaskConflict | ErrorBody`):
  오래된 승인은 `TaskConflict`(`shift_conflict`, 최신본 포함), 보관된 항목·동시 변경 재시도·새 id 부족은 `ErrorBody`(`invalid_state`,
  최신본 없음, 본문 없는 옛 요청도 같음). 응답 자체는 바꾸지 않고 계약만 맞췄다(`debd7fa`). 웹은 `current`가 있는
  `shift_conflict`만 다시 묻기로 돌리고 나머지는 일반 오류로 알린다.
- 웹: 업무 상세의 선행·후속 목록이 관계를 "시작 후 시작 (SS), 2일 뒤"처럼 글로 보이고, 고치기는 종류(뜻 문장과 함께)·지연일(0~3650
  정수만)을 고른 뒤 미리보기 → 승인/관계만 저장/취소. 승인 목록이 오래됐으면 다시 계산해 다시 묻는다. 간트의 선은 종류에 맞는 끝을
  잇고(SS는 두 시작의 왼쪽, FF는 두 끝의 오른쪽으로 돌아감, FS·SF는 틈이 없으면 줄 경계로), 화살촉은 후속 쪽에서 들어가는 방향을
  가리킨다. 줄 이름에 초점·가리킴이면 그 업무의 관계를 표 아래에 글로 보인다.
- 관계 지우기(`a17ee72`, 리뷰 r4334088593): 고치는 중인 관계를 지우면 편집 칸을 닫는다(전에는 편집 칸이 지운 관계를 계속 가리켜
  "관계 저장"이 그 관계를 다시 만들었다). 지우기 응답을 기다리는 동안 다른 저장을 막고, 다른 관계를 고치던 편집은 그대로 두며,
  실패하면 관계·편집 내용을 그대로 두고 알린다. 성공하면 "관계를 지웠습니다."와 함께 초점을 선행 업무 제목으로 돌린다. 다른 사람이
  다른 화면에서 지운 관계를 이 화면의 열린 편집 칸에서 저장하면 그 관계는 새로 생긴다(이 화면이 알 수 없는 변경, 남은 한계).
- 개발 예제(`dev-module-seed.sql`): 프로젝트 예제에 SS·1, FF·0, SF·0 관계를 더했다(날짜는 이미 관계를 채움). 예제는 한 번만 만드므로
  이미 만든 개발 DB에는 들어가지 않는다.
- 새 DB에서 개발 예제 전체(`debd7fa`, 일반 PostgreSQL): 별도 worktree(경로가 다르면 Compose 프로젝트·볼륨도 다르다)에서
  `cargo xtask dev --oidc --no-web`을 그대로 실행했다. 마이그레이션 전부(0077까지)와 `dev-seed.sql`·`dev-module-seed.sql`이 오류 없이
  적용됐고(`ON_ERROR_STOP=1`), 넣은 행을 다시 읽어 FS·0(옛 함수)·SS·1·FF·0·SF·0 관계 4개가 모두 날짜 제약을 채우는지, 모든 `id`가
  UUIDv7인지, `id` 기본값과 DB 안 UUID 생성 함수가 없는지 확인했다. 두 번째 실행은 행 수와 관계·업무·워크스페이스 내용 해시가 같았고,
  예제 워크스페이스를 앱 함수로 지운 뒤(휴지통, 그다음 영구 삭제) 다시 실행해도 만들지 않았다(같은 일회용 DB 안에서만).
  셀프호스트 Supabase에는 개발 실행 경로(`xtask dev --supabase`)가 없어 예제 실행은 NOT RUN이다(마이그레이션 전부는
  `test --supabase`가 새 Supabase DB에 적용한다).
- worktree 사이에 `CARGO_TARGET_DIR`를 같이 쓰면 `xtask`가 마지막으로 빌드된 worktree의 경로(`CARGO_MANIFEST_DIR`, 빌드 때 고정)를
  쓴다. 다른 worktree에서 `cargo xtask`를 돌리기 전에 그 worktree에서 `xtask`를 다시 빌드한다(이번 검증에서 그 탓에 일회용 프로젝트의
  컨테이너가 다시 만들어졌다. 마이그레이션·예제는 돌지 않았다).
- 시험: Rust 단위 13건(종류·지연일별 끝, 이미 채운 관계, 관계 고치기의 증분, 합류·분기·섞인 사슬, 마일스톤·한쪽 날짜, 날짜 없는 업무
  알림, 승인 목록 비교), 계약 3건(operationId, 선택 본문, 409 두 모양), 통합 7건(FS·0 유지와 옛 요청, 잘못된 입력·순환·권한, 미리보기
  원인과 원자적 승인·오래된 승인 거부, 종류별 API, 저장 때 다시 계산, 복제, 같은 미리보기의 동시 승인은 하나만, 409 두 모양), 웹
  `gantt-links.test.ts` 종류별 5건, `task-detail-links.test.ts` 10건(고치는 중 지우기 4건 포함, 그중 3건은 수정 전 실패),
  `link-conflict.test.ts` 2건. `debd7fa`에서 `cargo xtask check`(웹 364건), `test --oidc`(275), `test --supabase --oidc`(278),
  `test --supabase --session-pooler --oidc`(556) 통과.
- 마이그레이션 확인: 기존 개발 DB(관계 8개, 0076까지 적용)에 0077을 적용하자 8개 모두 FS·0이 됐다(DB 읽기). 별도로 `2edb234`(0076)로
  새 일회용 DB를 만들고 옛 함수로 FS 관계 3개(예제 1개, 사슬·합류 2개)를 둔 뒤 같은 DB를 `debd7fa`로 올렸다: 관계(선행·후속·항목·
  만든 사람·만든 때)와 업무(id·판·날짜)의 내용 해시가 그대로이고 세 관계 모두 FS·0, `kind`·`lag_days` 기본값과 CHECK가 생겼다.
- 브라우저 확인(내장 브라우저, 합성 데이터, 도구의 마우스·키보드 입력이며 사람의 실제 손 조작이 아님): 관계 종류 바꾸기(FS·0 →
  FF·2)의 미리보기와 원인 글, 취소(아무것도 바뀌지 않음), 승인(관계와 세 업무 날짜가 함께 바뀜), 옮길 것이 없는 바꾸기(바로 저장),
  새 관계 만들기와 "관계만 저장", 간트의 SS·FF·SF 선과 가리킴 때 관계 글(밝은·어두운 테마), 375×812 에뮬레이션(실제 기기 아님).
  저장한 뒤 초점이 대화상자 맨 위로 가던 것을 선행 업무 제목으로 돌렸다(`c0475a4`). 고치는 중 지우기는 `a19db16`에서 재현(지운 관계가
  다시 생김)한 뒤 `debd7fa`에서 편집 칸 닫힘·초점·꺼진 저장(요청 없음)·다른 관계 편집 유지·서버를 멈춘 실패를 확인했다. 지우기 응답이
  늦는 사이의 저장은 실제 화면에서 응답을 늦출 수 없어 컴포넌트 시험으로만 확인했다.

## 참고 자료

- [TypeScript strict](https://www.typescriptlang.org/tsconfig/strict.html)
- [noUncheckedIndexedAccess](https://www.typescriptlang.org/tsconfig/noUncheckedIndexedAccess.html)
- [typescript-eslint typed linting](https://typescript-eslint.io/getting-started/typed-linting/)
- [Clippy usage](https://doc.rust-lang.org/clippy/usage.html)
