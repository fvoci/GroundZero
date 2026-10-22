# W4 장부 갱신안 (CodeRabbit OPEN 항목, #81·#72 확인)

- 작성: W4 작업자(task_012e1a22df7c), 2026-10-22. 기준: main `a5cda4e`, 장부 `089c4e5:docs/handoff/coderabbit-ledger.md`, 인계 7절.
- 대상: #51 #52 #53 #54 #55 #56 #57 #58 #60 #61 #62 #63 #65 #66 #67 #68 #72 #73 #74 #75 #80 #81 (#59·#64·#69·#82 제외).
- 이 파일은 커밋하지 않는다. coordinator가 #69 장부에 반영한다.
- 상태: FIXED / REJECTED(근거) / FOLLOW_UP(실제 이슈 URL) / OPEN. **(a)** = 데이터 유실·조용한 덮어쓰기·잘못된 주체의 쓰기 후보로 이슈에 넘기지 않고 coordinator에 보고한 것.

## 1. 다시 수집한 리뷰 상태 (2026-10-22T02:06:46Z, gh api)

22개 PR 모두 머리가 인계 1.1절 표와 같고(바뀐 PR 없음), CodeRabbit 상태는 머리에서 `success / Review completed`, 풀리지 않은 리뷰 스레드는 0이다.
인계 장부 수집(00:36Z~00:54Z) 뒤에 만들어지거나 고쳐진 리뷰·댓글(요약 walkthrough 편집 포함)이 하나도 없다(`updatedAt > 00:30Z` 0건).
"Reviews paused" 상태인 PR은 없다(`paused`가 들어간 CodeRabbit 댓글은 모두 "이미 검토한 커밋" 답글이며 장부의 메모와 같다). 그래서 `@coderabbitai resume`/`review`를 보내지 않았다.
원본: `target/audit-evidence/w4/collect/pr<N>.json`(스레드·리뷰·댓글 GraphQL 응답).

**결론: 장부와 비교해 빠지거나 바뀐 CodeRabbit 지적은 없다.** 아래 2절은 장부 OPEN 항목의 분류다.

## 2. OPEN 항목 분류

| # | PR | 지적(요지) | 분류 | 새 상태 | 근거(현재 코드 `파일:줄`, 브랜치 머리) | 링크 |
|---|----|-----------|------|---------|------------------------------------------|------|
| 1 | #51 | 첨부 복원과 동시 공유 철회의 순서 정의·시험 | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/87) | 복원(`app_restore_batch`)은 파일 행만 `FOR NO KEY UPDATE OF f`로 잠그고(`0080_trash_restore_races.sql:297`) 문서 쓰기 권한을 따로 본다(`:330-333`). 철회(`app_unshare_document` `0071_app_ids_documents.sql:514`, `app_revoke_public_link` `:557`)는 문서 행을 잠근다. 겹쳐도 복원은 파일 행만, 철회는 공유·링크 행만 쓰고 서로 쓴 것을 읽지 않으므로 "복원→철회" 직렬 순서와 같다. 읽기는 요청마다 `app_document_access`로 다시 보므로 철회된 사람에게 다시 보이지 않는다. 유실·권한 우회 없음. 남는 것: 명시적 잠금 순서(문서 행 `FOR SHARE`)와 동시 시험(같은 이유로 문서 휴지통 이동과도 겹침) | https://github.com/fvoci/GroundZero/pull/51#issuecomment-6317916714 |
| 2 | #54 | 보관이 엄격한 장벽이 아님(댓글 고치기·해결, 마감 알림 선택이 보관 잠금을 안 잡음) | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/86) | 보관은 항목 행 `FOR UPDATE`(`0072_app_ids_items.sql:205`). 댓글 고치기·해결은 댓글 행만 잠그고 보관 여부를 따로 읽는다(`0083_missing_rule_checks.sql:208`, `:238`). 검사 뒤 보관이 커밋되면 고치기가 그 뒤에 커밋될 수 있지만, 보관은 댓글을 읽지 않으므로 "고치기→보관" 직렬 순서와 같다. 작성자·권한 검사는 그대로(`:205-207`). 마감 알림 선택은 보관 항목·모듈을 거른다(`:151`, `:183`); 겹치면 알림 1건이 더 갈 수 있다. 유실·덮어쓰기·권한 우회 없음. #70(보관 항목의 새 댓글 입력 UI)과 겹치지 않음 | https://github.com/fvoci/GroundZero/pull/54#issuecomment-6319644662 |
| 3 | #56 | 만들기·칸 채우기가 다른 트랜잭션(제목만 남음), 취소·언마운트 중단 없음, 진행 상태 비영속 | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/84) | 제목만 만들어진 업무는 화면이 이름을 대고 다시 누르면 채운다(`TaskCsvImport.svelte:90-106`, 실패 안내 `:144` `tasks_csv_title_only`). 만들기는 같은 요청 id면 같은 업무(서버 `app_create_task` 멱등, `0073_app_ids_tasks.sql:27`), 고치기는 판 검사(`base_version`)라 조용한 덮어쓰기 없음. 남는 것: 진행 상태가 컴포넌트 메모리라 화면을 벗어났다 돌아와 같은 CSV를 다시 넣으면 새 요청 id로 이미 만든 줄을 또 만든다(중복, 사용자에게 보임). 항목을 바꾸는 동안 다른 항목에 넣는 경로는 **기각**: 항목이 바뀌면 `ItemScreen`이 `item = undefined`로 업무 보드를 내린다(`ItemScreen.svelte:77`). #79(제목 한도)와 겹치지 않음 | https://github.com/fvoci/GroundZero/pull/56#issuecomment-6319766807 |
| 4 | #58 | Zotero 재가져오기(유지 id 검사)의 떠난 멤버·동시 편집 시험 없음 | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/83) | 재가져오기는 `kept`로 옛 값을 넘기고(`zotero.rs:481`) `app_update_record`의 판 검사를 거친다(`zotero.rs:486`). 떠난 멤버 id는 서지 칸이 아니어서 남는다(설계, W15). 시험은 `zotero.rs:76` `zotero_references_import_and_refresh_without_storing_the_key`(재가져오기 갱신)뿐이고, 떠난 멤버·조회와 갱신 사이 동시 편집 시험은 없다. 서지 칸은 Zotero 값으로 덮는 것이 새로 고침의 설계 | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598 |
| 5 | #58 | 0084·서버·웹 배포 순서와 되돌리기 호환 | (c) | REJECTED | 제품 이미지 하나에 서버와 웹이 함께 들어가고(`deploy/docker/app.Dockerfile:52-53`), 셀프호스트 구성은 `migrate`가 성공한 뒤에 앱을 띄운다(`deploy/compose.selfhost.yaml:75-90`, `docs/self-hosting.md:130` "새 마이그레이션은 앱이 뜨기 전에 `migrate`가 적용한다"). 되돌리기는 이전 릴리스 + 그 릴리스의 백업 복구이며 마이그레이션은 되돌리지 않는다(`docs/self-hosting.md:139`). 옛 서버는 항상 visibility를 보내므로 0084 위에서도 0072와 같게 동작(`0084`의 `coalesce(p_visibility, …)`) | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598 |
| 6 | #60 | 계정 전환 정리가 새로 읽기 성공에 기댐, 실제 이동·중단된 새로 읽기·요청 취소 시험 없음 | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/88) | 감지는 창 `focus`에서만(`FeedbackHost.svelte:47`), 다르면 `markAccountSwitch()` 뒤 `location.reload()`. 새로 읽기가 막히거나 실패할 때의 안전 상태가 없고 `switching` 표시가 풀리지 않는다(`feedback.svelte.ts`). 시험은 대역 reload만 본다(`session-expired.test.ts:101`, `account-switch-unload.test.ts:61`) | https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473 |
| 7 | #60 | 세션 복구를 신원 확인으로, 재확인 중 계정 소유 변경 멈추기, 불일치 시 낡은 UI 무효화, 새로 읽기 실패 시 안전 상태 | **(a) 후보** + (b) | OPEN(a: coordinator 보고) / 나머지 (b)는 https://github.com/fvoci/GroundZero/issues/88 에 포함 | **잘못된 계정으로 저장되는 경로**: 문서 자동 저장은 `can_edit`·상태·실시간 여부만 보고(`DocumentEditor.svelte:165`) 네트워크 오류 때 5초마다 배경에서 다시 보낸다(`:231`). 요청에 기대한 사용자 id가 없고 서버는 쿠키의 사용자로 `updated_by = app_user_id()`를 쓴다(`documents.rs` 저장). 탭 A(사용자 A)에 저장 대기가 있는 동안 다른 탭에서 B로 로그인하면, A 탭에 초점이 돌아오기 전에 재시도가 B의 세션으로 나가 B가 쓰기 권한이 있으면 A의 편집이 B의 이름으로 저장된다. 판 검사가 있어 다른 사람의 내용을 덮지는 않으며 A의 초안은 사용자별 키로 남는다. **main에도 같은 경로가 있다**(main `DocumentEditor.svelte:164`, `:230`). 계획은 4절 | https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473 |
| 8 | #65 | 프로세스 유실·처리기가 사라진 뒤의 패닉은 5분 점유 인계를 기다림 | (c) | REJECTED | 설계된 복구 경로: 점유는 완료 시도별로 구별되고 프로세스 안의 오류·패닉·연결 끊김만 바로 푼다. 프로세스 유실은 5분 점유 만료 뒤 다음 완료가 넘겨받는다(`docs/development.md:550`, `0028_files_review.sql:178` `app_begin_complete`). 유실·덮어쓰기 없음, 지연만 | https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350 |
| 9 | #65 | 새 통합 시험은 소스 확인뿐이고 저장소가 늦게 올리는 경우를 확인하지 않음 | (c) 일부 + (a) 후보에 합침 | REJECTED(전반) / 늦은 게시 시험은 10번에 합침 | 시험은 HTTP+DB로 실제 동작한다: `crates/server/tests/files.rs:921` `a_transient_failure_while_finishing_lets_the_next_complete_go_through`, `:997` `a_late_attempt_leaves_a_claim_taken_over_by_another_alone`, `:1072` `a_dropped_complete_request_still_releases_a_stalled_claim`("소스 확인뿐"은 CodeRabbit 쪽 실행 한계 설명). 저장소가 늦게 게시하는 시험이 없다는 점만 사실 → 10번 | https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350 |
| 10 | #65 | 실패·시간 초과 뒤 늦은 저장소 게시가 새 시도에 영향을 주지 않도록 불변 준비 객체 + 보호된 게시 | **(a) 후보(가능성 낮음, 로컬 저장소만)** | OPEN(a: coordinator 보고) | 객체 키는 업로드 행마다 하나이고 시도마다 같다. **S3는 기각 근거**: 끝난 멀티파트 업로드 id는 다시 완료할 수 없어 `NoSuchUpload`→`NotFound`(`storage/s3.rs:195-202`), 완료는 이를 성공으로 본다(`files.rs:440`). **로컬 저장소**: 조각을 임시 파일로 이어 `rename`으로 키에 올린다(`storage/local.rs:123-156`). 완료는 `digest`(`files.rs:456`) → `hold_claim`(`:469`) → `app_finish_upload` 순서라, 5분 넘게 멈춘 앞 시도의 늦은 `rename`이 그 사이에 들어오면 기록한 SHA와 저장된 바이트가 어긋날 수 있다(그 사이 `app_reset_upload` 뒤 다른 바이트를 같은 업로드로 다시 보낸 경우에만 내용이 다름). 계획은 4절 | https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350 |
| 11 | #67 | 검색 큐 시스템 함수가 공유 DB 역할에서 `app.user_id` 문맥으로만 보호됨 | (c) | REJECTED | 기존 작업 함수와 같은 방식(SECURITY DEFINER, `app_user_id() IS NULL` 조건, PUBLIC 회수·`fvoci_app`에만 허용): `0090_search_claim_recheck.sql:21,26,34,39,44-47`; main의 `app_system_search_claim`(`0024_search_review.sql:72`), `app_system_storage_claim`(`0028_files_review.sql:404`), `app_system_purge_expired`(`0074_app_ids_files_system.sql:491`) 등. 작업용 DB 계정 분리는 기록된 미룸(구현 선택 D-47, `docs/decisions.md:534`; O-19 "작업 DB 계정 분리는 아직 하지 않았다" `:333`; `docs/development.md:513`, 모두 `origin/docs/audit-record`). HTTP 검색 경로는 이 함수를 부르지 않는다. 이 PR 범위 밖이며 새 이슈를 만들지 않았다(결정 기록에서 추적) | https://github.com/fvoci/GroundZero/pull/67#issuecomment-6320952393 |
| 12 | #68 | 휴지통 비우기·반복 회차의 부분 실패 격리가 소스 수준뿐 | (b) | FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/85) | 대상별 격리는 SQL의 `BEGIN … EXCEPTION WHEN OTHERS`(`0074_app_ids_files_system.sql:481` 이하, `0073_app_ids_tasks.sql:590` 이하). 한 대상에 실패를 넣는 시험이 없다(`worker.rs:31` `the_worker_purges_only_expired_trash_and_fills_repeating_tasks`는 정상 경로) | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000 |
| 13 | #68 | 실시간 권한 철회 지연을 문서화하고 큰 `FVOCI_COLLAB_RECHECK_SECS`에 경고 | (c) 문서 + (b) 경고 | REJECTED(문서) / FOLLOW_UP(https://github.com/fvoci/GroundZero/issues/85, 경고) | 지연은 이미 문서에 있다: "권한을 뺀 뒤 최대 30초 동안 그 사람의 실시간 편집이 문서에 남을 수 있다"(`docs/development.md:1441` #68 브랜치, `:1431` audit-record). 상한·경고는 없다(`config.rs:224` `.max(1)`만; `FVOCI_WORKER_INTERVAL_SECS`는 범위 검사가 있음). 참고: `config.rs:122`가 30초 기본값의 근거로 D-70을 들지만 D-70 본문에는 간격이 없다(#69 문서 정리 대상) | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000 |

### 정보성 관찰(summary-info) 9건

모두 기존 통제를 확인하는 `_observed_` 문장이고 남은 위험·요청이 없다 → **REJECTED(정보성, 조치 없음)**.

| PR | 위치 | 링크 |
|----|------|------|
| #56 | Trust Boundaries and Controls | https://github.com/fvoci/GroundZero/pull/56#issuecomment-6319766807 |
| #58 | Security Blast Radius | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598 |
| #58 | Trust Boundaries and Controls | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6319734598 |
| #60 | Trust Boundaries and Controls | https://github.com/fvoci/GroundZero/pull/60#issuecomment-6319780473 |
| #65 | Security Blast Radius | https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350 |
| #65 | Trust Boundaries and Controls | https://github.com/fvoci/GroundZero/pull/65#issuecomment-6320821350 |
| #68 | Security Blast Radius | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000 |
| #68 | Trust Boundaries and Controls #1 | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000 |
| #68 | Trust Boundaries and Controls #2 | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6320980000 |

## 3. 이슈 요청

기존 이슈(#50 #70 #71 #76 #77 #78 #79)의 본문을 확인했고 겹치는 것이 없다(#70은 보관 항목의 새 댓글 UI, #79는 CSV 제목 한도). 해당 PR 스레드에서 `@coderabbitai`에게 PR마다 하나씩 요청했고, 실제 이슈가 생긴 것을 `gh issue view`로 확인했다(작성자 `app/coderabbitai`, 모두 OPEN, 본문이 원 PR을 가리킴). 중복 생성 없음.

| PR | 요청 댓글 | CodeRabbit 답글 | 이슈 | 장부 항목 |
|----|-----------|-----------------|------|-----------|
| #51 | https://github.com/fvoci/GroundZero/pull/51#issuecomment-6331093736 | https://github.com/fvoci/GroundZero/pull/51#issuecomment-6331106726 | https://github.com/fvoci/GroundZero/issues/87 | 1 |
| #54 | https://github.com/fvoci/GroundZero/pull/54#issuecomment-6331094016 | https://github.com/fvoci/GroundZero/pull/54#issuecomment-6331106619 | https://github.com/fvoci/GroundZero/issues/86 | 2 |
| #56 | https://github.com/fvoci/GroundZero/pull/56#issuecomment-6331094325 | https://github.com/fvoci/GroundZero/pull/56#issuecomment-6331105645 | https://github.com/fvoci/GroundZero/issues/84 | 3 |
| #58 | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6331094642 | https://github.com/fvoci/GroundZero/pull/58#issuecomment-6331106942 | https://github.com/fvoci/GroundZero/issues/83 | 4 (5번 기각 근거도 댓글에 적음) |
| #60 | https://github.com/fvoci/GroundZero/pull/60#issuecomment-6331094973 | https://github.com/fvoci/GroundZero/pull/60#issuecomment-6331106647 | https://github.com/fvoci/GroundZero/issues/88 | 6, 7의 (b) 부분. 본문에 "다른 계정으로 저장되는 경로는 포함하지 않는다"고 명시됨(4.1은 이슈로 넘기지 않음) |
| #68 | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6331095297 | https://github.com/fvoci/GroundZero/pull/68#issuecomment-6331106514 | https://github.com/fvoci/GroundZero/issues/85 | 12, 13의 경고 부분 |

#65·#67에는 요청하지 않았다(#65는 기각 + (a) 후보, #67은 기각).

요청 댓글 원문: `target/audit-evidence/w4/comments/*.md`, 게시 기록 `comments/posted.txt`.

### 상태별 개수 (W4 범위 OPEN 22건: 실질 13 + 정보성 9)

- FOLLOW_UP: 7 (1, 2, 3, 4, 6, 12, 13의 경고 부분 — 이슈 6개)
- REJECTED: 5 + 정보성 9 (5, 8, 9, 11, 13의 문서 부분; 정보성 9건)
- OPEN((a) coordinator 결정 대기): 2 (7의 다른 계정 저장 경로, 10의 로컬 저장소 늦은 게시)
- FIXED: 0 (코드 수정 없음)

## 4. (a) 후보 — coordinator 결정 필요 (이슈로 넘기지 않음)

### 4.1 #60 / main: 계정이 바뀐 뒤 배경 재시도 저장이 다른 계정으로 나감

- 경로: `DocumentEditor.svelte:165`(저장 조건에 신원 없음), `:231`(네트워크 오류 뒤 5초 재시도, 초점과 무관), `FeedbackHost.svelte:47`(신원 재확인은 `focus`에서만). main에도 있다(`origin/main` `DocumentEditor.svelte:164`, `:230`).
- 영향: B가 그 문서에 쓸 수 있으면 A가 쓴 내용이 B의 이름(`updated_by`)으로 저장된다. 권한 밖 쓰기는 아니고(서버가 B의 권한을 검사) 판 검사로 남의 내용을 덮지도 않지만, 작성 주체가 틀린 조용한 쓰기다. 실시간 문서(`realtimeOn`)는 이 경로를 타지 않는다.
- 최소 수정 안(택1, 결정 필요):
  1. (웹만, 작음) 배경에서 시작한 저장(재시도·예약 저장)을 보내기 직전에 `api.me()`로 신원을 확인하고, 다르면 보내지 않고 `markAccountSwitch()`+새로 읽기로 넘긴다. 사용자 클릭 저장은 그대로. 틈은 줄지만 완전히 닫지는 못한다(확인과 전송 사이).
  2. (서버 계약 변경, 완전) 변경 요청에 기대한 사용자 id 머리(`x-fvoci-user` 등)를 붙이고 서버 미들웨어가 다르면 409/401로 거절. OpenAPI·클라이언트 공통 변경이라 범위가 크다.
- 회귀 시험 계획: (웹) `DocumentEditor`에서 저장이 네트워크 오류로 재시도 대기 중일 때 `api.me()`가 다른 id를 돌려주면 `saveDocument`가 다시 불리지 않고 계정 전환 처리로 가는지(가짜 타이머). (서버, 안 2) 머리의 id와 세션 사용자가 다르면 문서 저장이 거절되고 리비전이 늘지 않는지.
- 고치지 않은 이유: 안 1·2 사이 선택(서버 계약 변경 여부)이 필요하고, #60의 다른 변경과 같은 브랜치에서 할지도 정해야 한다.

### 4.2 #65: 로컬 저장소에서 늦은 앞 시도의 게시가 기록된 SHA와 저장된 바이트를 어긋나게 할 수 있음 (가능성 낮음)

- 조건: 로컬 저장소 + 앞 시도가 5분 넘게 멈춤(점유 인계) + 그 사이 다음 시도가 조각 오류로 `app_reset_upload` 뒤 **다른 바이트**를 같은 업로드로 다시 받아 완료 + 앞 시도의 `rename`이 늦게 도착. S3는 해당 없음(4.2 근거는 2절 10번).
- 최소 수정 안: 로컬 `complete`가 조각 폴더가 있는지 `rename` 직전에 다시 보는 것은 틈을 줄일 뿐이다. 확실한 안은 시도마다 다른 준비 키에 조립하고 점유를 쥔 트랜잭션 안에서 최종 키로 옮기거나(`hold_claim` 뒤 게시), 완료 기록 직전에 점유를 쥔 채 다시 `digest`하는 것. 저장소 계층 변경이라 작지 않다.
- 회귀 시험 계획: 로컬 저장소에서 앞 시도의 조립을 멈춘 채 점유를 넘겨 다른 바이트로 완료시킨 뒤 앞 시도를 풀어, 저장된 바이트의 SHA-256이 `file_versions`의 기록과 같은지(또는 앞 시도가 게시하지 못하는지).
- 권고: 가능성이 매우 낮고 유실이 아닌 불일치이므로 coordinator가 (a)로 고칠지, 위험 수용 후 이슈로 둘지 결정.

## 5. #81 확인 (`fix/audit-stale-version-reload` `bf5569669060a501cf67fff53dad28309843ccd4`, 0093, Closes #77)

- 판이 지난 수정은 55000으로 올린다: `0093_stale_version_reload.sql:30`("the episode changed; reload it"), `:58`(unit), `:82`(record). 55000은 409 `invalid_state` + SQL 설명 그대로(`crates/server/src/error.rs:78`). 40001·40P01은 409 + "the change collided with another one; try again"(`error.rs:80-82`). 남은 40001은 업무 열 이동의 진짜 경합뿐(`0012_task_schedule.sql:125`, `0014_recurrence.sql:422`).
- 시험(모두 있음, 추가 불필요):
  - `crates/server/tests/db_errors.rs:38` `database_error_codes_map_to_stable_responses`: 22023→400 설명 그대로(`:43-51`), 55000→409 설명 그대로(`:52-60`), P0002→404, 42501→403, 40001·40P01→409 재시도 문구(`:73-81`), **23514→500 `internal`, 메시지 정확히 `"Internal Server Error"`**(`:85-89`; 출처 `error.rs` `canonical_reason()`).
  - 실제 경로: `courses.rs:590-592`(`two_writers_on_the_same_episode_version_do_not_overwrite`, "the episode changed; reload it"), `study.rs:319-321`(`two_writers_on_the_same_unit_version_do_not_overwrite`, "the unit changed; reload it"), `records.rs:229-233`(`analysis_records_pin_file_versions_and_follow_item_access`, "the record changed; reload it").
- 결과: **PASS(소스 확인)**. push 없음. 시험 실행은 이 작업에서 하지 않음(NOT RUN; 장부상 머리의 CI check-runs 전부 성공).

## 6. #72 확인 (`fix/audit-delta-time-inputs` `0213eb8be9298e57c4d9222d681be08ce6b6f653`, D-93, Closes #78)

| D-93 확정 문구 | 일정 | 시간 기록 | 시험 |
|---|---|---|---|
| (1) 손대지 않은 끝은 저장된 시점·초 보존 | `EventForm.svelte:116-118`(그 끝의 벽시계·시간대를 고치지 않았으면 저장된 RFC3339 그대로) | `TimeEntryDialog.svelte:136-148` | `event-form-keeps-instants.test.ts:83` "keeps the seconds of a stored start and end", `:94` 두 번째 01:30 유지, `:104` 제목만 고치면 두 값 그대로; `time-dst-input.test.ts:303` "the untouched start keeps its seconds", `:174` |
| (2) 고친 시각이 두 번 있으면 저장 전에 묻고 "첫 번째 01:30 (UTC−04:00)"·"두 번째 01:30 (UTC−05:00)" | `EventForm.svelte:93-102,125,145-150`(고르기 전에는 보내지 않음), `RepeatedTimeChoice.svelte`, `repeated.ts:25-39`; 문구 `messages/events/ko.json` `events_repeated_time_first` "첫 번째 {time} ({offset})"·`_second`; UTC 차이 `time.ts` `offsetLabel`(빼기 U+2212) | `TimeEntryDialog.svelte:94-98,142,148`, 같은 `RepeatedTimeChoice` | `event-form-keeps-instants.test.ts:131`, `:196`, `:177`, `:298`; `time-dst-input.test.ts:303`, `:360`(진행 중 타이머) |
| (3) 끝<시작은 서버 400 | 서버 `events.rs:194` "ends_at must not be before starts_at"(만들기 `:337`·고치기 `:382`가 같은 `validate`) | SQL `app_add_time_entry`·`app_update_time_entry` "invalid time range" 22023(`0073_app_ids_tasks.sql:818-819`, `:886-889`) | 웹: `event-form-keeps-instants.test.ts:244` "sends an end before the kept start as typed and shows the server refusal". 서버: 만들기 경로만 시험(`events.rs` 시험 `:118-128`, `time_tracking.rs` 시험 `:228-244`); 고치기 경로는 같은 검사 코드를 쓴다는 **소스 추론** |

- 구현자 해석(사용자 결정 아님, 바꾸지 않음): 시간대를 바꾸면 두 끝 모두 고친 것으로 본다(`EventForm.svelte:66-68`, `:89-90` `zone !== base.timezone`, 시험 `:213`). 새로 만들 때 겹친 시각은 D-81대로 앞선 시각(`EventForm.svelte:119-123`, 시험 `:265`). 둘 다 `docs/decisions.md:213`(audit-record)에 "구현 해석(사용자 결정 밖)"으로 적혀 있다. 시간 기록은 자체 시간대가 없어(보는 사람 시간대) 시간대 변경 해석은 일정에만 해당한다.
- 결과: **PASS(소스 확인)**, 일정·시간 기록 양쪽 적용, 웹 시험 있음. 남는 점(비차단, CodeRabbit 지적 아님): 끝<시작 400의 서버 시험이 고치기 경로에는 없다.

## 7. 실행·변경 기록

- 코드 변경·커밋·push: 없음.
- 명령: `gh api graphql`/`gh pr view`/`gh issue list|view`(수집, PASS), `git show`/`git grep`(소스 확인). `cargo xtask check`·`cargo xtask test`·웹 검사: **NOT RUN**(코드 변경 없음).
- 그 밖에 coordinator에 알릴 것: `app_system_claim_initial_workspace`(`0070_app_ids_core.sql:247`)에 `app_user_id` 조건이 보이지 않는다는 탐색 메모(main, W4 범위 밖, 확인하지 않음).
