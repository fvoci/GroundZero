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

- PostgreSQL·Meilisearch·격리된 Keycloak은 컨테이너로 실행한다. 테스트용 S3 호환 저장소는 SeaweedFS 고정 버전을 업로드 흐름 구현 시 추가한다(D-61, O-31). 운영 서비스나 개인 개발 DB를 테스트에 사용하지 않는다.
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
- **작업 큐 미결정.** `jobs/` 디렉터리는 만들지 않았다. 필요한 작업의 재시도·중복·복구 요구를 본 뒤 가장 단순한 구현을 고른다(O-19).
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

`cargo xtask licenses` 결과 중 허용 목록(D-60의 6개)에 없는 것 전부다. 예외는 넣지 않았고 `cargo xtask check`는 이 항목 때문에 FAIL이다. 아래 표의 어느 것도 "제품에 사용 불가능한" 범주(카피레프트 강제·비상업·사용 제한·불명)는 아니다. MPL-2.0 하나만 파일 단위 카피레프트이며 개발 도구에만 있다. 승인은 표를 보고 사용자가 결정한다.

범위 표기: **제품 바이너리** = 서버 실행 파일에 링크됨, **브라우저 배포물** = 웹 번들에 포함됨, **개발 도구** = 빌드·검사·생성에만 실행되고 산출물에 포함되지 않음, **테스트 서비스** = Compose 컨테이너.

| 패키지@버전                                                                                                                                                                                                                                                                                                                                                         | SPDX 식 전체                          | 원문                                                                                                                    | 유입 경로(직접 의존성 → …)                                                                                                        | 범위                                                                           | 의무                                                                                                                          | 대안                                                                                                                                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| icu_collections@2.3.0, icu_locale_core@2.3.0, icu_normalizer@2.3.0, icu_normalizer_data@2.3.0, icu_properties@2.3.0, icu_properties_data@2.3.0, icu_provider@2.3.1, litemap@0.8.3, potential_utf@0.1.6, tinystr@0.8.4, writeable@0.6.4, yoke@0.8.3, yoke-derive@0.8.4, zerofrom@0.1.8, zerofrom-derive@0.1.8, zerotrie@0.2.5, zerovec@0.11.8, zerovec-derive@0.11.6 | `Unicode-3.0`                         | [spdx](https://spdx.org/licenses/Unicode-3.0.html), [crates.io/icu_normalizer](https://crates.io/crates/icu_normalizer) | sqlx → sqlx-core → url → idna → idna_adapter → icu_*                                                                              | 제품 바이너리                                                                  | OSI 승인 허용적. 저작권 고지와 라이선스 원문을 배포물에 보존. 소스 제공 의무 없음                                             | 없음. sqlx-core가 url을 요구하고 url 2.5는 idna → icu를 쓴다. url의 `idna` 백엔드를 `unicode-rs`로 바꾸는 feature가 있으나 sqlx 경유로는 선택 불가 |
| unicode-ident@1.0.27                                                                                                                                                                                                                                                                                                                                                | `(MIT OR Apache-2.0) AND Unicode-3.0` | [crates.io](https://crates.io/crates/unicode-ident), [spdx Unicode-3.0](https://spdx.org/licenses/Unicode-3.0.html)     | serde(derive)·clap(derive)·sqlx(macros)·thiserror·utoipa → proc-macro2 → unicode-ident                                            | 제품 바이너리(derive 매크로는 컴파일 시 실행되지만 crate는 의존 그래프에 포함) | AND 결합이라 Unicode-3.0 조건(고지·원문 보존)도 적용. 소스 제공 의무 없음                                                     | 없음. proc-macro2를 쓰는 모든 derive 매크로가 의존. Rust 생태계 사실상 필수                                                                        |
| foldhash@0.1.5                                                                                                                                                                                                                                                                                                                                                      | `Zlib`                                | [spdx](https://spdx.org/licenses/Zlib.html), [crates.io](https://crates.io/crates/foldhash)                             | sqlx → sqlx-core → hashlink → hashbrown → foldhash                                                                                | 제품 바이너리                                                                  | 허용적. 출처를 허위 표시하지 않음, 변경 소스는 표시, 소스 배포 시 고지 유지. 바이너리 고지 의무는 없으나 원문 보존 권장       | 없음(hashbrown 0.15 기본 해셔). hashbrown feature 조정은 sqlx 경유로 불가                                                                          |
| argparse@2.0.1                                                                                                                                                                                                                                                                                                                                                      | `Python-2.0`                          | [spdx](https://spdx.org/licenses/Python-2.0.html), [npm](https://www.npmjs.com/package/argparse/v/2.0.1)                | @fvoci/api-client devDep openapi-typescript@7.13.0 → @redocly/openapi-core → js-yaml → argparse                                   | 개발 도구(계약 → TS 타입 생성)                                                 | 허용적. 고지 유지, 수정 시 변경 요약. 산출물에 미포함                                                                         | openapi-typescript를 다른 생성기로 교체(검토 필요) 또는 그대로 승인                                                                                |
| lightningcss@1.33.0, lightningcss-darwin-arm64@1.33.0 (플랫폼별 바이너리 패키지 동일)                                                                                                                                                                                                                                                                               | `MPL-2.0`                             | [spdx](https://spdx.org/licenses/MPL-2.0.html), [npm](https://www.npmjs.com/package/lightningcss/v/1.33.0)              | @fvoci/web devDep vite@8.3.4 → lightningcss                                                                                       | 개발 도구(CSS 변환). 브라우저 배포물에 lightningcss 코드는 포함되지 않음       | 파일 단위 카피레프트. MPL 파일을 배포할 때 그 파일의 소스 제공. 우리는 배포하지 않고 실행만 하므로 의무 미발생. 수정하지 않음 | vite 8이 기본 의존. 제거 불가. 승인 또는 vite 7 계열 사용(동일 의존 가능성 높음)                                                                   |
| lru-cache@11.5.3, minimatch@10.2.6                                                                                                                                                                                                                                                                                                                                  | `BlueOak-1.0.0`                       | [spdx](https://spdx.org/licenses/BlueOak-1.0.0.html), [npm lru-cache](https://www.npmjs.com/package/lru-cache/v/11.5.3) | 루트 devDep eslint@10.12.0 → @eslint/config-array → minimatch; @fvoci/web devDep jsdom@30.1.3 → @asamuzakjp/css-color → lru-cache | 개발 도구(lint, 테스트)                                                        | 허용적. 고지 유지                                                                                                             | 없음(eslint·jsdom 기본 의존). 승인                                                                                                                 |
| mdn-data@2.27.1                                                                                                                                                                                                                                                                                                                                                     | `CC0-1.0`                             | [spdx](https://spdx.org/licenses/CC0-1.0.html), [npm](https://www.npmjs.com/package/mdn-data/v/2.27.1)                  | @fvoci/web devDep jsdom@30.1.3 → css-tree → mdn-data                                                                              | 개발 도구(테스트 DOM)                                                          | 공개 도메인 헌정. 의무 없음                                                                                                   | 승인                                                                                                                                               |
| @csstools/color-helpers@6.1.2, @csstools/css-syntax-patches-for-csstree@1.1.15                                                                                                                                                                                                                                                                                      | `MIT-0`                               | [spdx](https://spdx.org/licenses/MIT-0.html), [npm](https://www.npmjs.com/package/@csstools/color-helpers/v/6.1.2)      | @fvoci/web devDep jsdom@30.1.3 → @asamuzakjp/css-color → @csstools/*                                                              | 개발 도구(테스트 DOM)                                                          | MIT에서 고지 의무를 뺀 것. 의무 없음                                                                                          | 승인                                                                                                                                               |

OR 결합으로 허용 쪽을 선택해 자동 통과한 것(선택 근거 기록): aho-corasick·byteorder·memchr `Unlicense OR MIT` → MIT. tinyvec `Zlib OR Apache-2.0 OR MIT` → MIT. whoami `Apache-2.0 OR BSL-1.0 OR MIT` → MIT.

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

허용 목록 밖(승인 요청에 추가):

| 패키지@버전                         | SPDX 식 전체 | 원문                                                                                            | 유입 경로                                                                         | 범위                                   | 의무                            | 대안         |
| ----------------------------------- | ------------ | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------- | ------------------------------- | ------------ |
| tslib@2.8.1                         | `0BSD`       | [spdx](https://spdx.org/licenses/0BSD.html), [npm](https://www.npmjs.com/package/tslib/v/2.8.1) | @fvoci/web devDep bits-ui@2.19.5 → @internationalized/date → @swc/helpers → tslib | 브라우저 배포물(번들에 헬퍼 포함 가능) | BSD Zero Clause. 고지 의무 없음 | 승인         |
| lightningcss@1.32.0 (+darwin-arm64) | `MPL-2.0`    | 위 표 참조                                                                                      | @tailwindcss/vite@4.3.3 → @tailwindcss/node → lightningcss                        | 개발 도구                              | 위 표와 동일                    | 위 표와 동일 |

### 미확인 (도입 전 확인 필요)

rhwp, Meilisearch 클라이언트 SDK, Tiptap 확장(오픈소스·유료 구분), OIDC 클라이언트 크레이트, Office 뷰어 후보, Playwright, 웹 폰트(현재는 시스템 글꼴만). 아직 의존성으로 추가하지 않았다.

## 10. CI 구조 (D-64)

- **베이스 이미지**: `deploy/docker/dev.Dockerfile` → `ghcr.io/fvoci/groundzero-ci-base`(공개 패키지, D-65). `.github/workflows/base-image.yml`이 Dockerfile·워크플로 변경, 주 1회 스케줄, 수동 실행 때만 빌드·게시한다. 제품 커밋마다 만들지 않는다. 게시 권한(`packages: write`)은 그 job에만 있고 pull_request에서는 실행되지 않는다.
- **제품 검사**: `.github/workflows/ci.yml`. `check`와 `integration` 두 job이 같은 `ghcr.io/fvoci/groundzero-ci-base@<digest>` 컨테이너에서 돈다. digest는 저장소 변수 `CI_BASE_IMAGE_DIGEST`이며 base-image 워크플로가 게시한 값을 옮겨 적는다. 변수가 비면 job이 시작 단계에서 실패하며 성공으로 간주하지 않는다. 호출하는 명령은 `pnpm install --frozen-lockfile`, `cargo xtask check`, `cargo xtask test --external-services`뿐이다.
- **서비스 격리**: GitHub의 컨테이너 job 안에서는 Compose를 띄울 수 없어 PostgreSQL·Meilisearch를 job `services`로 둔다. 자격 증명은 `compose.test.yaml`과 같은 테스트 값이며, `cargo xtask test --external-services`가 `DATABASE_URL`이 없으면 실패한다. 로컬은 `cargo xtask test`(Compose)로 같은 테스트를 돌린다.
- **캐시**: `Swatinem/rust-cache`와 buildx의 GHA 캐시는 속도용이다. 없어도 같은 명령이 성공해야 한다.
- **산출물 공유**: 현재 통합 테스트는 in-process라 제품 바이너리를 소비하지 않는다. e2e가 생기면 `build` job이 서버 바이너리와 웹 번들을 한 번 만들고 소스 SHA·아키텍처를 키로 하는 artifact로 넘긴다. 병목 측정 전에는 job을 더 쪼개지 않는다.
- **베이스 이미지 갱신 절차**: Dockerfile을 바꾸면 base-image 워크플로가 새 digest를 Step Summary에 적는다. `CI_BASE_IMAGE_DIGEST` 변수를 그 값으로 바꾸면 이후 CI가 새 이미지를 쓴다. 주간 스케줄 빌드도 같은 절차로 반영한다.

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
- **개발 로그인**: `cargo xtask dev --oidc`가 Keycloak(`oidc` 프로필)과 서버를 띄운다. 웹은 `VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev`, 접속은 `http://127.0.0.1:5173`(localhost 아님). 테스트 사용자는 `deploy/keycloak/realm-test.json`의 가짜 계정 `test-user`·`test-user-2`이며 운영에 쓰지 않는다. 이 realm의 클라이언트는 리다이렉트 URI를 `*`로 열어 둔 개발·테스트 전용이다.

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
- **다음 조각으로 미룬 것**: `@` 링크·사용자 언급·역링크, 워크스페이스 키(`@A/`)는 키 대장과 같은 방식으로 다음 조각에서 둔다. 항목 멤버·리더(12절 요구)도 이후 조각이다.

## 참고 자료

- [TypeScript strict](https://www.typescriptlang.org/tsconfig/strict.html)
- [noUncheckedIndexedAccess](https://www.typescriptlang.org/tsconfig/noUncheckedIndexedAccess.html)
- [typescript-eslint typed linting](https://typescript-eslint.io/getting-started/typed-linting/)
- [Clippy usage](https://doc.rust-lang.org/clippy/usage.html)
