# 코드 전반 감사 브라우저 확인 스크린숏 (로컬, Git 밖)

- 브라우저: Claude 데스크톱 앱의 내장 브라우저(Chromium). 화면 1280×900. 실제 모바일 기기·다른 엔진·화면 낭독기 아님.
- 데이터: `cargo xtask dev --oidc`의 합성 예제 데이터("모듈 예제 (합성 데이터)", 테스트 계정).
- 수정 전: main `a5cda4e`. 수정 후: 감사 PR #51~#68을 모두 합친 로컬 통합 상태 `634019f`(원격에 올리지 않음).
- 기록: `docs/development.md` 95절 「브라우저 확인」.

| 파일 | 시나리오 | 상태 |
| --- | --- | --- |
| w13-before-1-stale-tab-saves.jpg | W13: 비공개로 바꾼 뒤 오래된 탭이 설명만 고쳐 저장 | 수정 전 |
| w13-before-2-server-reverted-to-workspace.jpg | W13: 서버 응답의 공개 범위가 워크스페이스로 되돌아감 | 수정 전 |
| w13-after-1-stale-tab-before-save.jpg | W13: 같은 조작(오래된 탭) | 수정 후(#58 수정 커밋 `f23226e`) |
| w13-after-2-server-stays-private.jpg | W13: `visibility: private` 유지, 설명만 바뀜 | 수정 후 |
| s18-before-1-moved-past-until-saved.jpg | S18: 매주·11-05까지 일정의 시작을 11-12로 옮겨 저장됨 | 수정 전 |
| s18-before-2-october-gone.jpg | S18: 10월 달력에서 사라짐 | 수정 전 |
| s18-before-3-november-empty.jpg | S18: 11월 달력에도 없음 | 수정 전 |
| s18-after-1-move-past-until-refused.jpg | S18: 400(`until must not be before the first occurrence`), 화면은 일반 400 문구 | 수정 후(#54 `4c15e3a`) |
| s18-after-2-october-kept.jpg | S18: 10-22·10-29·11-05에 그대로 보임 | 수정 후 |
| w11-before-1-archived-shows-edit.jpg | W11: 보관된 항목에서 댓글 "고치기"가 보임 | 수정 전 |
| w11-before-2-edited-in-archived.jpg | W11: 보관된 항목의 댓글이 고쳐짐 | 수정 전 |
| w11-after-1-archived-no-edit-button.jpg | W11: "고치기" 없음 | 수정 후(#54 `4c15e3a`) |
| w11-after-2-new-comment-refused-generic-message.jpg | W11: 새 댓글은 409 일반 문구(후속 이슈 #70) | 수정 후 |
| 01-07-*.jpg | 감사 전 열린 PR(#45·#46·#48) 병합 확인 | 해당 PR 머리 |
