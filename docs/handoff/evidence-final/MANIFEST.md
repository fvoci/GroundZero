# 최종 감사 증거 묶음 목록 (evidence-final)

- 만든 때: 2026-10-22(W18). 인계 브랜치 `handoff/audit-resume` `089c4e5` 위의 병합하지 않는 브랜치 `handoff/audit-final-evidence`. 앞선 묶음
  `../evidence/`(089c4e5)의 다음 단계로, W1~W17 작업자가 각 worktree의 `target/audit-evidence/`에 남긴 증거의 공개 사본이다.
- 전체 파일과 SHA-256은 같은 폴더의 `SHA256SUMS`(이 파일 포함, `SHA256SUMS` 자신은 제외). 확인: `sha256sum -c SHA256SUMS`(macOS
  `shasum -a 256 -c SHA256SUMS`). `.gitattributes`(`* -text`)로 줄 끝 변환을 막는다. 저장소의 `.gitignore`가 `*.log`를 빼므로 `git add -f`로 넣었다.
- 폴더 이름은 원본 `target/audit-evidence/<이름>/`을 그대로 쓴다. 각 폴더의 `README.md`·`SUMMARY.md`·`report.md`·`REPORT.md`·`INDEX.md`가 작업자
  원본 설명이며, 아래 표는 그 설명과 로그 끝줄(`exit=`·`# rc=`·`test result:`)을 대조해 옮긴 것이다. 표와 폴더 설명이 다르면 로그가 기준이다.

## 사본 규칙 (앞선 `../evidence/MANIFEST.md`와 같다)

- 로그는 원본이 아니라 사본이다. 원본(각 worktree의 `target/audit-evidence/`)은 읽기만 했고 고치거나 지우지 않았다.
- 지운 것(텍스트 파일 144개에서 바뀜, 이미지·PDF는 바이트 그대로):
  - 로컬 사용자 경로 `/home/<user>` → `<home>`, 사용자 이름 → `<user>` (29개: `d95-pr64-w1b` 18, `w2-pr82` 10, `w17/logs/dev-stack.log` 1).
  - 홈 줄임 경로 `~/` → `<home>/` (42개: `d95-pr64` 19, `w10-expected-user` 17, `w6` 3, `w5`·`w7-pr65`·`w8` 각 1. 첫 push `659c378` 뒤 둘째 커밋에서 더함).
  - 터미널 색·제어 코드(ESC `[`…, OSC, vitest가 중간에서 자른 `ESC[32...` 같은 조각) 삭제 (65개: d95-pr64·d95-pr64-w1b·w2-pr82·w3-pr59·w5·w7-pr65·w8·w9·w11-dialogs·w13·w14·w17).
  - `…PASSWORD|SECRET|TOKEN|_KEY=` 값 → `<redacted>`, `Authorization: Bearer` 값 → `<redacted>` (2개: `w8/probe.sh`, `w8/probe2.sh`의
    Meilisearch 마스터 키. `deploy/`의 명백한 시험 값이지만 규칙대로 지웠다).
  - CodeRabbit 링크의 `scope=ghh_…` 식별자 → `<redacted>` (31개: `w4/collect/pr*.json` 22, `w6/pr60-after*.json` 2, `w10-expected-user/coderabbit-summary-*.md` 5,
    `w3-pr59/coderabbit-*-summary.md` 2). 토큰처럼 생긴 값이라 지웠다. 지운 뒤에도 JSON은 모두 `jq`로 읽힌다.
  - DB 주소 비밀번호(`postgres://<user>:<redacted>@`), 임시 작업 폴더(`<scratch>`), 쿠키·세션 토큰 값: 원본에 없어 바뀐 것 없음.
    `<scratchpad>`·`<worktree>`라는 낱말은 원본 작업자가 이미 가려 둔 자리 표시다.
- 바뀐 텍스트 가운데 패치·diff·소스 사본(`*.patch`·`*.diff`·`*.rs`·`*.svelte`·`*.sql`)은 없다. `w13/pr-update-patches/*.patch`는 원본과 같아 `git am`으로 쓸 수 있다.
- 남은 낱말 확인(`rg --no-ignore`, 사본 전체. 저장소 `.gitignore`가 `*.log`를 빼므로 `--no-ignore` 없이는 로그를 보지 않는다): "PASSWORD"·"SECRET"·"TOKEN"·"_KEY"는 시험 이름(`fake_secrets_never_reach_the_log_even_at_trace`,
  `s3_rejects_a_wrong_secret`, `split_settings_preserve_raw_password_…`), Compose의 "variable is not set" 경고, 문서·리뷰 본문의 변수 이름뿐이다.
  "cookie"는 시험 이름(`duplicate_cookies_are_rejected` 등)·크레이트 이름·설명문뿐이고 값은 없다. 사용자 이름·ESC 문자·`ghh_` 값·`~/` 경로는 0건이다.
  남은 `/home/`은 `apps/web/src/lib/features/home/` 같은 저장소 경로뿐이다. `10.200.x.1`은 Compose 내부 망 주소, `127.0.0.1:5173/w/<uuid>`는 로컬 개발 주소다.
- 환경 변수 파일·`.env`·인증 파일: 원본에 없다(넣은 것 없음).
- 스크린숏: `w17/screenshots/`의 PNG 4장·PDF 14개를 하나씩 열어 합성 데이터만인지 확인했다. 뺀 것 없음. 목록과 확인 결과는 `w17/screenshots/INDEX.md`.
  다른 폴더에는 이미지가 없다.
- 큰 파일: 가장 큰 것이 `d95-pr64-w1b/fixB-run1.log`(원본 1.66 MB, 색 코드를 지운 사본 1.07 MB)로 "수 MB"를 넘는 로그가 없어 **자른 파일은 없다**.
  묶음 전체는 약 27 MB(앞선 `../evidence/`는 3.5 MB).

### 뺀 파일 (2개)

| 원본                                         | 사유                                                                                                       |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `w4-ledger/…/w4/src/ledger.md`               | `089c4e5:docs/handoff/coderabbit-ledger.md`와 바이트까지 같다(같은 브랜치에 이미 있음). W4의 입력 사본이다 |
| `w4-ledger/…/w4/src/resume.md`               | `089c4e5:docs/handoff/audit-resume.md`와 바이트까지 같다. W4의 입력 사본이다                               |

### 내용이 빈 파일 (17개, 그대로 둠)

eslint는 문제가 없으면 아무것도 출력하지 않는다. 0바이트 eslint 로그 16개(`w2-pr82/final-eslint-1640dbc.log`, `w3-pr59/12-eslint.log`·`22-merge72-eslint.log`,
`w11-dialogs/*-eslint.log` 13개)는 그 결과이며, 종료 코드는 각 폴더 설명에 따른다(`w2-pr82/final-eslint.log`에는 `exit=0`이 있다). `w17/logs/b2-4-notes.json`은
"노트 목록이 비어 있음"을 기록한 0바이트 파일이다(`w17/INDEX.md` 항목 2).

## 폴더별 요약

| 폴더 (파일 수)                | 관련 코드 SHA·PR                                                                                                  | 시나리오                                                                                       | 결과                                                                                                                                                         |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `d95-pr64/` (24)              | #64 `5edd8f4..0446746`(e564f03·0fb6b04·6b22bd7·132777f·fb3a11e·0446746), 시험 대부분 `132777f`                     | W1: D-95 편집자 기록, 이동·복원의 잠금 대기와 권한 재확인                                       | check PASS(`0446746`). 세 DB 구성 시험은 `search.rs`만 FAIL(Meilisearch OOM, #89 전), collab 36/36 PASS. 수정 전 FAIL 기록 있음                               |
| `d95-pr64-w1b/` (23)          | #64 `0446746..2acebe9`(8cc4d89·cfae027·56e9108·2acebe9)                                                           | W1b: 이동 중 쓰기 권한 철회 재확인, 웹 미저장 알림, 경합 시험                                   | `cfae027`에서 `--oidc` 306·`--supabase --oidc` 317·session pooler 634 모두 0 실패. check PASS(`56e9108`). 웹 79파일 394(뒤 396)                               |
| `w2-pr82/` (18)               | #82 `263e471` → `1640dbc`                                                                                         | W2: 업무 덮어쓰기 확인의 늦은 응답, 공백·같아 보이는 값 비교                                    | 수정 전 새 시험 FAIL(`263e471`), 수정 뒤 웹 검사 PASS(vitest 77파일 430). 실제 브라우저 NOT RUN                                                              |
| `w3-pr59/` (26)               | #59 `df22615` → `21d92f4`, #72와 임시 합치기 `173e352`                                                            | W3: 일정 충돌 확인의 최신 판 처리, 서버식 trim                                                 | 수정 전 FAIL·변이 시험 FAIL(의도), `21d92f4` 웹 검사 PASS(85파일 436), 합치기 85파일 453 PASS. 실제 브라우저 NOT RUN                                         |
| `w4/` (31)                    | 기준 main `a5cda4e`, #81 `bf56696`, #72 `0213eb8`                                                                 | W4: CodeRabbit OPEN 22건 장부 분류(GitHub GraphQL 수집 `collect/`, 단 댓글 `comments/`)        | 코드 변경·시험 없음(NOT RUN, `ledger-update.md`). 수집 JSON의 로그인은 `coderabbitai`·`fvoci`뿐                                                               |
| `w5/` (22)                    | 통합 dry-run, 기준 `a5cda4e`, 병합 `95a201e`, 통합 `ae68989`(26개 PR)                                             | W5: PR 26개를 병합 순서대로 합친 통합 시험                                                     | `95a201e` check FAIL(웹 오류) → `ae68989` check PASS(cargo deny NOT RUN). `--oidc` 5회 FAIL(`search.rs`, 1회는 `recurrence.rs`), main `a5cda4e`도 같은 FAIL |
| `w6/` (8)                     | #60 `d913314` → `81c9d99` → `0345026`                                                                             | W6: 계정 전환 뒤 문서 저장이 다른 계정으로 나가는 경로                                         | 수정 전 FAIL 두 건, `0345026` 웹 검사 PASS(87파일 420). cargo NOT RUN                                                                                        |
| `w7-pr65/` (13)               | #65 `5400977` → `6e6a860` → `b9f5c22`                                                                             | W7: 늦은 로컬 저장소 게시가 claim을 잃은 시도의 최종 객체를 덮지 않음                          | 수정 전 FAIL → 새 시험 PASS, check PASS(`6e6a860`·`b9f5c22`). `--oidc`·`--supabase --oidc`는 `search.rs`만 FAIL. 뒤 바이너리 NOT RUN                         |
| `w8/` (35)                    | main `a5cda4e`, #89 `7d9529c` → `2518724`                                                                         | W8: 시험 Meilisearch OOM(CPU 많은 호스트), `compose.test.yaml` CPU 한도 2                      | 수정 전 `--oidc` FAIL(OOM, exit 137). 최종 CPU 2에서 `--oidc` 282×2·`--supabase --oidc` 293·check·licenses PASS. session pooler NOT RUN                      |
| `w9/` (15)                    | #66 `611e664` → `5142fb0`                                                                                         | W9: 반복 회차 레인 잠금 시험의 간헐 실패(시험 결함, 시험만 수정)                               | 수정 전 40회 중 1 FAIL, 수정 뒤 0/40. check PASS. `--oidc`는 `search.rs`만 FAIL(OOM, #89 전)                                                                  |
| `w10-expected-user/` (22)     | #60. 로그에 SHA 기록 없음. CodeRabbit 요약 파일 이름의 `1bc1f57`·`1d589bd`·`6ff6187`·`8e42036`·`a811f92`가 #60 머리 | W10: "expected user" 제한(저장·업로드·노트·실시간 연결이 편집을 시작한 계정으로만)             | 수정 전 서버·업로드·웹 FAIL. 수정 뒤 check PASS ×5, `--oidc` PASS ×3, 웹 PASS ×3. check 1회 FAIL은 문서 포맷 경합(`xtask-check-run1-docs-race.log`). README 없음 |
| `w11-dialogs/` (89)           | #59 `21d92f4` → `180a0ae`, #82 `1640dbc` → `4d3ae8a`, 합치기 `0986316`(#59+#72)·`ddac7b9`(#59+#82)                 | W11: 미저장 입력 확인, ClashConfirm 값 상자 키보드, #82 `formatNumber`                         | 수정 전·변이 시험 FAIL 확인, 최종 머리와 합치기의 웹 검사 PASS. 실제 브라우저·Safari·cargo NOT RUN                                                            |
| `w13/` (71)                   | 통합 `audit/int6` `2427544`(27개 PR 머리), 진단 `50a77db`(#64 `0446746`+#89)                                      | W13: 최종 통합 리허설과 PR 갱신 패치(`pr-update-patches/`), 충돌 해결 기록(`resolutions/`)     | `17df953` check FAIL → `2427544` check·`--oidc` 348·`--supabase --oidc` 359 PASS. session pooler 4회 중 3 FAIL(#64 collab이 Supavisor 풀 초과, W14가 고침) |
| `w14/` (125)                  | #89 `2518724` → `01f17fb`, #64 `56e9108`·`2acebe9`, 진단 `891f40e`·`2ddb17c`, 수정 `13b0e7a`·`5c652a0`·`77eea03`   | W14: session pooler에서 collab 시험이 풀(64)·DB 슬롯을 다 씀 → `RUST_TEST_THREADS=4`           | 진단 실행 FAIL 재현, 수정 뒤 세 SHA에서 session pooler 3회씩·`--oidc` 306·`--supabase --oidc` 317·check PASS, `01f17fb` session pooler PASS(586)              |
| `w15/` (3)                    | #69 docs `6ed1df8..4355447`                                                                                       | W15: 감사 기록 94차 개정(D-96~D-98)                                                            | `pnpm install`·`format:check` PASS. cargo check/test·브라우저 NOT RUN(문서만)                                                                                |
| `w16/` (116)                  | 최종 통합 `07be34d2486b2fe60507ccc3156fb7e33b5fde44`(트리 `ff5db09ae482`), 27단계 `final-steps.tsv`               | W16: 27개 PR 최종 통합 고정, `merge-plan.sh`(coordinator용 초안, W16은 실행 안 함), R8 행렬    | R8 a·b·c·d 모두 PASS(아래). 브라우저 NOT RUN(W17이 함). 단계별 전체 검사는 표본만                                                                             |
| `w17/` (113)                  | `07be34d`(같은 최종 통합)                                                                                         | W17: 실제 브라우저 수용 검증 항목 1~9(두 계정 Test User·Second User)                           | 9개 항목 모두 PASS(관찰 2건, 항목 7 잔여 위험 1건은 `INDEX.md`). Safari·Firefox·실기기·화면 낭독기 NOT RUN                                                    |

## 실행별 결과

환경 약어: PG = Compose PostgreSQL, OIDC = Keycloak 포함, SB = 셀프호스트 Supabase(`--supabase`), SP = session pooler(`--session-pooler`, Supavisor),
web = `pnpm run i18n`·eslint·svelte-check·vitest(또는 그 일부). SHA는 로그 머리(`# <sha> …`)에 있으면 그것, 없으면 폴더 설명의 귀속이며 둘 다 없으면 "미기록".
`--no-fail-fast`·`cargo test --workspace …`는 작업자가 원인 확인용으로 직접 돌린 것이다(커밋하지 않은 임시 변경, 폴더 설명 참고).

### 최종 통합 `07be34d` — R8 (`w16/logs/`, 각 1회)

| 로그                                                   | 명령                                                   | 환경             | 결과                                                         |
| ------------------------------------------------------ | ------------------------------------------------------ | ---------------- | ------------------------------------------------------------ |
| `r8a-check-07be34d.log`                                | `cargo xtask check`(licenses·cargo deny 포함)          | WSL2, DB 없음    | PASS `# rc=0`                                                |
| `r8b-test-oidc-07be34d.log`                            | `cargo xtask test --oidc`                              | PG+OIDC          | PASS `# rc=0`, 47개 결과 줄 359 통과·0 실패                  |
| `r8c-test-supabase-oidc-07be34d.log`                   | `cargo xtask test --supabase --oidc`                   | SB+OIDC          | PASS `# rc=0`, 47개 370 통과·0 실패                          |
| `r8d-test-supabase-session-pooler-oidc-07be34d.log`    | `cargo xtask test --supabase --session-pooler --oidc`  | SB+SP+OIDC       | PASS `# rc=0`, 94개(직접·session 각 370) 740 통과·0 실패     |

같은 폴더의 `r8*-pg-activity.txt`·`-max-connections.txt`·`-containers.txt`·`-db.log`·`-supavisor.log`는 연결 수 감시 자료다(`w16/README.md`).
`w16/merge-*.out`·`replay*.log`·`step*.log`는 통합 단계 기록, `*.sh`는 작업자가 쓴 보조 스크립트 사본이다.

### 통합 리허설 `2427544` (`w13/logs/`)

| 로그                                                                        | SHA        | 명령                                                   | 환경        | 결과                                                         |
| --------------------------------------------------------------------------- | ---------- | ------------------------------------------------------ | ----------- | ------------------------------------------------------------ |
| `pnpm-install.log`                                                          | —          | `pnpm install --frozen-lockfile`                       | —           | PASS                                                         |
| `check-17df953.log`                                                         | `17df953`  | `cargo xtask check`                                    | —           | FAIL(svelte-check 1, vitest 3파일 6건)                       |
| `check-2427544.log`                                                         | `2427544`  | `cargo xtask check`                                    | —           | PASS(vitest 123파일 670)                                     |
| `test-oidc-2427544.log`                                                     | `2427544`  | `cargo xtask test --oidc`                              | PG+OIDC     | PASS 348·0                                                   |
| `test-supabase-oidc-2427544.log`                                            | `2427544`  | `cargo xtask test --supabase --oidc`                   | SB+OIDC     | PASS 359·0                                                   |
| `test-supabase-session-pooler-oidc-2427544{,-run2,-run3,-run4}.log`         | `2427544`  | `cargo xtask test --supabase --session-pooler --oidc`  | SB+SP+OIDC  | 1·2·4회 FAIL(`collab.rs` XX000, 뒤 40개 NOT RUN), 3회 PASS   |
| `pr64-0446746+pr89-test-supabase-session-pooler-oidc{,-run2}.log`           | `50a77db`  | 같음                                                   | SB+SP+OIDC  | 1회 PASS(624), 2회 FAIL                                      |
| `pr64-0446746-alone-…-stopped-at-search-oom.log`                            | `0446746`  | 같음                                                   | SB+SP+OIDC  | FAIL(`search.rs` 6, Meilisearch OOM, #89 없음)               |

`docker-events-*.log`·`supavisor-*.log`·`supabase-db-run4-2427544.log`는 보조 자료. NOT RUN: main+#89 session pooler, 이 시점의 실제 브라우저.

### 실제 브라우저 `07be34d` (`w17/`)

`cargo xtask dev --oidc`(새 볼륨, 합성 시드), Orca 내장 Chromium 150, 1280×800·375×812. 항목 1 문서 이동(D-95), 2 서버가 닫은 연결(#64),
3 업무 409(D-94), 4 일정 409(#59), 5 Esc·늦은 응답, 6 미저장 닫기(D-97), 7 계정 전환(D-96·#60), 8 DST(D-93), 9 표시 설정·간트·375px·#82 값 상자:
**모두 PASS**. 항목 1의 방 없는 복원·DB 잠금 대기 편집, Safari·Firefox·실기기·화면 낭독기는 NOT RUN. 항목 7 잔여 위험(전환 전에 열린 실시간 연결이
다시 연결이 거절되기 전 이전 계정 이름으로 글을 보냄)과 관찰 2건은 `w17/INDEX.md`. 장면별 증거는 `screenshots/*.snapshot.txt`·`logs/`.

### #64 D-95 (`d95-pr64/`, `d95-pr64-w1b/`)

| 로그                                                                           | SHA                    | 명령                                                         | 환경        | 결과                                              |
| ------------------------------------------------------------------------------ | ---------------------- | ------------------------------------------------------------ | ----------- | ------------------------------------------------- |
| `d95-pr64/check-3.log`                                                         | `0446746`              | `cargo xtask check`                                          | —           | PASS                                              |
| `d95-pr64/check.log`, `check-2.log`                                            | 미기록                 | `cargo xtask check`(`check-2`는 `--skip-licenses`)           | —           | `check` FAIL(cargo deny 도구), `check-2` PASS     |
| `d95-pr64/licenses.log`                                                        | `132777f`              | `cargo xtask licenses`                                       | —           | PASS                                              |
| `d95-pr64/web-checks.log`                                                      | `e564f03`              | web                                                          | web         | PASS 78파일 392                                   |
| `d95-pr64/nff-132777f-{oidc,supabaseoidc,supabasesessionpooleroidc}.log`       | `132777f`              | `cargo xtask test --oidc` / `--supabase --oidc` / `--supabase --session-pooler --oidc`(임시 `--no-fail-fast`) | PG/SB/SB+SP, OIDC | 297·3 / 308·3 / 309·2 — 실패는 `search.rs`만 |
| `d95-pr64/fixed-test-oidc-3.log`                                               | `132777f`              | `cargo xtask test --oidc`                                    | PG+OIDC     | collab 36/36 PASS, `search.rs` FAIL               |
| `d95-pr64/fixed-test-oidc-{1,2}.log`, `final-test-*.log`, `*-matrix.txt`       | 미기록                 | `--oidc` 또는 `cargo test --workspace`                       | 각 이름대로 | FAIL(중간 기록, 실패는 collab 1 또는 `search.rs`) |
| `d95-pr64/base-5edd8f4-test.log`                                               | `5edd8f4`              | `cargo test --workspace --locked -- --skip oidc_ --skip supabase_` | PG    | FAIL `search.rs`(기준에서도 같음)                 |
| `d95-pr64/prefix{,2,3}-test.log`                                               | `e564f03` WIP / — / `6b22bd7` | 같음(새 시험, 수정 전)                                 | PG          | FAIL(수정 전 재현, `prefix2`는 시험 설정 오류)    |
| `d95-pr64-w1b/8cc4d89-*.log`                                                   | `8cc4d89`              | `--oidc` / `--supabase --oidc` / `--supabase --session-pooler --oidc` | 위와 같음 | PASS 301 / 312 / 624, 0 실패               |
| `d95-pr64-w1b/cfae027-*.log`                                                   | `cfae027`(#89 임시)    | 같음                                                         | 위와 같음   | PASS 306 / 317 / 634, 0 실패                      |
| `d95-pr64-w1b/check-{8cc4d89,cfae027,56e9108}.log`, `check-cr.log`             | 이름대로, `check-cr` 미기록 | `cargo xtask check`                                     | —           | PASS / FAIL(openapi·prettier, 56e9108에서 고침) / PASS / PASS |
| `d95-pr64-w1b/web.log`, `web2.log`                                             | `cfae027` / `2acebe9`  | web                                                          | web         | PASS 79파일 394 / 396                             |
| `d95-pr64-w1b/prefix-*.log`, `prerevert-0446746-room.log`, `observe-races-*.log`, `fixB-run1.log` | 수정 전·측정 | vitest 또는 `cargo test`                         | web / PG    | FAIL(수정 전 재현·측정). `prerevert`에서 실패한 시험은 `a_room_reopened_on_a_pending_state_saves_it_before_a_restore_or_move`(collab 35·1) |

### PR별 작업 (W2·W3·W6·W7·W8·W9·W10·W11·W14·W15)

| 폴더·로그                                                                       | SHA                         | 명령                                              | 환경        | 결과                                                    |
| ------------------------------------------------------------------------------- | --------------------------- | ------------------------------------------------- | ----------- | ------------------------------------------------------- |
| `w2-pr82/*-fail-before*.log`, `verifier-probes-on-263e471.log`                  | `263e471`                   | vitest(새 시험·탐침)                              | web         | FAIL(수정 전 10/50·3/25·1, 탐침 31/34)                  |
| `w2-pr82/final-*.log`                                                           | `1640dbc`                   | prettier·i18n·eslint·svelte-check·vitest          | web         | PASS(vitest 77파일 430, svelte-check 0/0)               |
| `w3-pr59/01-…`, `02`~`06-mutation-*.log`                                        | `df22615` / 변이            | vitest                                            | web         | FAIL(수정 전 2/26, 변이 시험은 의도된 FAIL)             |
| `w3-pr59/10`~`15-*.log`                                                         | `21d92f4`                   | prettier·i18n·eslint·svelte-check·vitest          | web         | PASS(85파일 436)                                        |
| `w3-pr59/2x-merge72-*.log`                                                      | `173e352`(#59+#72)          | 같음                                              | web         | PASS(85파일 453)                                        |
| `w6/before-fix-d913314.log`, `before-fix2-81c9d99.log`                          | `d913314` / `81c9d99`       | vitest(새 시험)                                   | web         | FAIL 2/3, 1/2(수정 전)                                  |
| `w6/web-checks.log`                                                             | `0345026`                   | web                                               | web         | PASS 87파일 420                                         |
| `w7-pr65/before-fix-xtask-test.log`, `review-fix-before-storage-local-direct.log` | `5400977`+새 시험 / `b9f5c22` 전 | `cargo xtask test`, `cargo test -p fvoci-server --test storage local_` | PG / 로컬 | FAIL(수정 전)                          |
| `w7-pr65/after-fix-xtask-check-full.log`, `review-fix-xtask-check.log`          | `6e6a860` / `b9f5c22`       | `cargo xtask check`                               | —           | PASS / PASS(`after-fix-xtask-check.log`는 첫 실행 FAIL: fmt·cargo deny 도구) |
| `w7-pr65/*-xtask-test-oidc*.log`, `*-supabase-oidc.log`                         | `6e6a860` / `b9f5c22`       | `--oidc` / `--supabase --oidc`                    | PG/SB+OIDC  | FAIL `search.rs`만(OOM, #89 전), 뒤 바이너리 NOT RUN   |
| `w7-pr65/*storage-local-direct.log`(수정 뒤)                                     | 미기록 / `b9f5c22`          | `cargo test … --test storage local_`              | 로컬        | PASS 3 / 4                                              |
| `w8/logs/before-a5cda4e-test-oidc.log`                                          | `a5cda4e`                   | `cargo xtask test --oidc`                         | PG+OIDC     | FAIL(`search.rs` 6, Meilisearch oom→137)               |
| `w8/logs/after-*.log`                                                           | 미기록(채택 안 한 시도)     | `--oidc`                                          | PG+OIDC     | PASS 282                                                |
| `w8/logs/final-cpus2-run{1,2}-test-oidc.log`, `final-cpus2-supabase-oidc.log`   | 최종 #89(README 기준)       | `--oidc` ×2 / `--supabase --oidc`                 | PG/SB+OIDC  | PASS 282 ×2 / 293                                       |
| `w8/logs/check-skip-licenses.log`, `licenses.log`                               | 최종 #89                    | `cargo xtask check --skip-licenses` / `licenses`  | —           | PASS / PASS. session pooler NOT RUN                     |
| `w9/logs/repeat-before-611e664.out`, `before-11.log`, `diag-probe.log`          | `611e664`                   | recurrence 바이너리 40회, 탐침                    | PG          | FAIL 1/40, 탐침 결정적 재현                             |
| `w9/logs/repeat-after-5142fb0.out`, `after-probe.log`, `check-5142fb0.log`      | `5142fb0`                   | 40회, 탐침, `cargo xtask check`                   | PG / —      | 0/40 FAIL, 탐침 PASS, check PASS                        |
| `w9/logs/test-oidc-5142fb0.log`, `test-oidc-nofailfast-5142fb0.log`             | `5142fb0`                   | `--oidc`, 임시 `--no-fail-fast`                   | PG+OIDC     | FAIL `search.rs`만(OOM), 나머지 44개 PASS              |
| `w10-expected-user/*before-fix.log`                                             | 미기록(수정 전)             | `cargo test --workspace …`, vitest                | PG / web    | FAIL(expected_user 3, collab 1, upload 1, web 3·1)      |
| `w10-expected-user/{,w10b-}xtask-check*.log`                                    | 미기록                      | `cargo xtask check`                               | —           | PASS ×5, `xtask-check-run1-docs-race.log` FAIL(문서 format:check) |
| `w10-expected-user/{,w10b-}xtask-test-oidc*.log`                                | 미기록                      | `cargo xtask test --oidc`                         | PG+OIDC     | PASS ×3(`exit=0`, 46개 결과 줄 모두 ok)                 |
| `w10-expected-user/{,w10b-}web-checks*.log`                                     | 미기록                      | i18n·vitest                                       | web         | PASS ×3(88파일 424, 89파일 426)                         |
| `w11-dialogs/01`~`07`, `31`, `45`~`49b-*.log`                                   | `21d92f4`·`1640dbc`·변형    | vitest(수정 전·변이)                              | web         | FAIL(수정 전 재현과 변이 시험, 의도)                    |
| `w11-dialogs/10`~`19-*.log`                                                     | #59 `9e226e0`·`180a0ae`     | prettier·i18n·eslint·svelte-check·vitest          | web         | PASS(86파일 450 / 453)                                  |
| `w11-dialogs/40`~`44-*.log`                                                     | #82 `4d3ae8a`               | 같음                                              | web         | PASS(78파일 446)                                        |
| `w11-dialogs/2x`·`5x`·`6x-merge*.log`                                           | 합치기, 최종 `0986316`·`ddac7b9` | 같음                                         | web         | PASS(최종 86파일 470 / 88파일 514). `32-pr82-B-without-ignore-*`는 음성 대조(오류가 나야 맞음) |
| `w14/logs/base-2518724-sp-run{1,2}.log`                                         | `2518724`                   | `--supabase --session-pooler --oidc`              | SB+SP+OIDC  | PASS 586 ×2                                             |
| `w14/logs/diag-891f40e-*.log`, `diag2-2ddb17c-sp-run{1,2,3}.log`                | `891f40e` / `2ddb17c`       | 같음                                              | SB+SP+OIDC  | FAIL / FAIL·PASS(슬롯 가득 4건)·FAIL                    |
| `w14/logs/fix-{13b0e7a,5c652a0,77eea03}-*.log`, `check-*.log`                   | 이름대로                    | session pooler ×3, `--oidc`, `--supabase --oidc`, `check --skip-licenses` | SB+SP / PG / SB, OIDC | PASS 634 ×3, 306, 317, check PASS(세 SHA 모두) |
| `w14/logs/pr89fix-01f17fb-sp-run1.log`                                          | `01f17fb`                   | `--supabase --session-pooler --oidc`              | SB+SP+OIDC  | PASS 586                                                |
| `w15/pnpm-install.log`, `format-check.log`                                      | `4355447`                   | `pnpm install --frozen-lockfile`, `pnpm run format:check` | —   | PASS / PASS. cargo NOT RUN(문서만)                      |

### 통합 dry-run (`w5/logs/`)

| 로그                                                     | SHA       | 명령                                         | 환경     | 결과                                                    |
| -------------------------------------------------------- | --------- | -------------------------------------------- | -------- | ------------------------------------------------------- |
| `pnpm-install.log`                                       | `95a201e` | `pnpm install --frozen-lockfile`             | —        | PASS                                                    |
| `check-95a201e-raw.log`, `web-*-95a201e-raw.log`         | `95a201e` | `cargo xtask check --skip-licenses`, web     | web      | FAIL(eslint 2, svelte-check 5, vitest 1/638)            |
| `check-ae68989.log`                                      | `ae68989` | `cargo xtask check --skip-licenses`          | —        | PASS(vitest 638), cargo deny NOT RUN                    |
| `test-oidc-ae68989{,-run2..5}.log`                       | `ae68989` | `cargo xtask test --oidc`                    | PG+OIDC  | FAIL ×5(`search.rs`, run2는 `recurrence.rs` 1)          |
| `test-oidc-ae68989-nofailfast.log`                       | `ae68989` | 임시 `--no-fail-fast`                        | PG+OIDC  | 330 통과·5 실패(`search.rs`)                            |
| `baseline-main-a5cda4e-test-oidc.log`                    | `a5cda4e` | `cargo xtask test --oidc`                    | PG+OIDC  | FAIL(`search.rs`, main에서도)                           |
| `docker-events-meili-run5.log`                           | `ae68989` | `docker events`                              | —        | Meilisearch `oom` → exit 137                            |

## 남은 확인 거리 (이 묶음에서 바꾸지 않음)

- `d95-pr64/` 일부 로그(`check.log`·`check-2.log`·`fixed-test-oidc-1/2`·`final-test-*`·`*-matrix.txt`)와 `w10-expected-user/` 전체는 실행 SHA가 기록되지 않았다.
- `d95-pr64-w1b/SUMMARY.md`는 `prerevert-0446746-room.log`를 "되돌린 stand-in 동작 시험 실패"라고 적었지만 로그에서 실패한 시험 이름은
  `a_room_reopened_on_a_pending_state_saves_it_before_a_restore_or_move`이고 stand-in 시험(`a_connected_writer_saving_for_an_editor_is_recorded_as_a_stand_in`)은 ok다.
- `w2-pr82/report.md`가 인용한 `final-eslint-1640dbc.log`는 0바이트이고, 같은 실행의 종료 코드는 `final-eslint.log`의 `exit=0`에 있다.
- `w3-pr59/`·`w11-dialogs/`의 i18n 로그는 명령 줄만 있고 종료 코드가 없다(폴더 설명은 PASS).
