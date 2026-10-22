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
