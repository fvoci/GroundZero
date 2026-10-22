# 통합본 d7ea7fc 실제 브라우저 확인 (로컬, Git 밖)

- SHA: `d7ea7fc` (로컬 `audit/integration-3`: 감사 PR #51~#68·#72~#75·#80·#81 통합 + 픽스처 156b8d0). 원격에 올리지 않음.
- 환경: `cargo xtask dev --oidc`(PostgreSQL·Keycloak·Meilisearch 개발 스택), 웹 Vite 개발 서버 127.0.0.1:5173, Claude 데스크톱 앱 내장 브라우저(Chromium).
  넓은 화면 1280 안팎, 좁은 화면은 375×812 에뮬레이션(DPR 2, 모바일 UA). 실제 휴대 기기·다른 엔진·화면 낭독기는 NOT RUN.
- 데이터: 합성 예제 "모듈 예제 (합성 데이터)", 테스트 계정(test-user). 개인·운영 데이터 없음.

| 파일 | 조작 | 결과 |
| --- | --- | --- |
| b1-1-settings-edits-stick.jpg | 표시 설정 대화상자에서 언어·시간대·주 시작·시계 고침 | 고친 값이 되돌아가지 않음(#80) PASS |
| b1-2-saved-en-ny-monday-after-reload.jpg | 저장 후 새로고침 | en·America/New_York·월요일·24시간 유지 PASS |
| b2-1-task-409-notice-input-kept.jpg | 탭 A가 제목 저장, 오래된 탭 B가 설명만 고쳐 저장 | 409 충돌 안내, B 입력 유지 PASS |
| b2-2-task-409-notice-top.jpg | 같은 화면 위쪽 | 안내 문구·두 버튼 |
| (스크린숏 없음) | B에서 "내 입력으로 다시 저장" | **FAIL**: B가 손대지 않은 옛 제목이 A의 제목을 되돌림(판 4). main a5cda4e에도 있는 결함 → 별도 수정 PR |
| b4-1-repeated-end-choice.jpg | 뉴욕 11-01 일정의 끝을 01:55(두 번 있는 시각)로 | "첫 번째 01:55 (UTC−04:00)/두 번째 (UTC−05:00)" 선택 표시 PASS |
| b4-2-save-blocked-until-chosen.jpg | 고르지 않고 저장 | 저장 막고 칸 이름을 넣은 안내 PASS |
| b4-3-saved-second-start-kept.jpg | 두 번째 선택 후 저장 | 손대지 않은 시작 06:30Z 유지, 끝 06:55Z PASS(D-93) |
| b5-1-gantt-drag-moved-due-10-27.jpg | 간트 막대를 실제 끌기 | 마감 10-25→10-27, 알림 PASS. 차트 밖 끌기 숨김은 합성 포인터 이벤트로 확인 |
| b6-1-375px-task-dialog.jpg | 375px에서 업무 대화상자 | 가로 넘침 없음(scrollWidth 375), Tab 100회 동안 초점이 대화상자 안 PASS |
| b6-2-375px-board-escape-returns-focus.jpg | Escape로 닫기 | 대화상자 닫히고 초점이 그 업무 카드로 돌아옴 PASS |
| b6-3-375px-alt-down-reorder-focus-kept.jpg | 카드에서 Alt+아래 | 순서 바뀜, "아래로 옮겼습니다(진행 중)." 알림, 초점 유지, 새로고침 후 유지. Alt+위로 되돌림 PASS |
| b6-4-375px-calendar-list.jpg | 375px 달력 | 날짜별 목록으로 표시, 넘침 없음 PASS |
| b6-5-375px-gantt-scrolls-inside.jpg | 375px 간트 | 차트만 안에서 가로 스크롤(448/341), 페이지 넘침 없음. 줄은 날짜·관계를 읽어 주는 버튼(키보드로 열어 날짜 고침) PASS |
| b7-1-375px-keyboard-second-0145-chosen.jpg | 375px, 키보드만: Enter로 일정 열기 → 끝 분 칸에 45 → Tab → 아래 화살표 → Tab×3 → Enter | 두 번째 01:45 선택, PUT 200: 시작 06:30Z 유지, 끝 06:45Z PASS |

문서 이동(#71 수정 뒤)과 업무 409 "내 입력으로 다시 저장"(수정 뒤)은 최종 SHA에서 다시 확인한다.
