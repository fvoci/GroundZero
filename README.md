# fvoci

통합 작업·지식·학습 관리 앱. 제품 원칙과 제안의 상태, 공통 규칙은 [결정 기록](docs/decisions.md)과
[AGENTS.md](AGENTS.md)를 읽는다. 변경 종류별로 읽을 곳과 돌릴 검사는 AGENTS.md가 가리키는 `.agents/skills/`에 있다.
달리는 아비시니안 오버홀 전의 개발 지침·저장소 구조 문서는 제거했고(D-127)
[제거 전 판](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md)은
역사 기록이다. 과거 실행·감사 기록은 필요한 SHA만 확인하며, 현재 정책이나 검증 완료로 해석하지 않는다.

설치는 [셀프호스트 안내](docs/self-hosting.md)를 따른다. 기본 Compose가 DB 준비·마이그레이션·앱 시작을 순서대로 처리한다.
기존 셀프호스트 Supabase의 DB를 쓰려면 [Supabase 연결 안내](docs/self-hosting-supabase.md)를 따른다. 로그인은 두 구성 모두 OIDC다.

## 요구 도구

- Rust 1.99.1 (`rust-toolchain.toml`이 고정), `cargo-deny` 0.20.x
- Node 24.22.1 LTS(Corepack 포함), pnpm 12.15.0 (`package.json`의 `packageManager`, Corepack으로 실행: `corepack enable pnpm`)
- Docker와 Docker Compose v2 (통합 테스트·개발 서비스)

## 명령

| 목적                                                                               | 명령                                                                                      |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| 의존성 설치                                                                        | `pnpm install --frozen-lockfile`                                                          |
| 포맷·lint·타입·단위 테스트·라이선스·계약 검사                                      | `cargo xtask check`                                                                       |
| 통합 테스트 (격리된 Compose 기동·정리 포함)                                        | `cargo xtask test`                                                                        |
| OIDC 통합 테스트까지 (Keycloak 포함)                                               | `cargo xtask test --oidc`                                                                 |
| 셀프호스트 Supabase 직접·세션 풀러 시험                                            | `cargo xtask test --supabase` / `cargo xtask test --supabase --session-pooler`            |
| 실제 브라우저 회귀 시험 (Chromium, 실제 서버·DB·테스트 OIDC)                       | `cargo xtask browser`                                                                     |
| 개발 환경 한 번에(서비스·서버·웹, 테스트 계정으로 바로 로그인)                     | `cargo xtask dev --oidc` → http://127.0.0.1:5173                                          |
| DB 준비·마이그레이션을 직접 실행할 때 ([설치 안내](docs/self-hosting.md#db-setup)) | `fvoci-server db bootstrap && fvoci-server db migrate`                                    |
| 웹 개발 서버만 따로(`cargo xtask dev --no-web`일 때)                               | `VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev`                         |
| 이 체크아웃의 Compose 자원 정리                                                    | `cargo xtask down`                                                                        |
| 여러 워크트리에서 나란히 작업(데우기·동시 빌드·정리·시험 격리)                     | [병렬 워크트리 안내](docs/parallel-worktrees.md)                                          |
| UI 컴포넌트 상태 보기(Storybook, D-108) / 정적 빌드                                | `pnpm --filter @fvoci/web run storybook` / `pnpm --filter @fvoci/web run build-storybook` |
| OpenAPI 계약 재생성                                                                | `cargo xtask openapi` 후 `pnpm run generate:api`                                          |
| 라이선스 검사만                                                                    | `cargo xtask licenses`                                                                    |
| HWP·HWPX 미리보기 WASM 빌드·고지 갱신                                              | `cargo xtask rhwp`                                                                        |

`cargo xtask check`는 외부 서비스를 띄우지 않는다. 데이터베이스가 필요한 통합 테스트는 `cargo xtask test`에서만 돌고, `DATABASE_URL`이 없으면 건너뛰지 않고 실패한다.

`cargo xtask down`은 이 체크아웃의 개발·시험·브라우저·Supabase 시험 프로젝트의 컨테이너·Compose 프로젝트 네트워크와 DB·검색·Keycloak 볼륨을 지운다. `cargo xtask test --supabase --keep`으로 남긴 시험 DB도 사라진다.
`target/dev-files` 첨부는 남는다. 기존 데이터를 지우기 전에 보존 범위와 사용자 승인을 확인한다.

SQL을 변경할 때는 결정 기록의 D-88·D-107·D-126을 먼저 본다. 제품 SQL 정본은 `crates/server/migrations/` 하나다.
