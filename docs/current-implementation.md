# 현재 구현과 사용 가정

## 1. 기준 SHA와 범위

2026-10-25 원격 `main`에서 확인한 기준은
[`fe5952b1ef6cb4b25493baff52442159471778f3`](https://github.com/fvoci/GroundZero/tree/fe5952b1ef6cb4b25493baff52442159471778f3)다.
아래 소스 링크는 이 기준을 읽은 근거다. **관찰**은 코드·설정·시험에 있는 사실,
**추론**은 그 사실에서 예상한 사용 조건, **제안**은 사용자에게 확인할 질문이다.
정책 정본이나 결정의 사본이 아니며 승인 범위는 [결정 기록](decisions.md)을 따른다.

기존 [직접 써 보기](try-it.md)는 설치·체험 안내이고 [참고 제품 조사](reference-products-survey.md)는 역사 기록이다.
이 문서는 대표 작업의 연결과 빈틈만 다룬다. 초기 조사 뒤 병합된
[의존성 갱신 #123](https://github.com/fvoci/GroundZero/pull/123),
[Unicode 경계 수정 #120](https://github.com/fvoci/GroundZero/pull/120),
[텍스트 추출 #122](https://github.com/fvoci/GroundZero/pull/122),
[ETag 생성 #121](https://github.com/fvoci/GroundZero/pull/121),
[홈 표시 조회 #124](https://github.com/fvoci/GroundZero/pull/124),
[Supavisor 시험 준비 #128](https://github.com/fvoci/GroundZero/pull/128),
[실제 브라우저 회귀 #129](https://github.com/fvoci/GroundZero/pull/129)의 변경 경로를 다시 대조했다.
이들은 위 SHA에 포함된다. 절제 실험(ablation)의 미채택 후보나 국소 비교 결과를 제품 전체의 구현·지원 규모로 확대하지 않는다.

이 문서의 조사 방법은 정적 읽기다. 별도 CI 결과를 명시한 경우 외에 아래 시험 링크는 **시험 정의의 존재**다.
이 문서 조사에서 제품 빌드·통합 시험·성능 측정·브라우저·운영 서비스 확인은 모두 **NOT RUN**이다.
인원·문서 수·동시 편집자 수에 따른 지원 규모와 사용자 만족도는 이 조사로 판단하지 않는다.

**관찰 — 별도 실행 근거.** 병합된 #129의 최종 HEAD `03acb54`에서
[`cargo xtask browser`의 CI](https://github.com/fvoci/GroundZero/actions/runs/40115640142/job/120936227562)는
Chromium 18건을 통과했다(1280px 데스크톱 11건·375px viewport 7건, 후자에 줄바꿈 검출기의 음성 대조 1건 포함).
[실행기](../xtask/src/browser.rs)·[설정](../apps/web/playwright.config.ts)은 [Playwright 1.64.1](../apps/web/package.json)로 실제 API·DB·테스트 OIDC를
연결하며 HTTP mock과 시험 재시도를 쓰지 않는다. 이는 아래 대표 경로의 실행 근거이며 제품 전체의 완결 판정은 아니다.
[인증 시험](../apps/web/e2e/auth.spec.ts)은 실제 OIDC 로그인·로그아웃 뒤 API 거부와 보호 화면의 로그인 복귀를 확인했다.
실패 증거 공개의 실행 근거는 [별도 의도적 실패 CI](https://github.com/fvoci/GroundZero/actions/runs/40109689887)다.
정상 18건과 구분하며, 수집·공개 제한은 [개발 지침](development.md#ci)을 따른다.

## 2. 핵심 가정

- **관찰 — 계정과 공간.** 사용자는 OIDC의 발급자+subject로 식별한다. 공간에는 `admin/writer/reader`와
  별도 `owner_id`가 있고, 모듈→항목 아래에 문서·업무·기록을 묶는다. 현행 스키마에 별도 조직 엔티티는 없다.
  따라서 조직 관리 제품이라는 설명은 **추론 이상의 근거가 부족**하다.
  근거: [accounts.rs](../crates/server/src/features/accounts.rs)의 `OidcProvider`,
  [초기 SQL](../crates/server/migrations/202610230001_initial_state.sql)의 `users/workspaces/workspace_members/modules/items`.
- **관찰 — 공개와 개인 영역.** 항목은 비공개/공간 공개와 항목 멤버로 접근을 나누며,
  공간의 비공개 문서는 작성자·관리자 접근과 명시적 공유를 함께 고려한다. 개인 노트는 공간 관리자에게 자동 공개되지 않지만
  소유자가 공유할 수 있다. 읽기·쓰기·복제·반출·첨부 다운로드는 같은 권한이 아니다.
  개인 시간 기록은 워크스페이스 관리자에게도 비공개이며, 소유자가 동의한 시간만 합계에 포함한다. 이는 DB·서버 운영자 대상 암호화를 뜻하지 않는다(H-01).
  근거: SQL의 `app_item_access`, `app_document_base_access`, `app_document_can_export`, `time_entries_select`;
  [sharing.rs](../crates/server/tests/sharing.rs)의 `shared_personal_note_does_not_reach_the_grantees_workspace_admin`,
  [time_tracking.rs](../crates/server/tests/time_tracking.rs)의 `task_totals_include_only_people_who_chose_to_share`.
- **관찰 — 설치 전제.** 웹·Rust API·PostgreSQL과 파일 저장소가 필요하고, 실제 로그인에는 외부 OIDC가 필요하다.
  Keycloak은 선택 제공자, Supabase는 선택 DB 배포 대상이며 Auth/Storage/Realtime 대체가 아니다.
  로컬 파일 저장이 기본이고 S3/R2는 선택이다. Meilisearch가 없거나 응답하지 않으면 제목 DB 검색으로 줄어든다.
  **추론:** 개인 사용자에게도 인증 제공자·DB 역할·주소/HTTPS·백업 준비가 도입 부담이다. Supabase를 이미 쓴다고
  이 부담이 없어지지는 않는다. 근거: [Config](../crates/server/src/config.rs),
  [검색 `search/fallback`](../crates/server/src/features/search_api.rs),
  [셀프호스트](self-hosting.md), [Supabase의 연결·설치 제약](self-hosting-supabase.md).
  [Supavisor의 `prepare_database`](../xtask/src/supabase.rs)는 해당 시험 프로젝트의 풀러를 멈춘 뒤
  DB 역할·마이그레이션을 준비하고 다시 시작한다. 보존한 연결과 SCRAM 캐시의 경합을 다루는
  [회귀 시험](../xtask/src/supabase/tests.rs)의 `kept_pooler_restarts_after_role_preparation`은
  격리된 셀프호스트 시험 근거이며 실제 Supabase Cloud 검증이나 새 인증 정책이 아니다.
- **관찰 — 한도와 규모는 별개.** 기본 설정은 본문 JSON 2,000,000바이트, 제목 유니코드 스칼라 값 200개, 문서 참조 500개,
  트리 깊이 10, 파일 1 GiB, 공간별(개인 영역은 사용자별) 저장량 50 GiB, 조각 8 MiB,
  업로드 조각 동시 수신 8개·ZIP 반출 2개다. 업로드 만료는 24시간이다.
  이는 [Config의 `Limits/FileLimits`](../crates/server/src/config.rs) 기본값이며 O-26의 운영 적정성은 미정이다.
  제목은 [workspaces.rs의 `validate_title`](../crates/server/src/features/workspaces.rs)에서 앞뒤 공백을 제거한 뒤
  Rust `str::chars`로 센다. 화면에서 한 글자로 보이는 묶음(grapheme cluster)의 수와는 다를 수 있다.
  [DB 풀](../crates/server/src/db/mod.rs)은 프로세스당 최대 연결 16·획득 대기 5초다.
  이 숫자를 처리량·동시 사용자 보장으로 바꾸지 않는다. 긴 시간 기록의 정책 상한과 반복 작업 잠금 개선은
  [#96](https://github.com/fvoci/GroundZero/issues/96)·[#95](https://github.com/fvoci/GroundZero/issues/95)로 남아 있다.

## 3. 대표 사용자 작업과 완결도

여기서 **정적 경로 완결**은 명시한 정상 작업과 실패 처리의 UI→API→서버 권한→DB 연결이 소스에 있다는 뜻이다.
브라우저 실행 근거는 위 CI에서 통과한 대표 경로에 한정하며, 나머지 경로는 **실행 미검증**으로 구분한다.
**부분**은 작업 중 남은 단계/실패 공백, **우회**는 수동 대안, **미구현**은 명시한 동작의 부재,
**미검증**은 실행·실사용 근거의 부재다. 화면 경로나 모듈 이름만으로 완결을 판정하지 않는다.

- **공통 문서 작성 — 정적 경로 완결·대표 브라우저 경로 통과.** 로그인한 공간 작성자가 공간과 위키 문서를 만들고
  제목·블록 본문을 저장해 다시 여는 작업이다. 개인 노트는 개인 소유 경로를 쓴다.
  [DocumentEditor](../apps/web/src/lib/features/documents/DocumentEditor.svelte)→
  [클라이언트 `saveDocument`](../apps/web/src/lib/api/client.ts)→
  [서버 `save`](../crates/server/src/features/documents.rs)→SQL의 문서 접근 함수·`documents/document_revisions`로 이어진다.
  자동 저장·초안·수동 저장과 오래된 판의 409/412 처리가 있다. 실패·충돌 때 초안을 보존하는 경로와
  응답 유실 뒤 같은 요청 재시도를 [first_flow.rs](../crates/server/tests/first_flow.rs)의
  `document_write_save_reopen_with_revisions`, `save_retry_is_idempotent_and_stale_save_conflicts`가 다룬다.
  위 CI의 [문서 시험](../apps/web/e2e/document.spec.ts)은 한국어 문자열 입력·저장·새로고침·재열기를,
  [협업 시험](../apps/web/e2e/collab.spec.ts)은 두 사용자의 독립 컨텍스트 편집과 서버 저장·새 컨텍스트 재열기를 확인했다.
  저장 응답 유실·충돌 복구 전체나 OS IME 조합을 브라우저로 확인한 것은 아니다.
  실시간 편집은 공간 설정에 따른 별도 경로다(기본 OFF). 모든 편집이 CRDT로 합쳐진다고 설명하면 틀린다.
- **프로젝트의 업무·일정 조정 — 부분.** 항목에 쓸 수 있는 팀원이 제목·담당자·시작/마감·선행 관계를 넣어
  칸반 상태와 간트 날짜를 바꾸고 달력에서 확인한다.
  [TaskBoard](../apps/web/src/lib/features/tasks/TaskBoard.svelte)·[GanttChart](../apps/web/src/lib/features/tasks/GanttChart.svelte)→
  클라이언트 업무 API→[tasks.rs의 `create/update`](../crates/server/src/features/tasks.rs)→SQL의
  `app_create_task/app_task_access`, `tasks/task_dependencies`가 정본이다.
  통합 달력은 [CalendarScreen](../apps/web/src/lib/features/events/CalendarScreen.svelte)→클라이언트 `calendar`→
  [events.rs의 `calendar`](../crates/server/src/features/events.rs)→공간 멤버·항목 접근 검사/RLS를 거쳐
  업무·일정·개인 학습 계획의 원본을 모은다. 날짜 입력은 시간대 검사와 조회 기간 제한을 받는다.
  [tasks.rs 시험](../crates/server/tests/tasks.rs)·[이동 충돌 UI 시험](../apps/web/tests/component/task-move-undo.test.ts)은
  판 충돌·되돌리기를 다룬다. 위 CI의 [칸반 시험](../apps/web/e2e/board.spec.ts)은 메뉴·키보드 이동과 되돌리기의 서버 반영,
  대화상자 취소·초점 복귀·미저장 입력 보존을 확인했다. 끌어 놓기 검증으로 확대하지 않는다.
  칸반 열 빠른 추가와 달력 업무 생성은 생성 후 이동/날짜 수정의 두 요청이다.
  두 번째 실패 때 중간 업무가 남을 수 있어 다시 확인/수정이 필요하다([#104](https://github.com/fvoci/GroundZero/issues/104)).
  CSV 한 행의 원자 생성(D-105)을 이 두 화면으로 확대하지 않는다.
- **연구·취미의 자료와 기록 축적 — 정적 경로 완결·실행 미검증(수동 기록 범위), 자동화는 우회/미구현.** 항목 작성자가 파일을 올리고
  연구 분석 기록 또는 취미 활동·결과물에 특정 파일 버전과 설명을 연결한다.
  [RecordsPanel](../apps/web/src/lib/features/records/RecordsPanel.svelte)→클라이언트 기록 API→
  [records.rs](../crates/server/src/features/records.rs)의 서버 정의·입력 검사→SQL의 `app_create_record`,
  `item_records/record_files`로 이어지며 항목 권한을 따른다([기록 시험](../crates/server/tests/records.rs)).
  파일 연결 목록 조회 실패는 빈 목록으로 처리되므로 기록을 쓸 수 있어도 첨부 연결은 다시 읽어야 한다.
  외부 분석 실행은 수동 URL/버전 기록으로 우회하며 앱 내 실행·파일 내부 diff는 범위 제외(X-02·X-03)다.
  취미 장비·활동 기록의 존재만으로 자산 관리나 취미 습관 개선의 효과를 주장하지 않는다.
- **스터디·강좌의 개인 진도 — 정적 경로 완결·실행 미검증(수동 진도 범위), 자동 시청 추적 미구현.** 공동 단위/회차는 작성 권한자가
  준비하고 참여자는 자기 계획일·진도·메모를 입력한다. 결과는 자신의 진도와 달력 계획이며 공통 업무 완료와 다르다.
  [StudyPanel](../apps/web/src/lib/features/study/StudyPanel.svelte)·[CoursePanel](../apps/web/src/lib/features/courses/CoursePanel.svelte)→
  `studyPlan/setUnitProgress`, `course/setEpisodeProgress`→[study.rs](../crates/server/src/features/study.rs)·
  [courses.rs](../crates/server/src/features/courses.rs)→SQL의 접근 함수·`unit_progress/episode_progress`로 이어진다.
  공동 단위의 오래된 수정은 충돌로 거절한다. [스터디 시험](../crates/server/tests/study.rs)의
  `study_units_are_shared_and_progress_is_personal`, `two_writers_on_the_same_unit_version_do_not_overwrite`와
  [강좌 시험](../crates/server/tests/courses.rs)의 `course_episodes_and_personal_progress`가 범위를 보여준다.
  외부 자동 시청 추적은 보류(H-02)이며 학습 효과·장기 참여는 미검증이다.
- **개인 시간·검색·첨부 회수 — 부분, 각각 전제 있음.** 로그인 사용자가 타이머로 자기 시간을 남기고,
  권한이 있는 문서·업무·파일을 검색해 다시 연다. 타이머는 사용자당 하나이며
  [시간 UI](../apps/web/src/lib/features/time/TimeEntryDialog.svelte)→[time_tracking.rs](../crates/server/src/features/time_tracking.rs)→
  `time_entries`와 개인 RLS, [SearchDialog](../apps/web/src/lib/features/search/SearchDialog.svelte)→
  `search_api::search`→색인 후보의 서버 권한 재검사/SQL 대체 검색으로 이어진다.
  색인 지연과 검색 서비스 장애 때 제목 검색 축소가 있으므로 즉시 본문 검색을 보장하지 않는다
  ([검색 시험](../crates/server/tests/search.rs)).
  [AttachmentsPanel](../apps/web/src/lib/features/files/AttachmentsPanel.svelte)·[uploadFile](../apps/web/src/lib/features/files/upload.ts)→
  [files.rs의 `create_upload/put_part/complete_upload/content`](../crates/server/src/features/files.rs)→
  업로드 권한·용량 검사와 `uploads/file_versions` 및 저장소로 이어진다. 조각 재개·무결성·완료 재시도는
  [files.rs 시험](../crates/server/tests/files.rs)의 `chunked_upload_resumes_checks_integrity_and_keeps_versions`가 다룬다.
  내려받기 권한·만료·용량 초과를 확인해야 하며 이 경로가 오프라인 작업 전반의 복구를 보장하지는 않는다.

## 4. 불일치·제약과 근거

- **의도한 절충.** 공간 문서의 관리자 접근과 개인 노트 비공개는 서로 다른 범위다. D-40·D-49가 소유 범위를 나눈다.
  공유 철회 뒤 새 요청 권한과 이미 내려받은 사본 회수도 다르다. [sharing.rs](../crates/server/tests/sharing.rs)의
  `share_permissions_and_revocation_take_effect_immediately`는 요청 권한을 다룬다.
  실시간 연결은 [Config](../crates/server/src/config.rs)의 기본 30초 재확인과
  [협업 시험](../crates/server/tests/collab.rs)이 별도 근거다. 모든 철회가 모든 화면·연결에서 즉시 끝난다는 설명은 과하다.
- **구현 결함/설명 불일치 구분.** 달력의 자정 중복일 조회 경계는 알려진 구현 결함
  [#101](https://github.com/fvoci/GroundZero/issues/101)이다. 검색·시간 기록의 시간대 처리를 근거로 달력까지 정확하다고
  설명하면 설명 불일치다. 빠른 업무 생성의 중간 상태는 [#104](https://github.com/fvoci/GroundZero/issues/104)의 기존 사실이며
  D-105의 CSV 한정 결정이 차이를 설명한다. 이 조사에서 새 결함으로 재등록하지 않는다.
- **복구의 존재와 한계.** 문서 판 충돌, 업무·일정의 내 입력 다시 저장, 이동 전 실시간 편집 보존은
  서로 다른 계약이다(D-53·D-94·D-95). [DocumentEditor](../apps/web/src/lib/features/documents/DocumentEditor.svelte),
  [충돌 시험](../apps/web/tests/component/task-conflict-save-mine.test.ts),
  [실시간 인계 시험](../apps/web/tests/component/live-handoff.test.ts)에 입력 보존 근거가 있다.
  늦은 응답·화면 이동 방어를 계정 전환 후 모든 화면 안전성의 완료로 확대하지 않는다
  ([#88](https://github.com/fvoci/GroundZero/issues/88)). 첨부 복원과 철회의 경합은
  [#87](https://github.com/fvoci/GroundZero/issues/87)의 후속 강화이며 새 권한 우회라는 결론이 아니다.
- **일부 실행 근거 — 기기·언어.** [표시 설정](../apps/web/src/lib/display.ts)은 한국어·영어, 사용자 시간대·주 시작·시간 표기를 지원한다.
  [CSS](../apps/web/src/styles/app.css)의 한국어 줄바꿈과 [정적 시험](../apps/web/tests/korean-line-break.test.ts),
  [칸반 키보드 이동 시험](../apps/web/tests/component/task-move-menu.test.ts)이 있다.
  내보내기 파일명·모듈 키·강좌 CSV의 Unicode 경계는
  [웹 회귀 시험](../apps/web/tests/component/unicode-boundaries.test.ts)과 [서버 회귀 시험](../crates/server/tests/unicode_boundaries.rs)이 다룬다.
  [#118](https://github.com/fvoci/GroundZero/issues/118)의 실제 배치 자동화는 #129로 병합됐다.
  위 Chromium CI의 [배치 시험](../apps/web/e2e/layout.spec.ts)과 [간트 시험](../apps/web/e2e/gantt.spec.ts)은
  1280px·375px에서 한국어 줄바꿈·긴 문자열·화면 넘침·간트 내부 가로 스크롤의 대표 조건을 확인했다.
  실제 Safari·Firefox·WebKit·휴대 기기·터치·OS IME 조합·다국어 혼용 전체는 이 묶음으로 검증하지 않았다.
  D-98의 Safari 미실행 범위와 [#112](https://github.com/fvoci/GroundZero/issues/112)의 기존 Orca 캡처 문제도 남는다.
- **관찰 — 홈 조회의 제한된 일관성.** [home.rs의 `marks`](../crates/server/src/features/home.rs)는 핀과 즐겨찾기를
  한 SQL 문장 스냅숏에서 읽고 두 목록으로 나눈다. [home_marks.rs](../crates/server/tests/home_marks.rs)는
  두 종류·순서·현재 접근 범위를 다루며, [후속 시험 #126](https://github.com/fvoci/GroundZero/pull/126)은
  생성 시각과 대상 ID의 순서가 반대인 경우도 확인한다. 홈 전체의 원자적 스냅숏이나 동시 사용 규모를 보장하는 변경은 아니다.
- **정책 미정/운영 미검증.** 한도 조정(O-26), 긴 시간 기록(P-29), 자체 공개 라이선스(O-27)는 정책 판단을 남겨 둔다.
  [backup.rs](../crates/server/src/backup.rs)·[복구 시험](../crates/server/tests/backup.rs)에 DB·첨부의 백업/빈 설치 복구 경로가 있다.
  복구는 마이그레이션 판 일치를 검사하고 세션은 복원하지 않는다. 운영자는 백업 보관·설정·OIDC의 별도 보존까지 준비해야 한다
  ([운영 안내](self-hosting.md)). 실제 운영 복구 시간·복구 시점 목표, 느린 저장소/DB에서의 체감·장애 대응은 미측정이다.

## 5. 방향 적합성과 확인할 질문

**추론:** 근거가 가장 많은 상황은 운영을 맡을 사람이 있고, 문서·업무·일정·파일을 한 항목에 묶어
프로젝트/연구를 기록하거나 스터디/강좌의 진도를 직접 입력하는 작업이다. 공통 정본과 권한·판 충돌 시험이 이 해석을 지지한다.
개인 노트·시간도 같은 도구에 두려는 요구와 연결되지만 실제 선택·만족을 입증하지는 않는다.

근거가 약한 상황은 설치 없이 즉시 시작하려는 개인, 휴대폰·오프라인 중심 사용자, 외부 학습/실험 자동 추적이 핵심인 사용자,
대규모 동시 협업이나 운영 복구 보장이 먼저 필요한 조직이다. 앞의 설치 전제·보류/제외 범위·미측정 항목 때문이다.
이는 시장 배제나 방향 확정이 아니라 검증 순서의 후보이며 새 기능 승인이 아니다.

**제안 — 다음 판단에 필요한 사용자 질문:**

1. 가장 자주 끝내야 하는 실제 작업 하나는 무엇인가? 프로젝트 일정 조정, 연구 근거 정리, 스터디/수강 진도,
   취미 활동 중 하나를 골라 입력부터 다음날 다시 찾기까지 성공 기준을 정할 수 있는가?
2. 혼자 쓰는 경우에도 OIDC·DB·백업을 준비할 운영자가 있는가? 없다면 현재 설치 부담을 감수할 이유가 무엇인가?
3. 팀 기록과 개인 노트·시간을 함께 둘 때, 관리자 열람·명시적 공유·다운로드 제한의 현재 경계를 사용자가 예상하는가?
4. 주 기기는 데스크톱인가, 좁은 화면·터치인가? 한국어/영어 혼용과 어떤 시간대에서 어떤 작업을 실제로 확인해야 하는가?
5. 저장 응답 유실·충돌·권한 철회 뒤 입력 보존/재시도와 운영 백업 복구 중 반드시 증명해야 할 것은 무엇인가?
   필요한 데이터량·동시 작업·허용 중단 시간을 실제 작업으로 정할 수 있는가?
