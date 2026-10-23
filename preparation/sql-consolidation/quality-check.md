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

## 제한 설치 관리자 검증 도구 추가 후 확인

종합 커밋 `848f13cb82ac236a6a5fd68a1f1c7af354c4c4fa`의 제품 코드·후보 SQL은 그대로이며, `sql-compare`에 제한 설치 관리자와
명시적인 핵심 행동 시험 범위를 추가했다. B의 실행 소스 `213cc650bc3e192727935670dc1ee6862c1a8d88`과
동일한 도구 코드를 반영했다.

- `cargo fmt --all --check`: PASS.
- `cargo clippy -p xtask --all-targets --locked -- -D warnings`: PASS.
- `cargo test -p xtask --locked`: 16개 PASS, 실패·무시 0.
- `cargo xtask sql-compare --help`: 새 옵션과 기존 기본 동작 확인.
- 원본 단위 시험 로그: 로컬 `target/sql-consolidation-check/restricted-unit.log`.
- 로그 SHA-256: `11160c0eb71b1b24be2d8027dcaf7473d15ead4db286faff9c59bbfa95b645a8`.

A는 새 권한 검증 코드에 한정해 실제 역할 속성·DB 권한 확인, 거부 판정, 반복 실행·자격 증명 보존,
결과 기록·등록 자원 정지를 교차 검토했고 구체적인 결함을 발견하지 못했다. 이는 실제 DB 시험을 대신하지 않는다.
종합 담당도 최종 권한 JSON·단계 결과·변조 탐지/원복과 양쪽 로그의 60개 통과 합계를 독립 확인했다.
두 Rust 모듈·후보의 SHA-256은 실행 입력과 같고, 위 B 실행 커밋과 종합본의 제품/도구 소스 차이는 없다.

## PostgreSQL 17 지원 기준과 초기화 전 검사

종합 커밋 `8c2ae88659e1932fded6e03687e9abcc8b4c147c`에 최소 버전 검사, 기존 Cloud 읽기 전용 경로의
숫자형 버전 조회, PG17 기본 시험 환경과 PG18 CI 분담을 반영했다. 실제 제품 마이그레이션과 후보 SQL은 바꾸지 않았다.

- `cargo test --workspace --lib --bins --locked`: 80 + 2 + 17 = **99개 PASS**, 실패·무시 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
- `cargo fmt --all --check`, 변경 문서·Compose·CI의 Prettier, `git diff --check`: PASS.
- 단위 로그: 로컬 `target/sql-consolidation-check/pg17-unit.log`, SHA-256
  `b4d3b417b98fb94f8022ed1dffd2413f0bb078bdb31b9af0133768c3e881cd02`.
- Clippy 로그: 로컬 `target/sql-consolidation-check/pg17-clippy.log`, SHA-256
  `2b5c793b32ef853492af25baa0ac0f6222df818b641fa3df5121cdce7a19debb`.

A는 B의 PG17/18 Compose 변경에서 데이터 디렉터리·기존 개발 볼륨 보존·CI override·Supabase 옵션을 교차 검토했다.
C의 버전 조회에서도 단일 읽기 전용 트랜잭션·기한·오류 정리·비밀 보호·관측과 지원 판정의 분리를 확인했고
구체적인 결함을 발견하지 못했다. 두 검토 모두 실제 DB 실행을 대신하지 않는다.
PG17 A/B 및 제한 권한, 지원 미달/필수 권한 거부의 실제 결과는 `verification/results.md`에서 별도로 기록한다.

종합 담당은 PG17 전체 실행(`e3d3e8ddd19045c11282d796354bef48dae73eb3`)의 양쪽 로그를 독립 합산해
각 47개 결과 그룹, 358 passed, 0 failed, 0 ignored, 17 filtered를 확인했다. 제한 권한 실행
(`9c122173e4c6ec5d33c6be261975f68753008015`)은 양쪽 각 7개 그룹, 60 passed, 실패·무시·필터 0이었다.
양쪽 역할 속성·기대 거부·반복 설치·멤버십·기존 자격 증명 보존 JSON, 네 변조의 탐지/원복과
전체 결과/정지 PASS도 확인했다. 두 실행의 비교기·권한 검사·서버 초기화 소스는 종합본과 같다.
후보는 다른 worktree 경로에서 입력했지만 SHA-256이 종합본의 후보와 일치한다.

초기화 전 검사도 실제 PG16.13의 두 거부 경로와 PG17의 권한 부족 9개 경로에서 전후 JSON을 비교해
카탈로그 무변경을 확인했다. 비밀번호는 카탈로그에서 추정하지 않았다. 별도 SCRAM 경로에서 두 역할의
잘못된 비밀번호가 거부되고, 세 bootstrap 실패 후 원래 비밀번호의 로그인 6회가 성공한 증거를 확인했다.
컨테이너 내부 localhost의 trust 인증은 이 비밀번호 보존 증거로 사용하지 않는다.

## 사용자 요청 재리뷰의 결과 기록 오류 진단

CodeRabbit 재리뷰 `2bcbf141-1ff2-4d18-bdb5-7f029e664383`가 `08cdcba`에 남긴
[Minor 지적](https://github.com/fvoci/GroundZero/pull/92#discussion_r4349753858)을 확인했다.
작업 또는 정리 오류와 결과 파일 기록 오류가 겹치면 반환 우선순위 때문에 기록 오류가 진단에서 사라졌다.
종합 커밋 `dd538ae5c4845f96365296fb11fb2cf72ee85b61`은 기록 오류의 원인 체인을 stderr에 남기고,
작업 오류 → 첫 정리 오류 → 기록 오류의 반환 우선순위를 유지한다.

- 기존 실패 주입 시험에서 작업·정리 실패 유무 네 조합에 기록 실패를 겹쳐, 반환 오류·정리 상태·기록 시도와 진단 내용을 확인했다.
- `cargo test -p xtask sql_compare::tests --locked`: **4개 PASS**, 실패·무시 0.
- `cargo clippy -p xtask --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `git diff --check`: PASS.
- 단위 로그: 로컬 `target/sql-consolidation-check/rereview-record-unit.log`, SHA-256
  `44e069739e4647ed80b76d7afdf75b96b4699b72d19cf1d756881bb0325f034a`.
- Clippy 로그: 로컬 `target/sql-consolidation-check/rereview-record-clippy.log`, SHA-256
  `7da0a4755fb0b3b50e1529710d3318eb7aa40ffecc9cb8fea78217707c4148ec`.

이 수정은 비교기의 오류 진단과 해당 시험에 한정한다. SQL·카탈로그 비교·역할·행동 시험 입력은 같으므로
DB 행렬을 다시 실행하지 않았다. 앞선 DB 실행 소스와 현재 비교기 사이에는 이 진단 변경이 있으며,
이전 파일 해시가 현재 비교기와 같다고 주장하지 않는다. 원격 CI와 후속 리뷰 상태는 Draft PR에 별도로 기록한다.

## 정책 반영 후 후보 (2026-10-23)

- 커밋: 행렬 실행 소스 `f5b5a9d4a42dc6c87deddddcda0d0ea41f7092f9`에 이 문서·실행 증거만 더한 작업 트리(Rust·웹·`deploy/`·`xtask/` 차이 없음).
- 명령: `cargo xtask check` (라이선스 생략 없음). 종료 코드 0, `all checks passed`.
- 첫 실행은 `pnpm format:check`만 실패했다. 실행 중 고친 `synthesis.md`의 표 정렬이었으며 Prettier로 정리한 뒤 전체를 다시 실행했다.
- 원본 로그: 로컬 `target/sqlp2-check/check2.log`, SHA-256 `d815e0596733776c436e247e2b3506912e9ab94e3ba2507b154ce86b16810fd5`.
  실패한 첫 로그는 `target/sqlp2-check/check.log`(`087dc967…78e21c8`).

| 검사                                          | 결과                                    |
| --------------------------------------------- | --------------------------------------- |
| Rust 포맷·Clippy·단위 시험                    | PASS — 83 + 2 + 19 = 104개, 실패·무시 0 |
| 의존성 advisory/bans/sources·라이선스         | PASS                                    |
| OpenAPI·생성 클라이언트 계약, Prettier·ESLint | PASS                                    |
| Svelte/TypeScript, 메시지 키, HWP WASM·고지   | PASS                                    |
| 웹 시험                                       | PASS — 135개 파일, 751개 시험           |

DB 행동 결과는 [검증 결과](verification/results.md)의 「정책 반영 후 후보의 검증」이 근거다.
