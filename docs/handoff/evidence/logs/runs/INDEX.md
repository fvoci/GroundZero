# 감사 검증 실행 기록 (로컬, Git 밖)

로그에는 `deploy/`의 시험용 자격 증명 값이 찍혀 있으므로 이 폴더 밖으로 올리지 않는다.

| 실행 | 시각(UTC) | SHA | 명령 | 환경 | 결과 | 로그 |
| --- | --- | --- | --- | --- | --- | --- |
| R0 | 2026-10-21 17:54~17:57 | 634019f(첫 통합) | `cargo xtask test --oidc`, `--supabase --oidc`, `--supabase --session-pooler --oidc` | PostgreSQL·Supabase·풀러 | FAIL 세 환경 모두(collab 2건) | 덮어써져 없음. 요약 r0-634019f-first-matrix-FAIL.txt |
| R1 | 17:58~17:59 | 634019f | `cargo xtask test --keep` | PostgreSQL(OIDC 없음) | FAIL(collab 2건, 결과 6건에서 멈춤, 104 통과) | r1-634019f-keep-pg-FAIL.log |
| R2 | 18:20~18:27 | 7d6775c | R0과 같음 | 세 환경 | PASS 45/321, 45/332, 90/664 | r2-7d6775c-*.log, r2-7d6775c-status.txt |
| R3 | 20:00~20:03 | 2286a9d(둘째 통합, #75 재병합·#64 시험 전) | `cargo xtask check --skip-licenses` | — | FAIL(format:check, 추적하지 않는 .claude/settings.local.json 한 건), 나머지 PASS | r3-2286a9d-check.log |
| R4 | 20:07~20:15 | a0a45bb | `cargo xtask down` 뒤 R0과 같음 | 세 환경 | PASS 45/328, 45/339, 90/678 | r4-a0a45bb-*.log |
| R5 | 20:38~20:42 | #68 브랜치 cfe3dbf + worker.rs 단언(= cd2c1bd) | `cargo xtask test --keep` | PostgreSQL(OIDC 없음) | PASS 45/282 | r5-cd2c1bd-branch68-keep-pg-PASS.log |
| R6 | 21:32~21:35 | 36f52d7(a0a45bb + #58·#59·#68·#80 새 머리) | `cargo xtask check --skip-licenses` | — | FAIL(R3과 같은 파일 한 건), 나머지 PASS(웹 118파일·578건) | r6-36f52d7-check.log |

- 첫 통합 상태(912c48a·634019f·7d6775c)에서 `cargo xtask check`를 돌린 기록은 찾지 못했다(미확인).
- 36f52d7에서는 DB 행렬을 돌리지 않았다. a0a45bb와의 차이는 웹 파일(#58 ItemScreen, #59 events/labels·time, #80 DisplaySettingsDialog)과 시험 파일(#68 worker.rs)뿐이다.
