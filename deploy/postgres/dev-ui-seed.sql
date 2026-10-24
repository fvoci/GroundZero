-- 개발 전용: cargo xtask dev --oidc가 dev-module-seed.sql 다음에 실행한다(운영·셀프호스트는 실행하지 않는다).
-- Storybook과 같은 합성 데이터(dev-ui-data.json, xtask가 세션 설정 fvoci.dev_ui_data로 넘긴다)로 "UI 상태 예제"
-- 워크스페이스를 한 번만 만든다.
-- 긴 한국어·공백 없는 문자열·이모지·숫자가 섞인 위키 문서 트리, 답글과 해결된 스레드가 있는 댓글, 상태·우선순위·
-- 담당자가 다른 업무를 실제 앱에서도 같은 내용으로 본다. 기존 테스트 사용자만 조회하고 계정을 만들거나 바꾸지 않는다.
-- 제품 데이터는 기존 app_* 정의자 함수로만 만든다. 'dev_ui_seed_v1' 표시가 있으면 아무것도 바꾸지 않는다(편집·삭제한
-- 예제를 되살리거나 덮어쓰지 않는다). 새 워크스페이스만 만들므로 실패는 결함이다: 전부 되돌리고 원래 오류 코드로
-- 실패시켜 `cargo xtask dev`가 멈추게 한다(경고로 바꿔 성공처럼 넘기지 않는다).
-- 새 행 id는 앞에 붙는 dev-ids.sql의 `pg_temp.take`로 받는다(D-88).
SET search_path = fvoci, pg_temp;
SET fvoci.dev_issuer = :'issuer';

DO $seed$
DECLARE
  v_data jsonb := current_setting('fvoci.dev_ui_data')::jsonb;
  v_people jsonb;
  v_admin uuid;
  v_user uuid;
  v_ws uuid;
  v_inv uuid;
  v_docs jsonb := '{}'::jsonb;
  v_doc uuid;
  v_row jsonb;
  v_reply jsonb;
  v_root uuid;
  v_target uuid;
  v_module uuid;
  v_item uuid;
  v_today date := current_date;
  v_assignees uuid[];
  v_tasks integer := 0;
  v_comments integer := 0;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('fvoci:dev_ui_seed_v1', 0));
  IF EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_ui_seed_v1') THEN
    RETURN;
  END IF;
  SELECT id INTO v_admin FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000a';
  SELECT id INTO v_user FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000b';
  IF v_admin IS NULL OR v_user IS NULL THEN
    RAISE EXCEPTION 'existing dev test users are required for this issuer';
  END IF;
  -- 데이터의 사람 열쇠(owner·writer)를 테스트 사용자에 잇는다.
  v_people := jsonb_build_object('owner', v_admin, 'writer', v_user);

  BEGIN
    PERFORM set_config('app.user_id', v_admin::text, true);
    v_ws := app_create_workspace(pg_temp.take1(), v_data->>'workspace', pg_temp.take(2));
    v_inv := app_invite(v_ws, 'test-user-2@example.test', 'writer', 7 * 24 * 3600, pg_temp.take(2));
    PERFORM set_config('app.user_id', v_user::text, true);
    PERFORM app_respond_invitation(v_inv, true, pg_temp.take(1));
    PERFORM set_config('app.user_id', v_admin::text, true);

    -- 위키 문서: 데이터 순서대로(부모가 먼저 온다). 빈 본문은 빈 문단 하나다.
    FOR v_row IN SELECT * FROM jsonb_array_elements(v_data->'documents') LOOP
      v_doc := app_create_wiki_document(v_ws, pg_temp.take1(), v_row->>'title', 1, jsonb_build_object(
        'type', 'doc', 'content', jsonb_build_array(
          CASE WHEN v_row->>'body' = '' THEN jsonb_build_object('type', 'paragraph')
          ELSE jsonb_build_object('type', 'paragraph', 'content', jsonb_build_array(
            jsonb_build_object('type', 'text', 'text', v_row->>'body'))) END)),
        (v_docs->>(v_row->>'parent'))::uuid, 10, pg_temp.take(3));
      v_docs := v_docs || jsonb_build_object(v_row->>'key', v_doc);
    END LOOP;

    -- 댓글: 스레드마다 쓴 사람으로 달고, 답글은 그 스레드에, 해결 표시는 마지막에 단다.
    v_target := (v_docs->>(v_data->'comments'->>'document'))::uuid;
    FOR v_row IN SELECT * FROM jsonb_array_elements(v_data->'comments'->'threads') LOOP
      PERFORM set_config('app.user_id', v_people->>(v_row->>'author'), true);
      v_root := app_add_comment(pg_temp.take1(), v_target, NULL, NULL, v_row->>'body',
                                pg_temp.take(app_add_comment_ids(v_target, NULL, NULL)));
      v_comments := v_comments + 1;
      FOR v_reply IN SELECT * FROM jsonb_array_elements(v_row->'replies') LOOP
        PERFORM set_config('app.user_id', v_people->>(v_reply->>'author'), true);
        PERFORM app_add_comment(pg_temp.take1(), v_target, NULL, v_root, v_reply->>'body',
                                pg_temp.take(app_add_comment_ids(v_target, NULL, v_root)));
        v_comments := v_comments + 1;
      END LOOP;
      IF (v_row->>'resolved')::boolean THEN
        PERFORM set_config('app.user_id', v_admin::text, true);
        PERFORM app_resolve_comment(v_root, true);
      END IF;
    END LOOP;

    -- 업무: 프로젝트 모듈의 한 항목에. 날짜는 처음 실행한 날 기준의 앞뒤 일수다.
    PERFORM set_config('app.user_id', v_admin::text, true);
    v_module := app_create_module(v_ws, pg_temp.take1(), 'project', v_data->'module'->>'name',
                                  v_data->'module'->>'key', pg_temp.take(2));
    PERFORM app_set_module_description(v_module, '개발 전용 합성 예제. Storybook과 같은 데이터입니다.');
    v_item := app_create_item(v_module, pg_temp.take1(), v_data->'module'->>'item',
                              '긴 제목·이모지·숫자·상태가 다른 업무로 화면을 점검합니다.', pg_temp.take(2));
    PERFORM app_set_item_members(v_item, ARRAY[v_user], ARRAY[]::uuid[],
                                 pg_temp.take(app_set_item_members_ids(v_item)));
    FOR v_row IN SELECT * FROM jsonb_array_elements(v_data->'tasks') LOOP
      SELECT coalesce(array_agg((v_people->>a)::uuid), ARRAY[]::uuid[]) INTO v_assignees
        FROM jsonb_array_elements_text(v_row->'assignees') AS a;
      -- id: 업무·만들기 감사 + 담당자마다 1개.
      PERFORM app_create_task(v_item, pg_temp.take1(), v_row->>'title', NULL, v_row->>'description',
        v_row->>'status', v_row->>'priority', v_today + (v_row->>'start')::integer,
        v_today + (v_row->>'due')::integer, (v_row->>'estimate')::integer,
        ARRAY(SELECT jsonb_array_elements_text(v_row->'tags')), v_assignees,
        (v_row->>'milestone')::boolean, pg_temp.take(2 + cardinality(v_assignees)));
      v_tasks := v_tasks + 1;
    END LOOP;

    PERFORM set_config('app.user_id', '', true);
    INSERT INTO instance_state (key, value) VALUES ('dev_ui_seed_v1', jsonb_build_object(
      'workspace', v_ws, 'documents', v_docs, 'comments', v_comments, 'tasks', v_tasks, 'seeded_on', v_today));
  EXCEPTION WHEN OTHERS THEN
    -- 무엇을 하다 실패했는지 붙여 원래 오류 코드 그대로 다시 낸다.
    RAISE EXCEPTION 'dev UI examples not added: %', SQLERRM USING ERRCODE = SQLSTATE;
  END;
END
$seed$;

SELECT value FROM instance_state WHERE key = 'dev_ui_seed_v1';
