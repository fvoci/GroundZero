# fvoci

통합 작업·지식·학습 관리 앱. 결정 기록은 [docs/decisions.md](docs/decisions.md), 개발 절차는 [docs/development.md](docs/development.md), 저장소 배치는 [docs/repo-structure.md](docs/repo-structure.md)에 있다.

설치는 [셀프호스트 안내](docs/self-hosting.md)를 따른다. 기본 Compose가 DB 준비·마이그레이션·앱 시작을 순서대로 처리한다.
기존 셀프호스트 Supabase의 DB를 쓰려면 [Supabase 연결 안내](docs/self-hosting-supabase.md)를 따른다. 로그인은 두 구성 모두 OIDC다.

## 요구 도구

- Rust 1.99.0 (`rust-toolchain.toml`이 고정), `cargo-deny` 0.20.x
- Node 24, pnpm 11.24.0 (`package.json`의 `packageManager`)
- Docker와 Docker Compose v2 (통합 테스트·개발 서비스)

## 명령

| 목적                                                                               | 명령                                                              |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| 의존성 설치                                                                        | `pnpm install --frozen-lockfile`                                  |
| 포맷·lint·타입·단위 테스트·라이선스·계약 검사                                      | `cargo xtask check`                                               |
| 통합 테스트 (격리된 Compose 기동·정리 포함)                                        | `cargo xtask test`                                                |
| OIDC 통합 테스트까지 (Keycloak 포함)                                               | `cargo xtask test --oidc`                                         |
| 개발 환경 한 번에(서비스·서버·웹, 테스트 계정으로 바로 로그인)                     | `cargo xtask dev --oidc` → http://127.0.0.1:5173                  |
| DB 준비·마이그레이션을 직접 실행할 때 ([설치 안내](docs/self-hosting.md#db-setup)) | `fvoci-server db bootstrap && fvoci-server db migrate`            |
| 웹 개발 서버만 따로(`cargo xtask dev --no-web`일 때)                               | `VITE_API_URL=http://127.0.0.1:8080 pnpm --filter @fvoci/web dev` |
| 이 체크아웃의 Compose 자원 정리                                                    | `cargo xtask down`                                                |
| OpenAPI 계약 재생성                                                                | `cargo xtask openapi` 후 `pnpm run generate:api`                  |
| 라이선스 검사만                                                                    | `cargo xtask licenses`                                            |

`cargo xtask check`는 외부 서비스를 띄우지 않는다. 데이터베이스가 필요한 통합 테스트는 `cargo xtask test`에서만 돌고, `DATABASE_URL`이 없으면 건너뛰지 않고 실패한다.

SQL을 변경할 때는 [마이그레이션 적용 순서와 D-88 파일 지도](crates/server/migrations/README.md)를 먼저 본다.
