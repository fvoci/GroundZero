# W17 스크린숏 목록 (SHA 07be34d)

- 빌드: 최종 통합 `07be34d2486b2fe60507ccc3156fb7e33b5fde44`, `cargo xtask dev --oidc`, Orca 내장 Chromium 150, 1280×800(좁은 화면은 375×812).
  항목별 시나리오·판정·한계는 상위 `../INDEX.md`(W17 원본)와 `../notes.md`에 있다. 이 파일은 공개 사본을 만들 때(W18) 붙인 목록이다.
- 형식: 렌더링된 PNG 4장, `orca pdf` 인쇄 사본 PDF 14개, 같은 순간의 접근성 snapshot 텍스트 41개(`*.snapshot.txt`). 나머지 장면은 PNG가
  빈 회색으로 찍혀 snapshot 텍스트와 `../logs/`가 증거다(`../INDEX.md` "증거 형식과 한계").
- **공개 전 확인(W18, 2026-10-22)**: PNG 4장과 PDF 14개를 하나씩 열어 봤다. 보이는 것은 합성 데이터뿐이다(작업 공간 "모듈 예제 (합성 데이터)"·"개발",
  계정 Test User·Second User, 제목에 "합성"·"W17"). 비밀번호·토큰·쿠키·이메일·실명·로컬 경로·주소창·다른 앱 알림은 없다. PDF 안 링크 주석은
  `http://127.0.0.1:5173/w/<uuid>` 같은 로컬 개발 주소뿐이고, 문서 정보는 Creator Chromium·Title fvoci뿐이다. 뺀 파일은 없다.
  원본 `../INDEX.md`의 "PDF 내용을 눈으로 확인하지 못했다"는 W17 당시 사정이고, 이번에 열어 보니 각 장면이 제대로 인쇄돼 있다.
- snapshot 텍스트도 다른 사본과 같은 규칙으로 지웠다(바뀐 것 없음). 표시되는 `#9221`·`#a6e2` 같은 꼬리표는 합성 계정 ID 조각이다.

| 파일                                          | 항목(`../INDEX.md`)                  | 장면                                                     | 확인        |
| --------------------------------------------- | ------------------------------------ | -------------------------------------------------------- | ----------- |
| `b1-6-revision-history-stand-in-label.pdf`    | 1 문서 이동(D-95)                    | 리비전 기록 "편집: Second User · 대신 저장: Test User"   | 합성만      |
| `b1-7-export-stand-in-label.pdf`              | 1 문서 이동(D-95)                    | 내보내기 상자의 같은 표시                                | 합성만      |
| `b2-2-b-4403-unsaved-kept-notice.pdf`         | 2 서버가 닫은 연결(#64)              | 권한 강등 4403 뒤 다시 연결 안내                         | 합성만      |
| `b2-3-b-4403-unsaved-kept-notice.pdf`         | 2 서버가 닫은 연결(#64)              | 확인 안 된 편집이 브라우저 초안으로 남았다는 안내        | 합성만      |
| `b2-4-b-reopen-draft-offer.pdf`               | 2 서버가 닫은 연결(#64)              | 다시 열 때 초안을 노트로 남길지 묻기                     | 합성만      |
| `b2-5-b-4409-draft-offer-nonlive.pdf`         | 2 서버가 닫은 연결(#64)              | 4409 뒤 비실시간 문서의 초안 복원 제안                   | 합성만      |
| `b3-1-task-409-notice-input-kept.pdf`         | 3 업무 409 다시 저장(D-94)           | 409 안내, 입력 유지                                      | 합성만      |
| `b3-2-task-clash-both-values.pdf`             | 3 업무 409 다시 저장(D-94)           | 같은 칸 충돌, 서버 값과 내 값                            | 합성만      |
| `b3-7-task-clash-same-name-assignees.pdf`     | 3 업무 409 다시 저장(D-94)           | 이름이 같은 담당자 구분 표시                             | 합성만      |
| `b4-1-event-409-notice.pdf`                   | 4 일정 409 다시 저장(#59)            | 일정 409 안내                                            | 합성만      |
| `b4-2-event-clash-time-bundle.pdf` / `.png`   | 4 일정 409 다시 저장(#59)            | 날짜·시간·시간대 묶음 충돌(DST 1:10/1:20 AM)             | 합성만      |
| `b4-3-event-after-overwrite.png`              | 4 일정 409 다시 저장(#59)            | 덮어쓴 뒤                                                | 합성만      |
| `b4-4b-event-after-repeat-overwrite.png`      | 4 일정 409 다시 저장(#59)            | 다시 덮어쓴 뒤                                           | 합성만      |
| `b7-2-back-to-test-user-own-draft-offer.pdf`  | 7 계정 전환(D-96, #60)               | 원래 계정으로 돌아와 자기 초안만 제안                    | 합성만      |
| `b8-1-end-0130-repeated-choice.pdf`           | 8 DST(D-93)                          | 끝 01:30이 두 번 있는 날 선택 요구                       | 합성만      |
| `b9-1-settings-after-reload-ko-ny-mon-24h.pdf`| 9 표시 설정                          | 새로고침 뒤 한국어·New York·월요일·24시간 유지           | 합성만      |
| `b9-4-value-box-scrolled-by-keyboard.png`     | 9 #82 값 상자                        | 긴 서버 값 상자를 키보드로 스크롤                        | 합성만      |
