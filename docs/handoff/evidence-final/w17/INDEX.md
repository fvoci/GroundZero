# W17 최종 통합본 실제 브라우저 수용 검증 (SHA 07be34d)

- SHA: `07be34d2486b2fe60507ccc3156fb7e33b5fde44` (로컬 `audit/final`, 작업 트리 깨끗함, 코드 수정 없음).
- 환경: 이 체크아웃의 `cargo xtask dev --oidc`(새 볼륨 `fvoci-w13-int6-207f0e-dev_*`로 시작, 합성 시드 "모듈 예제 (합성 데이터)"·"개발"), 서버 8080·웹 5173·Keycloak 8180.
  실행 기록 `logs/dev-stack.log`. 2026-10-22 07:58~08:17 UTC 무렵.
- 브라우저: Orca 내장 브라우저, Chromium 150.0.7871.250(UA `Chrome/150.0.7871.250`, Linux). 기본 화면 1280×800(DPR 1).
  좁은 화면은 `set viewport 375 812 2` + `set device "iPhone 15"`(모바일 UA, DPR 2) 에뮬레이션.
- 데이터: 합성 데이터만. 계정 `test-user`(Test User)·`test-user-2`(Second User). 비밀번호·쿠키·토큰 값은 출력·기록하지 않음(로그인은 값을 변수로만 넘기는 스크립트).

## 두 사용자·두 세션 방법

- 다른 사용자: Orca 브라우저 **격리 프로필**(`w17-user2`, partition `persist:orca-browser-session-…`)의 탭에 `test-user-2`로 로그인. 이 탭은 처음에 로그인 화면이 떠
  기본 프로필(`test-user`)과 쿠키를 나누지 않음을 확인했다. curl·Python은 쓰지 않았다.
- 계정 전환(항목 7): **기본 프로필 탭 두 개**(편집 탭 A, 전환 탭 A2). A2가 로그인 없이 Test User로 열려 쿠키를 나눔을 확인.
- API 준비·확인은 각 탭 안의 `fetch`(그 탭의 세션)로 했다(`logs/*.json`).

## 증거 형식과 한계 (중요)

- **스크린숏 대부분 NOT RUN(렌더링 불가)**: `orca screenshot`이 데스크톱 창이 그려지지 않아 빈 회색 PNG를 돌려줬다(data: URL 시험 페이지도 87바이트).
  coordinator에 알렸고(지시: snapshot·eval·network 증거로 대신, PDF 시험, 주기 재시도) 렌더링된 4장만 PNG로 남겼다:
  `b4-2`, `b4-3`, `b4-4b`, `b9-4`. 나머지는 **같은 순간의 접근성 snapshot 텍스트**(`screenshots/*.snapshot.txt`, 첫 줄에 시각·URL·화면 크기)와
  eval로 읽은 DOM 상태·요청 기록(`logs/`)이 증거다.
- `*.pdf`: `orca pdf`(인쇄 렌더링) 사본. 이 환경에 PDF 보기 도구가 없어 **내용을 눈으로 확인하지 못했다**(보조 자료).
- 요청 확인: `orca network`는 `method_not_found`라 쓰지 못했고, agent-browser `network requests`(방법·주소·상태·`x-fvoci-expected-user` 머리 유무만 추림,
  쿠키 값 없음)와 서버 접근 로그의 `error_code`로 확인했다.
- 실시간 닫기 코드: 서버 로그에 남지 않아, 페이지 안에 WebSocket을 감싸 open/close 코드를 기록하는 시험용 훅을 SPA 이동 전에 넣었다(`logs/*ws-log*`).
- 시험용 훅(코드 변경 아님, 페이지 메모리에만): 저장 중 상태를 만들려고 `fetch` 전송을 8~10초 늦춤(항목 6), 늦은 응답을 만들려고 체크리스트 응답을 25초 붙잡음(항목 5),
  확인과 전송 사이의 계정 전환 창을 만들려고 문서 PUT을 45초 붙잡음(항목 7). 클릭은 `mouse move/down/up`(실제 CDP 입력), 입력은 키 입력.

## 결과 요약

| 항목 | 결과 | 비고 |
| --- | --- | --- |
| 1 문서 이동 전후·대리 저장 이력·실패·취소·방 없는 대기 편집 | **PASS** (방 없는 대기 편집의 **복원** 경로·DB 잠금 대기 중 편집은 NOT RUN) | 이력·내보내기에 "편집: Second User · 대신 저장: Test User" |
| 2 4403/4409 닫힘의 미확인 편집 초안·안내·재개방 | **PASS** (관찰 1건) | 4409 뒤에는 "저장되지 않음" 줄 대신 초안 제안이 바로 보임(템플릿 설계, 대상 실시간 켜짐·꺼짐 둘 다) |
| 3 업무 409 다시 저장(D-94) | **PASS** | 지난 FAIL(B가 A의 제목을 되돌림) 재확인 PASS |
| 4 일정 409 다시 저장·시간 묶음 | **PASS** | |
| 5 Escape·재개방·늦은 응답 | **PASS** | |
| 6 미저장 닫기(D-97) | **PASS** (관찰 1건) | 저장 중에는 초점이 잠시 body로 감(저장 뒤 복원) |
| 7 계정 전환(D-96) | **PASS** (남는 위험 관찰) | 서버 409 `account_changed` 2건, 실시간 재연결 4412 |
| 8 D-93 DST | **PASS** | 손대지 않은 시작 `05:10:17Z` 초까지 보존 |
| 9 표시 설정·간트·375px·키보드·#82 값 상자 | **PASS** (Safari NOT RUN) | |

FAIL 없음. NOT RUN: 실제 휴대 기기, Safari(WebKit)·Firefox, 화면 낭독기, Safari 값 상자 키보드 스크롤, 방 없이 남은 저장 대기 편집의 **복원**(이동만 확인),
이동 트랜잭션의 DB 잠금 대기 중 들어온 편집(브라우저로 잠금 대기를 만들 수 없음. 이동 직전·닫히기 직전 편집으로 대신 확인).

## 항목별 증거

공통: SHA `07be34d`, Orca 내장 Chromium 150, 1280×800(따로 적은 것 제외). 파일은 `screenshots/<이름>.snapshot.txt`(+ `.png`/`.pdf`가 있으면 함께).

### 1. 문서 이동 (D-95, #64) — PASS

준비: A가 워크스페이스 "W17 이동 대상 (합성)"(실시간 꺼짐)과 '개발'(실시간 켜짐) 위키에 "W17 이동 문서 1~5" 생성(`logs/b1-setup.txt`).

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b1-1-move-dialog-preview-loses-second-user | B가 문서 3에서 실시간으로 씀. A가 이동 대화상자에서 대상 고름 | 미리보기 "더 이상 볼 수 없게 되는 사람: Second User" |
| (logs/b1-3-after-cancel.json) | A가 "취소" | 이동 요청 없음(미리보기 POST만), 문서는 '개발'·리비전 1, B는 "Live"로 계속 씀 **PASS** |
| b1-2-move-409-access-changed, logs/b1-2-after-409.json | A가 미리보기를 연 채로 대상 워크스페이스에 B를 초대·B가 수락, 그 뒤 A가 "이동" | `POST …/move` 409, "그사이 접근할 수 있는 사람이 바뀌었습니다. 다시 확인하세요.", 미리보기 갱신. 문서는 옮겨지지 않았고 B의 실시간 글은 리비전 2(작성자 Second User)에 남음, B는 계속 Live **PASS** |
| b1-4-b-after-move-doc1, logs/b1-4-* | 문서 1: B가 씀(확인됨), A가 이동 | 200. B 연결 **4409 "document changed; reload"**, 다시 붙기 1006(대상은 실시간 꺼짐) → 실시간 아닌 편집기 "Saved". 옮긴 문서(대상 워크스페이스) 리비전 2(작성자 Second User: 이동 전 체크포인트 때 편집자가 쓸 수 있어 대신 저장 아님)에 B의 글 전부 **PASS** |
| b2-1-b-after-overlap-move-doc3, logs/b2-1-* | 문서 3: B가 약 0.29초마다 계속 입력하는 중에 A가 이동 | 4409 @07:59:15.857. 옮긴 문서 리비전 3에 닫히기 전까지 친 "겹1…겹24" 전부(3초 저장 확인 전 것 포함). 닫힌 뒤 입력은 반영 안 됨, 남은 초안 없음 **PASS** |
| logs/b2-2-*, logs/b1-5-after-roomless-move.json | 문서 2: B가 계속 입력하는 중 A가 B를 '개발' 독자로 강등(API 204) | 약 9.5초 뒤 B 연결 **4403 "access ended"**, 편집기 읽기 전용·"Your access … changed" 안내. 쓸 수 있는 사람이 없어 방이 저장 대기 상태로 닫힘. A가 **연결하지 않은 채** API로 이동(방 없는 길) → 200, 리비전 2 작성자 Test User, `edited_by_names` ["Second User"], 본문 끝 "…강52" = B 화면의 마지막 글 **PASS** |
| b1-6-revision-history-stand-in-label (+pdf) | A가 옮긴 문서 2의 "기록" | 목록 "r2 편집: Second User · 대신 저장: Test User", "r1 Test User" **PASS** |
| b1-7-export-stand-in-label (+pdf) | A가 "내보내기" | 리비전 선택 "리비전 2 (최신) · 2026. 10. 22. 04:00 · 편집: Second User · 대신 저장: Test User" **PASS** |

### 2. 실시간 닫힘과 미확인 편집 (#64 W1b) — PASS

미확인 편집은 `set offline on`으로 만들었다(열린 WebSocket은 유지되고 앱은 "Can't reach the server…"를 보임, 입력은 서버 확인을 받지 못함).

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b2-3-b-4403-unsaved-kept-notice (+pdf), logs/b2-3-* | 문서 4: B 온라인 글(확인됨) → 오프라인에서 "오프라인에서 쓴 글 (서버 미확인)" → A가 B를 독자로 강등 | **4403 "access ended"** @08:02:33.8. 안내 "Some of your edits were not saved on the server. They are kept as a draft in this browser; when the document opens again you can keep them as a note or discard them." 초안 키 1개 **PASS** |
| b2-4-b-reopen-draft-offer (+pdf), logs/b2-4-kept-note.json | 다시 열기(새로고침, 독자) | "Edits to this document that never reached the server are still in this browser (…). They won't be merged into the live document. Keep them as a note or discard them." [Keep my draft as a note][Discard draft] → 보존: `POST /api/me/notes` 201·PUT 200(기대 사용자 머리 보냄), 노트 "… (my draft)"에 오프라인 글 포함 **PASS** |
| b2-5-b-4409-draft-offer-nonlive (+pdf), b2-6-…, logs/b2-5-*, b2-6-* | 문서 5: 오프라인 입력 중 A가 API로 이동(대상 실시간 꺼짐) → 온라인 | **4409** @08:04:09.76 → 1006(대상 실시간 꺼짐) → 실시간 아닌 편집기에 "There's an unsaved draft in this browser" [Restore draft][Discard draft]. 복원 → 자동 저장 409(기준 리비전 1) → 비교 화면 [Keep my draft as a note][Save my draft again][Use the server version] → "Save my draft again"은 확인 대화상자를 먼저 연다(이때 PUT 없음: `logs/b2-6-EMPTY-click-opened-confirm-no-put.txt`) → "Save again" → PUT 200(`b2-6-save-draft-again.txt`), 옮긴 문서 리비전 3 "이동 전 온라인 글 / 이동 중 오프라인 글" **PASS** |
| b2-7-b-4409-live-target-draft-offer, logs/b2-7-* | 문서 6: 대상 워크스페이스 실시간을 켜고(204) 같은 순서(오프라인 입력 → 이동 → 온라인) | **4409** @08:21:45.666 → 새 자리에 바로 다시 붙음(Live). 화면은 서버 내용(온라인 글)이고 초안 제안 "Edits to this document that never reached the server are still in this browser (…). They won't be merged into the live document. Keep them as a note or discard them." **PASS** |

관찰: 두 4409 경우 모두 "Some of your edits were not saved…" 줄(`documents_live_unsaved_kept`)은 보이지 않고 초안 제안이 바로 보였다. 템플릿이 제안이 떠 있을 때는
그 줄을 감추게 되어 있다(`LiveDocumentEditor.svelte:436-441`, `unsaved === 'kept' && found === undefined`). 제안 문구가 같은 내용(서버에 닿지 않음, 보존·버리기)을
알리므로 설계대로로 본다. 4403 경우(문서 4)에는 다시 열기 전이라 그 줄이 보였다. 편집은 어느 경우에도 초안으로 남아 잃지 않았다.
(문서 2의 4403 실행에서는 서버가 모든 입력을 이미 받아 미확인 편집이 없어 초안·안내가 없었다. 서버 내용과 화면이 같아 올바름.)

### 3. 업무 409 다시 저장 (D-94, #82) — PASS

업무 DEVP-9 "API 연결 점검"(모듈 예제). A = Test User 탭, B = Second User 탭(오래된 화면).

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b3-1-task-409-notice-input-kept (+pdf), logs/b3-1-after-save-mine.json | A가 제목 저장(판 3). B가 설명만 고쳐 저장 → "Save mine again" | B 저장 409, "Someone else changed this task in the meantime (now titled “API 연결 점검 (A가 고친 제목)”…). Your input is kept." 다시 저장 → 200, 판 4: **제목 = A의 값, 설명 = B의 값**. 지난 FAIL 재확인 **PASS** |
| b3-2-task-clash-both-values (+pdf) | 같은 칸: A 설명 "A 설명 1"(판 5), B 설명 "B 설명" → 409 → Save mine again | 요청 없이 확인 "Someone else also changed some of these fields": Description · Server now "A 설명 1" / Yours "B 설명" **PASS** |
| b3-3-task-overwrite-409-again, b3-4-task-asks-again-newer-value, logs/b3-3-after-overwrite.json | 확인이 떠 있는 동안 A가 "A 설명 2"(판 6) → B "Overwrite with mine and save" | PUT 409 → 확인 닫힘·충돌 알림 유지·초점 "Save mine again" → 다시 누르면 Server now "A 설명 2"로 다시 물음 → 덮어쓰기 200 판 7, 설명 "B 설명", 제목 유지 **PASS** |
| logs/b3-5-*, b3-6-after-same-set-assignees.json | 담당자 순서: A가 API로 [Second, Test] 순서로 저장(판 11). B는 같은 집합(체크 추가) + 예상 시간 45 | 409 → Save mine again → 확인 없이 PUT 200 판 12, 예상 45·담당자 집합 유지 **PASS**. 손대지 않은 담당자는 서버 값 유지(태그만 다시 저장, 판 9) |
| b3-7-task-clash-same-name-assignees (+pdf) | 개발 Keycloak에서 test-user-2 이름을 잠시 "Test User"로 바꿔 다시 로그인(끝난 뒤 "Second User"로 되돌리고 다시 로그인). A는 담당자 #9221만, B는 #a6e2에 #9221 추가 | 체크박스 "Test User #a6e2"/"Test User #9221", 확인 Server now "Test User #9221" / Yours "Test User #9221, Test User #a6e2" **PASS** |

### 4. 일정 409 다시 저장 (D-94, #59) — PASS

일정 "W17 DST 합성 일정"(America/New_York). B 화면 시간대는 Asia/Seoul(보는 사람 시각 + 일정 시간대 시각 함께 표시).

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b4-1-event-409-notice (+pdf), logs/b4-1-after-save-mine.json | A 제목 저장, B 설명 저장 → 409 → Save mine again | 200: 제목 A, 설명 B, 시작 05:10:17Z·끝 06:30:00Z 그대로 **PASS** |
| **b4-2-event-clash-time-bundle.png** (+snapshot, pdf), b4-3-event-after-overwrite.png, logs/b4-2-*.json | A는 끝만 01:45(첫 번째, 05:45Z), B는 시작만 01:20(First, UTC−04:00) | 409 → 확인 한 묶음 "Date, time, time zone and all-day": 서버 2:10–2:45 PM / 내 입력 2:20–3:30 PM(NY 1:10/1:20 AM). 덮어쓰기 → 200 시작 05:20Z·끝 06:30Z(B 묶음 전체) **PASS** |
| b4-4-event-asks-again-newer, **b4-4b-…png**, logs/b4-4-after-repeat.json | 확인 중 A가 설명 바꿈(API) → 덮어쓰기 | 409 → Save mine again → "Now on the server: A 일정 설명 2" → 덮어쓰기 200 판 10 **PASS** |

### 5. Escape·재개방·늦은 응답 (#82·#59) — PASS

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| logs/b5-1-task-clash-click-then-esc.txt | 업무 충돌 확인에서 Esc; 안내문 p·라벨 "Description"·dt·서버 값 상자·내 값 상자를 실제 마우스로 누른 뒤 Esc(각각 다시 열어) | 매번 확인만 닫힘, 업무 대화상자 유지, 입력 "B 설명" 유지, 초점 "Save mine again" **PASS** |
| b5-2-event-x-during-clash | 일정 충돌 확인이 떠 있을 때 대화상자 X | 미저장 확인이 위에 뜸 → "Keep editing" → 충돌 확인 그대로·입력 유지·초점 X → Esc는 충돌 확인만 닫음 **PASS** |
| b5-3-task-late-response-confirm-kept | 체크리스트 토글 응답을 25초 붙잡음 → 그사이 409·충돌 확인 → A가 설명을 또 바꿈(판 16) → 붙잡은 응답이 돌아와 업무를 다시 읽음 | 확인 유지, 서버 값이 "A 늦은 응답 설명 2"로 갱신, 초점 확인 제목(H3) 안. Esc는 확인만. 덮어쓰기 판 17 **PASS** |
| (항목 6 참조) | 대화상자 닫았다 다시 열기 | 변경이 있으면 닫기 전에 확인(계속 편집 시 입력·충돌 상태 보존), 버리기를 고른 뒤 다시 열면 서버 값 **PASS** |

### 6. 미저장 닫기 (D-97) — PASS

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| (notes) | 일정·업무 대화상자 변경 없이 Esc | 바로 닫힘, 초점은 그 일정·업무 카드 |
| b6-1-event-esc-discard-confirm | 일정 제목 고친 뒤 Esc | 확인 "저장하지 않은 변경이 있습니다" [계속 편집][변경 버리고 닫기]. 확인에서 Esc → 확인만 닫힘, 입력 유지, 초점 제목 칸 |
| b6-2-event-close-button-confirm | 바깥 클릭(30,400)·X 단추 | 각각 확인. 계속 편집 → 초점 대화상자 안(바깥 클릭)·X(닫기 단추) |
| (notes) | 확인의 설명문·제목 글자를 누른 뒤 Esc | 확인만 닫힘, 초점 제목 칸 |
| b6-3-event-saving-not-closed, logs/b6-5-task-saving-not-closed.txt | 저장 요청을 8~10초 늦추고 저장 중 Esc·바깥 클릭·X | 일정·업무 모두 닫히지 않음(필드 비활성, X 숨김). 저장 200 뒤 대화상자 유지 |
| (notes) | 변경 버리고 닫기 → 다시 열기 | 닫힘·저장 요청 없음·다시 열면 저장된 값 |
| b6-4-task-esc-discard-confirm | 업무(담당자 바꾸고 409 알림이 있는 상태)에서 Esc·확인 안 Esc·글자 누른 뒤 Esc·바깥 클릭·X·버리기 | 각각 위와 같음, 409 알림·입력 유지, 버리기 뒤 서버 판 14 그대로 |

관찰: 저장 중에는 필드가 비활성이라 초점이 잠시 body로 간다(저장 뒤 제목 칸·저장 단추로 돌아옴). D-97 위반은 아님.

### 7. 계정 전환 (D-96, #60) — PASS (남는 위험 관찰)

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b7-1-after-switch-second-user-no-draft, logs/b7-1-*, b7-server-account-changed.jsonl | 실시간 꺼진 문서에서 Test User 입력 → 자동 저장의 `/api/me` 확인(200 Test User) 뒤 PUT을 붙잡고 A2에서 test-user-2로 전환 | PUT **409, 서버 `error_code: account_changed`** (08:08:04.879Z). 탭이 Second User로 다시 읽힘: 서버 본문 그대로(리비전 2), 초안 제안 없음. Test User 입력은 `fvoci:draft:<…a6e2>:<문서>` 키에 남음 **PASS** |
| b7-2-back-to-test-user-own-draft-offer (+pdf), logs/b7-2-* | Second User가 "저장" 단추(직접 저장), PUT을 붙잡고 A2에서 test-user로 전환 | **409 account_changed** (08:09:25.811Z). Test User 화면에는 Test User 자신의 초안만 제안("이 브라우저에 저장하지 않은 초안이 있습니다") → 복원 → 리비전 3 저장. Second User 입력은 9221 키에만 **PASS** |
| b7-3-retry-stopped-second-user-view, logs/b7-3-retry-precheck.txt | Test User 오프라인 → 자동 저장 실패·"재시도 대기" → A2에서 전환 → 온라인 | 재시도의 `/api/me` 확인이 다른 계정을 보고 **PUT을 보내지 않음**, 탭 다시 읽기. 서버 `account_changed` 수 그대로, 문서 리비전 3 그대로. 초안 키가 사용자별로 분리됨 **PASS**(클라이언트 앞단 확인 경로) |
| b7-4-after-4412-second-user-view, logs/b7-4-live-ws-log.txt | 실시간 문서: Keycloak만 로그아웃 → 앱에서 다른 계정으로 로그인(세션 회전) → 이 탭이 소켓을 닫아 끊김을 흉내 | 다시 붙기 **4412 "signed-in account changed"** @08:15:00.838 → 탭이 새 계정으로 다시 읽힘. 새 계정 화면에는 그 계정 자신의 초안만, 앞 계정 입력은 앞 계정 키에 남음 **PASS** |

- 앱 로그아웃으로 전환하면 앞 세션이 끝나 열린 연결은 재확인에서 **4403 "access ended"**로 닫힌다(다시 붙기 없음, 4412가 아님). 4412는 위처럼 끊김 뒤 다시 붙을 때만 나온다.
- **남는 위험(관찰, 문서화된 것)**: 새 로그인으로 앞 세션이 이미 회전(삭제)된 뒤, 다음 재확인(기본 30초) 전에 끊기기 전의 앞 계정 연결로 보낸 글이 받아들여져
  실시간 문서에 앞 계정 이름으로 반영됐다. `docs/development.md` 96절 "남는 위험"(바꾸기 전에 열려 있던 연결은 처음 사용자 세션으로 남고, 그 세션이 끝나면
  재확인에서 4403)과 재확인 간격만큼의 창이다. 다른 계정 이름으로 쓰인 것은 아니다.

### 8. D-93 DST — PASS

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| logs/b8-create-event.txt | API로 일정 생성: 2026-11-01 시작 05:10:17Z(01:10:17 EDT), 끝 07:00:42Z, America/New_York | 201 |
| b8-1-end-0130-repeated-choice (+pdf) | 표시 설정 America/New_York에서 끝을 키보드로 01:30 | "종료: 이 시각은 America/New_York에서 두 번 있습니다(시계를 되돌리는 날). 어느 쪽인지 고르세요." 라디오 **"첫 번째 01:30 (UTC−04:00)" / "두 번째 01:30 (UTC−05:00)"** |
| b8-2-save-blocked-until-chosen | 고르지 않고 저장 | 요청 없음, "종료: 두 번 있는 시각입니다. 저장하기 전에 첫 번째와 두 번째 중 하나를 고르세요." |
| b8-3-saved-second-start-seconds-kept, logs/b8-after-save.txt | 두 번째 선택 후 저장 | PUT 200 → GET 시작 **05:10:17Z(초까지 그대로)**, 끝 06:30:00Z **PASS**. (영어 화면 B에서도 "First 1:20 AM (UTC−04:00)/Second …" 표시, 항목 4) |

### 9. 표시 설정·간트·좁은 화면·키보드·값 상자 — PASS (Safari NOT RUN)

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b9-1-settings-after-reload-ko-ny-mon-24h (+pdf) | 표시 설정에서 한국어·America/New_York·월요일·24시간 고르고 2초 뒤 값 확인 → 저장 → 새로고침 → 다시 엶 | 고른 값 되돌아가지 않음(#80), `/api/me` ko·America/New_York·mon·h23, 대화상자 같은 값, 화면 한국어 **PASS** |
| b9-2a/b9-2b, logs/b9-2-gantt-*.json | 간트 DEVP-9 막대를 실제 마우스로 +160px(80px/일 × 2) 끌기 | PUT, 마감 10-26 → 10-28(판 18→19) **PASS** |
| b9-3a-375-task-dialog, logs/b9-3-375-overflow.txt | 375×812 DPR 2, iPhone 15 UA: 홈·항목·달력·업무 대화상자 | 문서 scrollWidth 360(스크롤바 15px)·375, 가로 넘침 없음. 간트는 안에서만 스크롤(448/326) **PASS** |
| logs/b9-5-375-task-dialog-tab-order.txt, b9-5-375-esc-focus-returns-card | 375px 업무 대화상자에서 Tab 45번 → Esc | 초점 이동 37번 모두 대화상자 안, 모두 `:focus-visible`+초점 테두리, 순서 제목>상태>우선순위>시작>마감>설명>…>저장>…>댓글>고정>즐겨찾기>지우기 순환. Esc → 닫히고 초점이 그 카드 **PASS** |
| **b9-4-value-box-scrolled-by-keyboard.png** (+snapshot), logs/b9-4-value-box-keyboard-scroll.txt | #82 충돌 확인의 60줄 서버 값(높이 1358/240): Shift+Tab으로 값 상자 → ArrowDown·PageDown·End·Home | 값 상자 `role=region tabindex=0`, 초점 표시. scrollTop 40→80→120→330→1118(끝)→0 **PASS(Chromium)**. Safari NOT RUN |

## 정리·남은 것

- 바꾼 상태(합성 데이터만): 새 워크스페이스 "W17 이동 대상 (합성)"(test-user-2를 작성자로 초대·수락, 마지막 시험을 위해 **실시간 켬**), '개발'·'모듈 예제'의
  W17 문서 1~6·계정 전환 문서·일정·체크리스트, 업무 DEVP-9 값, test-user-2의 '개발' 역할은 마지막에 **작성자**로 되돌림(API로 확인),
  Keycloak test-user-2 이름은 "Second User"로 되돌림. 브라우저 프로필 `w17-user2`를 하나 만들었다(지우지 않음). 내장 브라우저 탭 3개(A·A2·B)는 열어 둠.
- 개발 스택(`logs/stack-stopped-state.txt`): 08:22:46Z `cargo xtask dev` 프로세스 그룹에 SIGINT(Ctrl-C와 같음) → 서버 "shutdown complete", Vite 종료,
  8080·5173 응답 없음. Ctrl-C는 개발 서비스 컨테이너를 남겨 Keycloak이 8180에 계속 응답했으므로, coordinator 승인을 받아 이 체크아웃 프로젝트만
  `docker compose --project-name fvoci-w13-int6-207f0e-dev --file deploy/compose.dev.yaml --profile oidc stop`(down 아님) → 컨테이너 3개 Exited, 8180 응답 없음.
  볼륨 `fvoci-w13-int6-207f0e-dev_{pgdata,kcdata,meilidata}` 남음. `cargo xtask down`·볼륨 삭제는 하지 않았다. 다른 체크아웃은 건드리지 않았다.
