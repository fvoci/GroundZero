-- 개발 전용: cargo xtask dev --oidc가 dev-seed.sql 다음에 실행한다(운영·셀프호스트는 실행하지 않는다).
-- dev-seed.sql 다음에만 실행한다. 기존 테스트 사용자만 조회하며 계정·자격 증명을 만들거나 바꾸지 않는다.
-- 제품 데이터는 기존 app_* 정의자 함수로만 생성한다. JSON 칸·선택값은 features/item_profile.rs와
-- features/records.rs의 정의에 맞춘다. 파일 메타데이터 예제는 기록이며 실제 업로드나 외부 접속은 하지 않는다.
-- 하나의 DO 트랜잭션으로 예제와 완료 표시를 함께 저장한다. 잠금 후 표시를 확인하므로 동시 실행도 안전하다.
-- 완료 표시가 있으면 예제의 편집·삭제·보관 상태를 그대로 둔다. 재실행으로 복원하거나 덮어쓰지 않는다.
-- 새 행 id는 앞에 붙는 dev-ids.sql의 `pg_temp.take`로 받는다(D-88). 개수가 정해지지 않는 함수는 그 `_ids`로 묻는다.
SET search_path = fvoci, pg_temp;
SET fvoci.dev_issuer = :'issuer';

DO $seed$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_ws uuid;
  v_inv uuid;
  v_module uuid;
  v_item uuid;
  v_course uuid;
  v_study uuid;
  v_task uuid;
  v_child uuid;
  v_milestone uuid;
  v_together uuid;
  v_linked uuid;
  v_doc uuid;
  v_unit uuid;
  v_episode uuid;
  v_row jsonb;
  v_items jsonb := '{}'::jsonb;
  v_documents jsonb := '{}'::jsonb;
  v_today date := current_date;
  v_kind text;
  v_time_kind text;
  v_version integer;
  v_outcome text;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('fvoci:dev_module_seed_v1', 0));
  IF EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_module_seed_v1') THEN
    RETURN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_seed') THEN
    RAISE EXCEPTION 'run the existing dev seed first';
  END IF;
  SELECT id INTO v_admin FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000a';
  SELECT id INTO v_user FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000b';
  IF v_admin IS NULL OR v_user IS NULL THEN
    RAISE EXCEPTION 'existing dev test users are required for this issuer';
  END IF;

  PERFORM set_config('app.user_id', v_admin::text, true);
  v_ws := app_create_workspace(pg_temp.take1(), '모듈 예제 (합성 데이터)', pg_temp.take(2));
  v_inv := app_invite(v_ws, 'test-user-2@example.test', 'writer', 7 * 24 * 3600, pg_temp.take(2));
  PERFORM set_config('app.user_id', v_user::text, true);
  PERFORM app_respond_invitation(v_inv, true, pg_temp.take(1));
  PERFORM set_config('app.user_id', v_admin::text, true);

  -- 각 모듈에는 독립된 전용 정보와 동일한 공통 기능을 둔다. 공통 기능 재사용과 데이터 혼선을 구별한다.
  FOR v_row IN SELECT * FROM jsonb_array_elements('[
    {"kind":"project","name":"프로젝트 예제","key":"DEVP","title":"작은 출시 준비","goal":"대표 사용자 흐름을 점검한다","details":{}},
    {"kind":"research","name":"연구 예제","key":"DEVR","title":"학습 습관 합성 연구","goal":"실제 개인정보 없이 자료 관리 흐름을 확인한다","details":{"field":"교육 연구","keywords":["합성 데이터","학습 습관"],"question":"짧은 복습이 기억에 도움이 되는가?","hypothesis":"분산 복습 집단의 회상률이 높다","model":"두 집단 비교","method":"합성 관측값 비교","sample":"가상 참여자 12명"}},
    {"kind":"study","name":"스터디 예제","key":"DEVS","title":"SQL 기초 함께 읽기","goal":"공통 계획과 개인별 진행을 구분한다","details":{"level":"입문","target":"20페이지와 연습 문제 8개"}},
    {"kind":"course","name":"강의 예제","key":"DEVC","title":"데이터 입문 강좌","goal":"공유 목차와 개인 진도를 구분한다","details":{}},
    {"kind":"hobby","name":"취미 예제","key":"DEVH","title":"주말 수채화","goal":"준비부터 결과물까지 활동을 기록한다","details":{"activity_type":"수채화"}}
  ]'::jsonb) LOOP
    v_kind := v_row->>'kind';
    v_module := app_create_module(v_ws, pg_temp.take1(), v_kind, v_row->>'name', v_row->>'key', pg_temp.take(2));
    PERFORM app_set_module_description(v_module, '개발 전용 합성 예제. 실제 고객·연구 자료가 아닙니다.');
    v_item := app_create_item(v_module, pg_temp.take1(), v_row->>'title', '수정·저장·새로고침 후 재조회하는 개발 예제',
                              pg_temp.take(2));
    -- 새 항목의 기본 공개 범위는 private다. 두 번째 테스트 사용자를 명시적 항목 멤버로 추가한다.
    PERFORM app_set_item_members(v_item, ARRAY[v_user], ARRAY[]::uuid[],
                                 pg_temp.take(app_set_item_members_ids(v_item)));
    -- 정보 패널 저장은 읽은 판을 받는다(D-104). 멤버 정리도 판을 올릴 수 있어 바로 앞에서 읽는다.
    SELECT profile_version INTO STRICT v_version FROM items WHERE id = v_item;
    v_outcome := app_set_item_profile(v_item, v_version, v_admin, v_row->>'goal', '모듈 연결과 저장 동작 확인',
      v_today - 7, v_today + 14, 'active', v_row->'details', pg_temp.take(2));
    IF v_outcome <> 'updated' THEN
      RAISE EXCEPTION 'dev item profile was not saved: %', v_outcome;
    END IF;
    v_items := v_items || jsonb_build_object(v_kind, v_item);

    -- 업무는 모든 칸과 함께 한 번에 만든다(D-105). id: 업무·만들기 감사 + 담당자마다 1개.
    v_task := app_create_task(v_item, pg_temp.take1(), '대표 업무: 준비 사항 확인', NULL,
      '모든 담당자가 같은 업무 상태를 공유합니다.', 'in_progress', 'medium', v_today, v_today + 2, 45,
      ARRAY['개발예제'], ARRAY[v_admin, v_user], false, pg_temp.take(4));
    PERFORM app_add_checklist_items(v_task, jsonb_build_array(
      jsonb_build_object('request_id', pg_temp.take1(), 'text', '준비 자료 확인', 'checked', true),
      jsonb_build_object('request_id', pg_temp.take1(), 'text', '저장 후 다시 열기', 'checked', false)),
      pg_temp.take(2));
    v_doc := app_create_module_document(v_item, pg_temp.take1(), '공동 작업 노트', 1,
      '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"개발용 합성 문서입니다. 업무 완료와 개인 학습 진도는 별도로 저장됩니다."}]},{"type":"taskList","content":[{"type":"taskItem","attrs":{"checked":false},"content":[{"type":"paragraph","content":[{"type":"text","text":"문서 체크리스트: 내용을 검토한다"}]}]}]}]}',
      NULL, 10, pg_temp.take(3));
    v_documents := v_documents || jsonb_build_object(v_kind, v_doc);
    PERFORM app_create_event(v_item, pg_temp.take1(), 'meeting', '예제 점검 모임', '공통 일정의 모듈별 표시 확인',
      true, v_today + 1, v_today + 1, NULL, NULL, 'UTC', pg_temp.take(2));
    v_time_kind := CASE v_kind WHEN 'study' THEN 'study' WHEN 'course' THEN 'lecture'
      WHEN 'hobby' THEN 'hobby' ELSE 'work' END;
    PERFORM app_add_time_entry(pg_temp.take1(), now() - interval '90 minutes', now() - interval '60 minutes',
      0, v_time_kind, '개발 예제: 종료된 개인 시간 기록', 'task', v_task, pg_temp.take(1));
    IF v_kind = 'project' THEN
      v_child := app_create_task(v_item, pg_temp.take1(), '하위 업무: 검토 목록 작성', v_task, '', 'done', 'low',
        v_today, v_today, 15, ARRAY['개발예제'], ARRAY[v_admin], false, pg_temp.take(3));
      v_milestone := app_create_task(v_item, pg_temp.take1(), '마일스톤: 검토 완료', NULL, '', 'todo', 'high',
        NULL, v_today + 3, NULL, ARRAY['개발예제'], ARRAY[v_admin], true, pg_temp.take(3));
      PERFORM app_set_task_dependency(v_task, v_milestone, 'FS', 0, NULL, NULL, '[]'::jsonb, pg_temp.take(1));
      -- 관계 종류 예제(D-91): 대표 업무와 함께 시작(SS·1), 함께 끝(FF·0), 그 업무가 시작하기 전에 끝(SF·0).
      -- 날짜는 이미 관계를 채우므로 옮길 업무가 없다.
      v_together := app_create_task(v_item, pg_temp.take1(), '관계 예제: 하루 뒤 함께 시작', NULL, '', 'todo', 'medium',
        v_today + 1, v_today + 3, NULL, ARRAY['개발예제'], ARRAY[v_admin], false, pg_temp.take(3));
      PERFORM app_set_task_dependency(v_task, v_together, 'SS', 1, NULL, NULL, '[]', pg_temp.take(1));
      v_linked := app_create_task(v_item, pg_temp.take1(), '관계 예제: 함께 마무리', NULL, '', 'todo', 'low',
        v_today + 1, v_today + 2, NULL, ARRAY['개발예제'], ARRAY[v_admin], false, pg_temp.take(3));
      PERFORM app_set_task_dependency(v_task, v_linked, 'FF', 0, NULL, NULL, '[]', pg_temp.take(1));
      v_linked := app_create_task(v_item, pg_temp.take1(), '관계 예제: 시작 전에 준비 끝', NULL, '', 'todo', 'low',
        v_today - 1, v_today, NULL, ARRAY['개발예제'], ARRAY[v_admin], false, pg_temp.take(3));
      PERFORM app_set_task_dependency(v_together, v_linked, 'SF', 0, NULL, NULL, '[]', pg_temp.take(1));
    END IF;
  END LOOP;

  -- 강좌 세부 정보와 세 회차. 날짜는 처음 실행한 날을 기준으로 늦음·오늘·예정을 함께 보여 준다.
  v_course := (v_items->>'course')::uuid;
  PERFORM app_set_course_details(v_course, '데이터 기초', '가상 강사', '개발 예제',
    'https://example.test/course', '가상 교재 1장', v_today - 7, v_today + 30, v_today + 7, pg_temp.take(1));
  PERFORM app_add_episodes(v_course, jsonb_build_array(
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 1, 'title', '데이터와 표', 'chapter', '기초',
      'duration_seconds', 600, 'materials', '가상 교재 1–5쪽', 'planned_on', v_today - 1),
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 2, 'title', '질문 만들기', 'chapter', '기초',
      'duration_seconds', 900, 'materials', '가상 교재 6–10쪽', 'planned_on', v_today),
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 3, 'title', '결과 읽기', 'chapter', '적용',
      'duration_seconds', 1200, 'materials', '가상 교재 11–15쪽', 'planned_on', v_today + 1)), pg_temp.take(4));
  SELECT id INTO STRICT v_episode FROM course_episodes WHERE item_id = v_course AND number = 1;
  PERFORM app_set_episode_progress(v_episode, 'done', NULL, 600, '내 비공개 메모: 첫 회차 완료', v_today);
  PERFORM app_add_time_entry(pg_temp.take1(), now() - interval '50 minutes', now() - interval '40 minutes',
    0, 'lecture', '첫 회차 시청', 'episode', v_episode, pg_temp.take(1));
  PERFORM set_config('app.user_id', v_user::text, true);
  PERFORM app_set_episode_progress(v_episode, 'in_progress', v_today + 1, 240,
    '두 번째 사용자의 비공개 메모', v_today);
  PERFORM set_config('app.user_id', v_admin::text, true);
  SELECT id INTO STRICT v_episode FROM course_episodes WHERE item_id = v_course AND number = 2;
  PERFORM app_set_episode_progress(v_episode, 'in_progress', NULL, 120, '질문을 적으며 시청 중', v_today);

  v_study := (v_items->>'study')::uuid;
  PERFORM app_add_study_units(v_study, jsonb_build_array(
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 1, 'title', '1장 읽기', 'chapter', '입문',
      'unit', 'pages', 'amount', 20, 'planned_on', v_today - 1),
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 2, 'title', '연습 문제', 'chapter', '입문',
      'unit', 'problems', 'amount', 8, 'planned_on', v_today),
    jsonb_build_object('request_id', pg_temp.take1(), 'number', 3, 'title', '개념 복습', 'chapter', '복습',
      'unit', 'minutes', 'amount', 25, 'planned_on', v_today + 1)), pg_temp.take(4));
  SELECT id INTO STRICT v_unit FROM study_units WHERE item_id = v_study AND number = 1;
  PERFORM app_set_unit_progress(v_unit, 'done', 20, NULL, '내 비공개 독서 메모', v_today);
  PERFORM app_add_time_entry(pg_temp.take1(), now() - interval '35 minutes', now() - interval '10 minutes',
    0, 'study', '강의 시청과 구분되는 능동 학습', 'study_unit', v_unit, pg_temp.take(1));
  PERFORM set_config('app.user_id', v_user::text, true);
  PERFORM app_set_unit_progress(v_unit, 'in_progress', 8, v_today + 1, '두 번째 사용자의 독서 메모', v_today);
  PERFORM set_config('app.user_id', v_admin::text, true);
  SELECT id INTO STRICT v_unit FROM study_units WHERE item_id = v_study AND number = 2;
  PERFORM app_set_unit_progress(v_unit, 'in_progress', 3, NULL, '나머지 다섯 문제를 복습한다', v_today);

  -- 구현된 기록 종류 18종. 빈 날짜는 그 종류가 날짜 칸을 받지 않는다는 뜻이다.
  FOR v_row IN SELECT * FROM jsonb_array_elements('[
    {"module":"project","kind":"deliverable","title":"검토 보고서","day":3,"fields":{"status":"in_progress","criteria":"대표 흐름 다섯 개를 재조회한다"}},
    {"module":"project","kind":"risk","title":"일정 지연 위험","day":0,"fields":{"likelihood":"medium","impact":"low","status":"mitigating","mitigation":"작은 범위부터 확인한다"}},
    {"module":"project","kind":"issue","title":"가상 검토 누락","day":0,"fields":{"severity":"low","status":"resolved","resolution":"체크리스트를 보완했다"}},
    {"module":"project","kind":"retrospective","title":"첫 검토 회고","day":0,"fields":{"went_well":"공통 정본을 확인했다","to_improve":"화면에서 목적을 더 설명한다","actions":"다음 예제를 다시 읽는다"}},
    {"module":"research","kind":"reference","title":"합성 문헌: 분산 복습","fields":{"authors":"가상 연구자","year":2026,"venue":"개발용 가상 학술지","url":"https://example.test/reference","read_status":"reading","tags":["합성","복습"],"summary":"실제 논문이 아닌 입력 예제","method_note":"가상 집단 비교","findings":"결과는 합성 값이다","limitations":"실제 연구 근거로 사용하지 않는다","citation":"개발용 가상 인용","pages":"1–4"}},
    {"module":"research","kind":"research_log","title":"합성 랩미팅","day":0,"fields":{"log_type":"lab_meeting"}},
    {"module":"research","kind":"dataset","title":"가상 학습 관측값","day":-1,"fields":{"stage":"raw","source":"개발자가 만든 합성 값","location":"example.test/synthetic-data (설명용 위치)","data_version":"v1","quality":"실제 개인정보 없음; 결측 0건","access_note":"개발 예제 참여자"}},
    {"module":"research","kind":"variable","title":"recall_score","fields":{"label":"가상 회상 점수","var_type":"numeric","values":"0–10","measurement":"합성 점수","dataset":"가상 학습 관측값"}},
    {"module":"research","kind":"analysis","title":"합성 평균 비교","day":0,"fields":{"purpose":"자료와 분석 메타데이터 분리 확인","status":"done","settings":"가상 두 집단","log":"앱에서 코드를 실행하지 않음","compute_minutes":2,"results":"가상 평균 7.5","compared_with":"가상 기준 실행","comparison":"설명용 결과"}},
    {"module":"research","kind":"submission","title":"가상 초록 제출","day":7,"fields":{"venue":"가상 학술대회","status":"preparing","checklist":"제목과 저자를 검토한다","irb":"합성 데이터 예제; 실제 심의 없음"}},
    {"module":"study","kind":"material","title":"연결 강좌와 교재","fields":{"material_type":"course","url":"https://example.test/course","amount":"20페이지·8문제","chapters":"입문 → 연습 → 복습"}},
    {"module":"study","kind":"meeting","title":"첫 스터디 모임","day":1,"fields":{"assignment":"1장 읽고 질문 하나 준비"}},
    {"module":"study","kind":"exam","title":"개념 확인","day":0,"fields":{"score":8,"max_score":10,"feedback":"집계 함수 복습"}},
    {"module":"study","kind":"review","title":"집계 함수 복습","day":2,"fields":{"topic":"GROUP BY","weakness":"그룹 조건과 행 조건 구분","rounds":1}},
    {"module":"hobby","kind":"supply","title":"수채 물감과 붓","fields":{"supply_type":"material","state":"ready","quantity":"1세트","settings":"작은 팔레트"}},
    {"module":"hobby","kind":"equipment_use","title":"붓 사용 기록","day":0,"fields":{"equipment":"둥근 붓 6호","settings":"물 적게 사용"}},
    {"module":"hobby","kind":"activity","title":"잎사귀 연습","day":0,"fields":{"minutes":30,"result":"습작 두 장","learned":"물의 양을 줄이니 경계가 선명해졌다"}},
    {"module":"hobby","kind":"portfolio","title":"첫 잎사귀 습작","day":0,"fields":{"retrospective":"명암 단계를 연습했다","next_goal":"다음에는 꽃 한 송이"}}
  ]'::jsonb) LOOP
    IF v_row->>'kind' = 'material' THEN
      v_row := jsonb_set(v_row, '{fields,course}', to_jsonb(v_course));
    ELSIF v_row->>'kind' IN ('deliverable', 'risk', 'issue', 'analysis') THEN
      v_row := jsonb_set(v_row, '{fields,owner}', to_jsonb(v_admin));
    ELSIF v_row->>'kind' IN ('research_log', 'activity') THEN
      v_row := jsonb_set(v_row, '{fields,participants}', jsonb_build_array(v_admin, v_user));
    ELSIF v_row->>'kind' = 'submission' THEN
      v_row := jsonb_set(v_row, '{fields,coauthors}', jsonb_build_array(v_admin, v_user));
    ELSIF v_row->>'kind' = 'meeting' THEN
      v_row := jsonb_set(v_row, '{fields,presenter}', to_jsonb(v_admin));
      v_row := jsonb_set(v_row, '{fields,attendees}', jsonb_build_array(v_admin, v_user));
    ELSIF v_row->>'kind' = 'exam' THEN
      v_row := jsonb_set(v_row, '{fields,taker}', to_jsonb(v_admin));
    END IF;
    PERFORM app_create_record(pg_temp.take1(), (v_items->>(v_row->>'module'))::uuid, v_row->>'kind', v_row->>'title',
      CASE WHEN v_row ? 'day' THEN v_today + (v_row->>'day')::integer END,
      '개발용 합성 기록. 실제 자료가 아닙니다.', v_row->'fields', '[]', pg_temp.take(2));
  END LOOP;
  PERFORM set_config('app.user_id', '', true);
  INSERT INTO instance_state (key, value) VALUES ('dev_module_seed_v1', jsonb_build_object(
    'workspace', v_ws, 'items', v_items, 'documents', v_documents, 'seeded_on', v_today));
END
$seed$;

SELECT value FROM instance_state WHERE key = 'dev_module_seed_v1';

-- 보강(v2): 같은 합성 워크스페이스에 칸반 카드 여럿(열 이동·순서 바꾸기 체험), 위키 트리 세 단계와 서로 가리키는
-- 문서(백링크), 구분선 바로 아래 체크리스트가 있는 문서를 더한다. v1과 따로 자기 표시(dev_module_seed_v2)로 한 번만
-- 실행하며 기존 행을 고치거나 지우지 않는다. 사용자가 예제 워크스페이스를 지웠으면 아무것도 만들지 않고 표시만
-- 남기며(다시 채우지 않음), 지웠거나 보관한 항목에는 업무를 더하지 않는다. 만들다 실패하면 이 블록만 되돌리고 알린 뒤
-- 개발 실행은 계속한다(다음 실행에 다시 시도).
DO $seed2$
DECLARE
  v_admin uuid;
  v_ws uuid;
  v_v1 jsonb;
  v_kind text;
  v_item uuid;
  v_task uuid;
  v_row jsonb;
  v_today date := current_date;
  v_guide uuid;
  v_install uuid;
  v_backup uuid;
  v_log uuid;
  v_added integer := 0;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('fvoci:dev_module_seed_v2', 0));
  IF EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_module_seed_v2') THEN
    RETURN;
  END IF;
  SELECT value INTO v_v1 FROM instance_state WHERE key = 'dev_module_seed_v1';
  IF v_v1 IS NULL THEN
    RETURN;
  END IF;
  v_ws := (v_v1->>'workspace')::uuid;
  IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = v_ws AND deleted_at IS NULL) THEN
    INSERT INTO instance_state (key, value)
      VALUES ('dev_module_seed_v2', jsonb_build_object('skipped', 'example workspace removed'));
    RETURN;
  END IF;
  SELECT id INTO v_admin FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000a';
  IF v_admin IS NULL THEN
    RAISE EXCEPTION 'existing dev test users are required for this issuer';
  END IF;

  BEGIN
    PERFORM set_config('app.user_id', v_admin::text, true);

    -- 칸반: 모듈마다 카드를 더한다(우선순위·마감 섞음). v1과 합쳐 프로젝트는 열마다 두 장 이상(같은 열 안 순서 바꾸기),
    -- 나머지 모듈은 열마다 한 장(열 사이 이동).
    FOR v_kind IN SELECT jsonb_object_keys(v_v1->'items') LOOP
      v_item := (v_v1->'items'->>v_kind)::uuid;
      CONTINUE WHEN NOT EXISTS (
        SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
        WHERE i.id = v_item AND i.deleted_at IS NULL AND m.deleted_at IS NULL
          AND i.archived_at IS NULL AND m.archived_at IS NULL);
      FOR v_row IN SELECT * FROM jsonb_array_elements(CASE WHEN v_kind = 'project' THEN '[
        {"title":"API 연결 점검","status":"todo","priority":"high","due":4},
        {"title":"문서 초안 검토","status":"todo","priority":"low","due":6},
        {"title":"배포 체크리스트 확인","status":"in_progress","priority":"urgent","due":1},
        {"title":"회고 일정 잡기","status":"done","priority":"none"}
      ]' ELSE '[
        {"title":"자료 정리","status":"todo","priority":"medium","due":3},
        {"title":"지난 활동 돌아보기","status":"done","priority":"low"}
      ]' END::jsonb) LOOP
        v_task := app_create_task(v_item, pg_temp.take1(), v_row->>'title', NULL, '개발용 합성 업무입니다.',
          v_row->>'status', v_row->>'priority', NULL,
          CASE WHEN v_row ? 'due' THEN v_today + (v_row->>'due')::integer END,
          NULL, ARRAY['개발예제'], ARRAY[v_admin], false, pg_temp.take(3));
        v_added := v_added + 1;
      END LOOP;
    END LOOP;

    -- 위키: 운영 안내 › 설치 › 백업 세 단계와, 설치·백업을 가리키는 점검 기록(백링크). 운영 안내 본문은 구분선
    -- 바로 아래 체크리스트가 있어 블록 손잡이·구분선 다루기를 해 볼 수 있다.
    v_guide := app_create_wiki_document(v_ws, pg_temp.take1(), '운영 안내 (합성)', 1,
      '{"type":"doc","content":[
        {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"개요"}]},
        {"type":"paragraph","content":[{"type":"text","text":"개발용 합성 위키입니다. 트리에서 하위 문서를 펼치고 Alt+방향키로 옮겨 보세요."}]},
        {"type":"horizontalRule"},
        {"type":"taskList","content":[
          {"type":"taskItem","attrs":{"checked":true},"content":[{"type":"paragraph","content":[{"type":"text","text":"설치 문서 읽기"}]}]},
          {"type":"taskItem","attrs":{"checked":false},"content":[{"type":"paragraph","content":[{"type":"text","text":"백업 절차 따라 해 보기"}]}]}
        ]},
        {"type":"bulletList","content":[
          {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"구분선 손잡이를 끌어 위치를 바꿔 볼 수 있습니다."}]}]}
        ]}
      ]}'::jsonb, NULL, 10, pg_temp.take(3));
    v_install := app_create_wiki_document(v_ws, pg_temp.take1(), '설치', 1, jsonb_build_object(
      'type', 'doc', 'content', jsonb_build_array(
        jsonb_build_object('type', 'paragraph', 'content',
          -- 예제 프로젝트 항목을 지웠으면 가리킬 대상이 없으므로 참조 없이 글만 둔다.
          CASE WHEN EXISTS (SELECT 1 FROM items WHERE id = (v_v1->'items'->>'project')::uuid AND deleted_at IS NULL)
            THEN jsonb_build_array(
              jsonb_build_object('type', 'text', 'text', '설치 전에 확인할 항목: '),
              jsonb_build_object('type', 'reference', 'attrs', jsonb_build_object('kind', 'item', 'id', v_v1->'items'->>'project')))
            ELSE jsonb_build_array(jsonb_build_object('type', 'text', 'text', '설치 전에 예제 프로젝트 항목을 확인합니다.'))
          END))),
      v_guide, 10, pg_temp.take(3));
    v_backup := app_create_wiki_document(v_ws, pg_temp.take1(), '백업', 1,
      '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"합성 백업 절차. 실제 서버 정보가 아닙니다."}]}]}'::jsonb,
      v_install, 10, pg_temp.take(3));
    v_log := app_create_wiki_document(v_ws, pg_temp.take1(), '점검 기록', 1, jsonb_build_object(
      'type', 'doc', 'content', jsonb_build_array(
        jsonb_build_object('type', 'paragraph', 'content', jsonb_build_array(
          jsonb_build_object('type', 'text', 'text', '이번 점검에서 본 문서: '),
          jsonb_build_object('type', 'reference', 'attrs', jsonb_build_object('kind', 'document', 'id', v_install)),
          jsonb_build_object('type', 'text', 'text', ', '),
          jsonb_build_object('type', 'reference', 'attrs', jsonb_build_object('kind', 'document', 'id', v_backup)))))),
      NULL, 10, pg_temp.take(3));
    -- 저장할 때와 같은 함수로 참조를 기록해 대상 문서·항목의 "이곳을 가리키는 문서"에 보이게 한다(알림은 보내지 않음).
    PERFORM app_record_document_references(v_install, false,
      pg_temp.take(app_record_document_references_ids(v_install, false)));
    PERFORM app_record_document_references(v_log, false,
      pg_temp.take(app_record_document_references_ids(v_log, false)));

    PERFORM set_config('app.user_id', '', true);
    INSERT INTO instance_state (key, value) VALUES ('dev_module_seed_v2', jsonb_build_object(
      'workspace', v_ws, 'tasks_added', v_added,
      'wiki', jsonb_build_object('guide', v_guide, 'install', v_install, 'backup', v_backup, 'log', v_log),
      'seeded_on', v_today));
  EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'dev module examples v2 not added (%: %); the dev run continues and tries again next time',
      SQLSTATE, SQLERRM;
  END;
END
$seed2$;

SELECT value FROM instance_state WHERE key = 'dev_module_seed_v2';
