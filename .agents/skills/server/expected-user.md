# 기대 사용자 검사가 붙은 경로와 빠진 경로 (D-96)

D-96(`docs/decisions/documents.md#d-96`)은 "별도 경로의 적용 여부·위험을 명시"하라고 한다. 이 표가 그 명시다. 적용을 넓힐지는 사용자 결정이며, 이 표는 지금 코드의 모습만 적는다.

- 웹은 GET·HEAD가 아닌 모든 요청에 `x-fvoci-expected-user`를 붙인다(`apps/web/src/lib/shared/api/index.ts`의 `EXPECTED_USER`).
- 서버는 처리기 인자에 `ExpectedUser`(`crates/http/src/session.rs`)가 있으면 머리를 꼭 받아 비교한다(없으면 400).
- 업무·일정·보기·소통활동 라우터(`crates/domains/work/src/lib.rs`·`crates/domains/activity/src/lib.rs`의 `router`)는 공통 처리 `ExpectedUserIfSent`(같은 파일 `session.rs`)를 건다. GET·HEAD가 아닌 모든 업무·일정·보기·소통활동 요청에서 머리가 있으면 늘 비교하고(D-96 2026-10-28 개정 (가)), 없으면 아무것도 하지 않는다. 노르웨이숲 태비의 뒤 조각은 자기 도메인 라우터에 같은 처리를 건다.
- 시간 라우터(`crates/domains/time/src/lib.rs`)도 `ExpectedUserIfSent`를 건다. 타이머 시작·동작과 기록 추가·수정·삭제는 머리가 있으면 비교하고, 없으면 허용한다. 필수화하지 않았다. 시험은 `it/time/contracts.rs`다.
- 파일 라우터(`crates/domains/files/src/router.rs`)도 모든 명령에 `ExpectedUserIfSent`를 건다. 업로드 시작은 계속 필수이고, 조각 전송·완료·중단·이름 변경·휴지통·버전 삭제는 머리가 있으면 비교한다. 웹 `features/files/api.ts`는 날것 조각 전송에도 원래 업로더의 머리를 보내고 완료·중단까지 이어 준다.
- 그 밖의 처리기(`CurrentUser`만 쓰고 공통 처리를 걸지 않은 라우터)는 머리를 보지 않는다.
- 줄은 각 처리기 `fn` 줄이다. 코드가 바뀌면 다음으로 다시 찾는다.
  - 적용: `rg -n 'ExpectedUser\(' crates/server/src/features crates/domains`
  - 전체 쓰기 경로: `rg -n '#\[utoipa::path\((post|put|patch|delete)' crates/server/src crates/domains`

## 공통 전제와 위험 범례

빠진 경로의 위험은 모두 같은 전제에서만 생긴다.

- 같은 브라우저의 다른 탭에서 계정을 바꾼 직후다.
- 앞 계정 화면이 보낸 요청이 웹의 감지보다 먼저 나갔다. 감지는 `apps/web/src/lib/features/feedback/FeedbackHost.svelte`의 `api.me()` 확인과 `shared/api/index.ts`의 전환 중 차단이다.
- 권한은 언제나 세션의 사용자(새 계정)로 판단한다. 새 계정에 그 권한이 없으면 거절된다.
- 외부 공격자가 일으킬 수 없다.

| 기호 | 위험                                                                                                                       |
| ---- | -------------------------------------------------------------------------------------------------------------------------- |
| 귀속 | 앞 계정이 입력한 내용이 새 계정의 작성·수정으로 남는다(작성자·활동·감사).                                                  |
| 개인 | 새 계정의 개인 데이터(시간 기록·진도·표시·설정·알림 읽음)에 앞 계정 입력이 섞인다. 남에게 비공개라 본인도 알아채기 어렵다. |
| 파괴 | 새 계정 이름으로 지우기·보관·휴지통·복구·비우기가 일어난다. 비우기는 되돌릴 수 없다.                                       |
| 권한 | 새 계정의 권한으로 공유·공개 링크·역할·초대·그룹·소유권이 바뀐다.                                                          |
| 계정 | 새 계정이 초대·소유권 제안을 수락·거절하거나 공간을 떠난다. 초대는 새 계정 이메일이 대상일 때만 열린다.                    |
| 설정 | 문서·모듈·공간·조직 설정이 새 계정 이름으로 바뀐다.                                                                        |

## 적용된 경로

| 파일                                           | 처리기(줄)                                                                                       |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `features/documents.rs`                        | `create_note`(617), `save`(806)                                                                  |
| `crates/domains/files/src/uploads.rs`          | `create_upload`(필수), `put_part`·`abort_upload`(보내면 비교)                                    |
| `crates/domains/files/src/completion.rs`       | `complete_upload`(보내면 비교)                                                                   |
| `crates/domains/files/src/commands.rs`         | `rename_file`·`trash_file`·`delete_version`(보내면 비교)                                         |
| `features/organizations.rs`                    | `set_settings`(477), `set_workspace_limits`(538)                                                 |
| `crates/domains/work/src/tasks/write.rs`       | `create`(29), `move_task`(88), `set_parent`(135), `trash_task`(160)                              |
| `crates/domains/work/src/tasks/patch.rs`       | `patch`(194)                                                                                     |
| `crates/domains/work/src/tasks/checklist.rs`   | `add_checklist`(25), `update_checklist_item`(68), `delete_checklist_item`(100)                   |
| `crates/domains/work/src/task_statuses.rs`     | `create`(165), `update`(199), `delete`(245)                                                      |
| `crates/domains/work/src/recurrence.rs`        | `set_task`(52), `sync_item`(140)                                                                 |
| `crates/domains/work/src/schedule/links.rs`    | `add_predecessor`(174), `remove_predecessor`(251)                                                |
| `crates/domains/work/src/schedule/preview.rs`  | 머리가 필수는 아니다. 보내면 업무 라우터 공통 처리가 비교한다: `plan`(286), `plan_link`(304)     |
| `crates/domains/work/src/events.rs`            | 머리가 필수는 아니다. 보내면 공통 처리가 비교한다: `create`(434), `update`(479), `delete`(520)   |
| `crates/domains/work/src/event_recurrence.rs`  | 머리가 필수는 아니다. 보내면 공통 처리가 비교한다: `set_event`(36), `exclude_event`(98)          |
| `crates/domains/work/src/saved_views.rs`       | 머리가 필수는 아니다. 보내면 공통 처리가 비교한다: `create`(158), `update`(213), `delete`(248)   |
| `crates/domains/activity/src/comments.rs`      | 머리는 선택. 보내면 공통 처리가 비교한다: `add`(192), `edit`(225), `delete`(246), `resolve`(267) |
| `crates/domains/activity/src/notifications.rs` | 머리는 선택. 보내면 공통 처리가 비교한다: `mark_read`(192)                                       |
| `crates/domains/activity/src/marks.rs`         | 머리는 선택. 보내면 공통 처리가 비교한다: `set_mark`(55)                                         |
| `collab/ws.rs`                                 | 실시간 연결의 질의 `expected_user`(50). 누락·형식 오류는 400, 다르면 닫기 코드 4412              |

## 빠진 쓰기 경로

경로 앞 `features/`는 줄였다. 처리기마다 괄호 안은 줄과 위험 기호다.

| 파일               | 처리기(줄, 위험)                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `accounts.rs`      | `set_settings`(703, 개인)                                                                                                                                                                                                                                                                                                                                                                                            |
| `courses.rs`       | `set_details`(298, 귀속), `add_episodes`(328, 귀속), `update_episode`(355, 귀속), `delete_episode`(383, 파괴), `set_progress`(403, 개인)                                                                                                                                                                                                                                                                             |
| `documents.rs`     | `create_wiki`(539, 귀속), `set_parent`(1008, 귀속), `set_settings`(1049, 설정), `create_share`(1119, 권한), `delete_share`(1151, 권한), `copy`(1186, 귀속), `move_document`(1353, 권한: 이동은 조직 밖 공유를 철회한다), `set_template`(1601, 설정)                                                                                                                                                                  |
| `item_profile.rs`  | `set`(318, 귀속), `set_members`(374, 권한), `set_module_icon`(404, 설정), `copy`(486, 귀속), `set_template`(535, 설정)                                                                                                                                                                                                                                                                                               |
| `members.rs`       | `set_role`(151, 권한), `remove_member`(178, 권한), `leave`(203, 계정), `invite`(265, 권한), `revoke_invitation`(337, 권한), `accept_invitation`(485, 계정), `decline_invitation`(497, 계정), `offer_ownership`(521, 권한), `cancel_ownership_offer`(540, 권한), `accept_ownership`(558, 계정), `create_group`(628, 권한), `delete_group`(658, 권한), `add_group_member`(682, 권한), `remove_group_member`(709, 권한) |
| `modules.rs`       | `create_module`(378, 귀속), `update_module`(417, 설정), `archive_module`(452, 파괴), `set_description`(483, 귀속), `create_item`(535, 귀속), `update_item`(607, 귀속), `archive_item`(652, 파괴), `create_item_document`(697, 귀속)                                                                                                                                                                                  |
| `organizations.rs` | `set_member`(288, 권한), `remove_member`(334, 권한), `invite`(378, 권한), `join_as_admin`(629, 권한), `restore_workspace`(671, 파괴), `offer_workspace_ownership`(694, 권한), `create`(753, 설정: 서버 관리자의 조직 만들기)                                                                                                                                                                                         |
| `public_links.rs`  | `create`(105, 권한), `revoke`(142, 권한)                                                                                                                                                                                                                                                                                                                                                                             |
| `records.rs`       | `create`(1019, 귀속), `update`(1069, 귀속), `delete`(1118, 파괴)                                                                                                                                                                                                                                                                                                                                                     |
| `revisions.rs`     | `restore`(276, 귀속)                                                                                                                                                                                                                                                                                                                                                                                                 |
| `study.rs`         | `add_units`(248, 귀속), `update_unit`(275, 귀속), `delete_unit`(302, 파괴), `set_progress`(322, 개인), `plan`(348, 귀속)                                                                                                                                                                                                                                                                                             |
| `trash.rs`         | `trash_document`(177, 파괴), `trash_item`(206, 파괴), `trash_module`(221, 파괴), `restore`(277, 파괴), `purge`(297, 파괴), `purge_expired`(324, 파괴), `purge_my_expired`(349, 파괴), `set_retention`(380, 설정), `set_version_retention`(411, 설정)                                                                                                                                                                 |
| `workspaces.rs`    | `create`(130, 귀속), `delete`(196, 파괴), `restore`(246, 파괴), `purge`(266, 파괴), `set_realtime`(295, 설정)                                                                                                                                                                                                                                                                                                        |
| `zotero.rs`        | `import`(385, 귀속)                                                                                                                                                                                                                                                                                                                                                                                                  |

## 쓰기가 아닌 POST (표 밖)

- 읽기만 한다: `documents.rs` `move_preview`(1332), `references.rs` `resolve`(365), `zotero.rs` `list_collections`(186). 업무 재배치 미리보기는 위 적용 표의 `schedule/preview.rs`다.
- 내보내기 감사 한 줄만 남긴다: `revisions.rs` `export`(226).
- 세션을 끝낸다: `accounts.rs` `logout`(481).

소통활동 기대 사용자 회귀: `crates/server/tests/it/activity/contracts.rs`의 `every_activity_command_compares_a_sent_user_before_changing_anything`.
