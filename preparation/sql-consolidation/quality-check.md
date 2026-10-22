# 종합 worktree 품질 검사

- 실행일: 2026-10-22.
- 커밋: `70d0ba1450abd9b47f404ee713590291018166f5`.
- 디렉터리: `/home/kinesis/orca/workspaces/GroundZero/sql-prep-synthesis`.
- 사전 설치: `pnpm install --frozen-lockfile` PASS.
- 명령: `cargo xtask check` (라이선스 생략 없음).
- 종료 코드: 0, `all checks passed`.
- 원본 로그: 이 worktree의 `target/sql-consolidation-check/final-check.log`에 로컬 보존.
- 로그 SHA-256: `5e6289ce90a9edd69894924ab3fb9d3be204d11d3d92158a473b178283dffb9d`.

| 검사                                | 결과                                   |
| ----------------------------------- | -------------------------------------- |
| Rust 포맷·Clippy                    | PASS                                   |
| Rust 단위 시험                      | PASS — 79 + 2 + 12 = 93개, 실패·무시 0 |
| 의존성 advisory/bans/sources        | PASS                                   |
| OpenAPI 및 생성 API 클라이언트 계약 | PASS                                   |
| Prettier·ESLint                     | PASS                                   |
| Svelte/TypeScript                   | PASS — 오류 0, 경고 0                  |
| 양 언어 메시지 키·컴파일            | PASS                                   |
| HWP WASM 및 제3자 고지              | PASS                                   |
| 웹 시험                             | PASS — 131개 파일, 734개 시험          |
| Rust·pnpm 라이선스                  | PASS                                   |

이 검사는 DB 행동 비교와 별개다. DB별 객체·권한·초기 데이터·실제 앱 역할 결과는
`verification/results.md`에 기록한다. 이 검사 이후의 문서와 실행 증거 추가는 별도 형식 검사로 확인한다.

B가 DB 비교를 실행한 로컬 커밋 `efdaa5a7ffa1248ed3587b46bcbe8f8927606bd7`과 위 종합 커밋 사이에서
`crates/`, `xtask/`, `deploy/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`의 Git 차이는 0이다.
비교기·카탈로그·후보 SQL의 SHA-256도 실행 입력과 일치함을 확인했다. 따라서 PR 체크아웃에서 같은 내용으로 재현할 수 있으며,
작업자의 로컬 커밋 SHA만 원격에 존재한다고 가정하지 않는다.

## CodeRabbit 실패 경로 수정 후 확인

종합 커밋 `57667970449d31b4332f2df97d86691f280d08ba`에서 비교기의 기동 등록·정지·결과 기록만 수정했다.
후보 SQL, 카탈로그 비교, 앱 행동 시험 입력은 바뀌지 않았다. 변경 범위에 맞춰 다음을 다시 확인했다.

- `cargo fmt --all --check`: PASS.
- `cargo clippy -p xtask --all-targets --locked -- -D warnings`: PASS.
- `cargo test -p xtask --locked sql_compare::tests`: 새 실패 경로 3개 PASS.
- `cargo test --workspace --lib --bins --locked`: 79 + 2 + 15 = 96개 PASS, 실패·무시 0.
- 단위 시험 원본 로그: 로컬 `target/sql-consolidation-check/cleanup-unit.log`.
- 로그 SHA-256: `9b0ec5289259ef13a562a20bdc5463ffe54ac0286b21c18fe4a331ec36db764d`.

DB 담당자는 같은 수정의 실제 PostgreSQL A/A 실행에서 설치·스냅숏·객체·변조·정지 단계를 재확인했다.
최종 비교기 소스와 이 후속 실행의 근거는 `verification/results.md`에서 별도로 구분한다.

## 출력 경로 경합 수정 후 확인

종합 커밋 `71c029dc1ea31ee33babaf4f484b9bd07ee23903`에서 출력 디렉터리를 원자적으로 확보하도록 수정했다.
8개 동시 요청 중 하나만 성공하며, 나머지 요청과 재실행이 기존 증거를 덮지 않는 시험을 추가했다.
SQL 적용·카탈로그·행동 입력에는 변화가 없어 DB 행렬을 반복하지 않았다.

- `cargo fmt --all --check`: PASS.
- `cargo clippy -p xtask --all-targets --locked -- -D warnings`: PASS.
- `cargo test -p xtask --locked`: 16개 PASS, 실패·무시 0. 비교기 시험은 4개다.
- 원본 로그: 로컬 `target/sql-consolidation-check/atomic-output-unit.log`.
- 로그 SHA-256: `701e0be8490a7529e734a9c24e013dee20fae2feb313e968e64b804acd369bf0`.
