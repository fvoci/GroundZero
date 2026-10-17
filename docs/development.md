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

허용 목록 밖으로 들어온 tslib·lightningcss는 위 O-30 표에 합쳤다(번들 포함 여부를 소스맵으로 다시 확인함).

@ 참조 입력(2026-10-17): @tiptap/suggestion 3.32.0 MIT, @floating-ui/dom 1.8.0 MIT(이미 bits-ui로 들어와 있던 것을 직접 의존성으로 고정). 허용 목록 안이다. @tiptap/extension-mention은 표시 이름(label)을 본문에 저장하므로 쓰지 않았다.

### 미확인 (도입 전 확인 필요)

rhwp, Meilisearch 클라이언트 SDK, Tiptap 유료 확장, OIDC 클라이언트 크레이트, Office 뷰어 후보, Playwright, 웹 폰트(현재는 시스템 글꼴만). 아직 의존성으로 추가하지 않았다.

## 10. CI 구조 (D-64)

- **베이스 이미지**: `deploy/docker/dev.Dockerfile` → `ghcr.io/fvoci/groundzero-ci-base`(공개 패키지, D-65). `.github/workflows/base-image.yml`이 Dockerfile·워크플로 변경, 주 1회 스케줄, 수동 실행 때만 빌드·게시한다. 제품 커밋마다 만들지 않는다. 게시 권한(`packages: write`)은 그 job에만 있고 pull_request에서는 실행되지 않는다.
- **제품 검사**: `.github/workflows/ci.yml`. `check`와 `integration` 두 job이 같은 `ghcr.io/fvoci/groundzero-ci-base@<digest>` 컨테이너에서 돈다. digest는 저장소 변수 `CI_BASE_IMAGE_DIGEST`이며 base-image 워크플로가 게시한 값을 옮겨 적는다. 변수가 비면 job이 시작 단계에서 실패하며 성공으로 간주하지 않는다. 호출하는 명령은 `pnpm install --frozen-lockfile`, `cargo xtask check`, `cargo xtask test --external-services`뿐이다.
- **서비스 격리**: GitHub의 컨테이너 job 안에서는 Compose를 띄울 수 없어 PostgreSQL·Meilisearch를 job `services`로 둔다. 자격 증명은 `compose.test.yaml`과 같은 테스트 값이며, `cargo xtask test --external-services`가 `DATABASE_URL`이 없으면 실패한다. 로컬은 `cargo xtask test`(Compose)로 같은 테스트를 돌린다.
- **OIDC 로그인 테스트(`oidc` job)**: 컨테이너 job에서는 Compose를 띄울 수 없으므로 GitHub 호스트 러너(`ubuntu-24.04`)에서 `compose.test.yaml`의 `oidc` 프로필(PostgreSQL·Meilisearch·Keycloak)을 띄운다. 테스트는 다른 job과 같은 베이스 이미지 digest 안에서 같은 진입점 `cargo xtask test --external-services --oidc`로 돈다. 테스트 컨테이너는 Compose 네트워크에 붙고 체크아웃만 마운트한다(Docker 소켓은 넘기지 않음). 프로젝트명 `fvoci-ci-<run_id>-<run_attempt>`로 실행마다 컨테이너·네트워크·볼륨을 분리하고, PostgreSQL은 tmpfs라 매번 빈 DB에서 시작하며, 결과와 상관없이 `down --volumes`로 지운다.
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
- **남은 범위(D-31)**: 마일스톤·선행 관계·반복·일정·달력·간트·저장한 보기는 다음 조각.

## 21. 마일스톤·선행 관계·후속 재배치 승인·간트 (2026-10-17, D-31·D-33·D-50·D-57, 마이그레이션 0012)

- **마일스톤**: 마감일 하나만 가진 업무(`milestone`, DB 검사로 시작일 금지). 간트에 ◆로 보인다.
- **선행 관계**: 같은 항목 안의 "끝나야 시작" 하나(`task_dependencies`). 후속 업무 쓰기 권한으로 추가·삭제하며 순환·자기 참조·다른 항목은 400, 독자는 403, 볼 수 없으면 404. 같은 항목의 관계 변경은 잠금으로 직렬화해 동시에 서로를 선행으로 걸어도 하나만 성공한다. `link_predecessor`·`unlink_predecessor` 감사 기록.
- **후속 재배치 (D-31 "영향과 충돌을 보여준 뒤 사용자가 승인")**: 날짜를 바꾸고 후속 업무가 있으면 화면이 먼저 `POST /api/tasks/{id}/schedule-preview`로 제안을 받는다(읽기, 아무것도 바꾸지 않음). 제안은 바뀐 선행의 마감 다음 날부터 시작하도록 미는 최소 이동이고 기간은 유지한다(앞당기면 제안 없음, 날짜 없는 업무는 옮기지 않음). 사용자가 "후속 업무도 옮겨 저장"을 고르면 업무 고치기의 `successor_shifts`로 같은 트랜잭션에서 함께 옮긴다. 승인한 업무 중 하나라도 기준 버전이 지났으면 아무것도 바꾸지 않고 409 `shift_conflict`, 화면은 다시 계산해 묻는다. 후속이 아닌 업무를 옮기라는 요청은 400. 옮긴 업무마다 `reschedule` 감사 기록(이유 `predecessor_moved`). "이 업무만 저장"은 후속 업무를 그대로 둔다.
- **간트**: 항목 업무 영역의 세 번째 보기. 상위 업무 바로 뒤에 하위 업무, 막대·마일스톤·오늘 선. 막대는 보조 표시이고 줄마다 단추가 같은 내용(번호·제목·기간·상태·선행)을 글로 말한다. 가로 이동은 "2주 앞·오늘·2주 뒤" 단추로 키보드에서도 된다. 날짜는 UTC 날짜 번호로 계산해 시간대 때문에 하루가 밀리지 않는다(D-50). 달력·간트 라이브러리는 들이지 않았다: 필요한 것은 읽기 표시와 상세 열기이고 끌어서 옮기기는 없으며, 새 의존성은 허용 목록·접근성 확인 부담(O-30 대기 중)만 늘린다.
- **리뷰 반영(0011 동시성)**: 열 잠금을 업무 행보다 먼저 잡는다(다시 매기기와의 교착 방지). 다시 매기기는 그 열에 남은 행만 바꾼다. 볼 수 없게 된 담당 정리는 워크스페이스 잠금으로 직렬화해 비공개 전환과 역할 강등이 동시에 일어나도 놓치지 않는다. 새 담당자 목록은 한 번 정해 그 목록만 넣고(그사이 지워진 담당을 확인 없이 되살리지 않음) 멤버 행은 사용자 id 순으로 잠근다. 실제로 지운 경우에만 `unassign` 감사 기록. 잠금 순서: 열 → 업무 행(id 순) → 항목 공유 → 번호.
- **리뷰 반영(이 조각)**: 업무 행을 잠그는 모든 경로(고치기·옮기기·상위 바꾸기)가 `app_lock_task_rows`로 열 잠금을 (항목, 상태) 순으로 먼저 잡고 행을 id 순으로 잡는다. 잡는 사이 열이 바뀌었거나 교착이 감지되면 409(다시 시도)이며 아무것도 바뀌지 않는다. 승인 재배치는 선행 관계 잠금 아래에서 후속 여부를 확인하고, 같은 업무를 두 번 넣거나 기준 버전이 빠지면 거부한다. 제안은 이번에 옮겨진 선행이 원래보다 더 요구하는 만큼만이며(전부터 겹친 만큼과 완료한 업무는 그대로), 미리보기도 시작일 > 마감일을 거부한다. 화면은 미리보기 패널이 떠 있는 동안 폼을 잠그고 그때의 입력·기준 버전 그대로 보내며, 서버 기준 내용과 날짜가 다르면 언제나 미리 본다(충돌 뒤 다시 저장 포함). 이중 제출을 막고, 패널이 닫히면 저장 단추로 초점을 돌린다. 간트는 최대 400일까지 그리고 그 뒤의 업무는 따로 적는다.
- **검증**: 통합 테스트 3건(마일스톤 날짜 규칙, 관계 규칙·권한·감사·동시 순환 방지, 미리보기·승인 재배치·`shift_conflict` 무변경·후속 아님 400·감사), 단위 테스트 3건(최소 이동·앞당김·두 선행 중 늦은 쪽), 웹 테스트 2건(간트 배치·시간대). 브라우저: 간트 표시와 줄 단추 설명, 선행 마감을 미루고 저장 → 후속 2개 제안(초점이 안내 제목으로) → 함께 옮겨 저장 → 간트 갱신, 대화상자를 닫으면 간트 줄로 초점 복귀.
- **남은 범위(D-31)**: 회의·발표·행사 일정과 달력 보기, 반복 업무·일정(O-19 미결이므로 백그라운드 작업 없이 설계), 저장한 보기·검색은 다음 조각.

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
- **남은 범위(D-32)**: 강의 회차·공부와의 연결과 영상 길이·배속 기준 예상 시간은 강의·스터디 조각에서. 사용자 시간대 설정 저장은 아직 없고 브라우저 시간대를 쓴다.

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
- **아직 없음**: 첨부 본문 검색(파일 조각 뒤), 모듈·담당자·상태·날짜 필터(API는 종류·워크스페이스만).
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

## 참고 자료

- [TypeScript strict](https://www.typescriptlang.org/tsconfig/strict.html)
- [noUncheckedIndexedAccess](https://www.typescriptlang.org/tsconfig/noUncheckedIndexedAccess.html)
- [typescript-eslint typed linting](https://typescript-eslint.io/getting-started/typed-linting/)
- [Clippy usage](https://doc.rust-lang.org/clippy/usage.html)
