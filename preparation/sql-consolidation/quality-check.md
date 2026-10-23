# 종합 worktree 품질 검사

> **역사 기록이며 현재 상태를 나타내지 않음.** 아래는 기록된 SHA의 실행 결과다. 관련 결함이나 SHA 조사 때만 원본을 참조한다. 이번 분리 작업의 결과는 [증거 안내](evidence.md)에 따로 기록한다.

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

## 대체된 기록

최초 `70d0ba1450abd9b47f404ee713590291018166f5`와 이후 단계별 검사·중간 실패는 [원래 품질 기록](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/quality-check.md)에 보존한다. 최종 정책 반영 실행은 위 표이며 과거 PASS/FAIL을 이번 검사 결과로 재사용하지 않는다. 위 `target/` 로그는 당시 로컬 경로이고 원격 보존을 검증한 270개 파일에 포함되지 않는다.
