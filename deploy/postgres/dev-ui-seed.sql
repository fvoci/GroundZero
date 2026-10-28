-- 개발 전용: cargo xtask dev --oidc가 dev-module-seed.sql 다음에 실행한다(운영·셀프호스트는 실행하지 않는다).
-- Storybook과 같은 합성 데이터(dev-ui-data.json, xtask가 세션 설정 fvoci.dev_ui_data로 넘긴다)로 "UI 상태 예제"
-- 워크스페이스를 한 번만 만든다.
-- 긴 한국어·공백 없는 문자열·이모지·숫자가 섞인 위키 문서 트리, 답글과 해결된 스레드가 있는 댓글, 상태·우선순위·
-- 담당자가 다른 업무, 다섯 종류의 모듈과 항목을 실제 앱에서도 같은 내용으로 본다. 기존 테스트 사용자만 조회하고 계정을
-- 만들거나 바꾸지 않는다. 뒤에 더한 내용은 아래의 두 번째 묶음('dev_ui_seed_v2')이 따로 한 번만 넣는다.
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
  v_org uuid;
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
    SELECT organization_id INTO STRICT v_org FROM organization_members
      WHERE user_id = v_admin ORDER BY organization_id LIMIT 1;
    IF NOT EXISTS (SELECT 1 FROM organization_members WHERE organization_id = v_org AND user_id = v_user) THEN
      v_inv := app_invite_organization(v_org, 'test-user-2@example.test', 'member', 7 * 24 * 3600, pg_temp.take(3));
      PERFORM set_config('app.user_id', v_user::text, true);
      PERFORM app_respond_invitation(v_inv, true, pg_temp.take(1));
      PERFORM set_config('app.user_id', v_admin::text, true);
    END IF;
    v_ws := app_create_workspace(pg_temp.take1(), v_org, v_data->>'workspace', pg_temp.take(3));
    v_inv := app_invite(v_ws, 'test-user-2@example.test', 'member', 7 * 24 * 3600, pg_temp.take(3));
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
        (v_docs->>(v_row->>'parent'))::uuid, 10, pg_temp.take(4));
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
    PERFORM app_set_module_description(v_module, v_data->'module'->>'description');
    v_item := app_create_item(v_module, pg_temp.take1(), v_data->'module'->>'item',
                              v_data->'module'->>'item_description', pg_temp.take(6));
    PERFORM app_set_item_members(v_item, ARRAY[v_user], ARRAY[]::uuid[],
                                 pg_temp.take(app_set_item_members_ids(v_item)));
    FOR v_row IN SELECT * FROM jsonb_array_elements(v_data->'tasks') LOOP
      SELECT coalesce(array_agg((v_people->>a)::uuid), ARRAY[]::uuid[]) INTO v_assignees
        FROM jsonb_array_elements_text(v_row->'assignees') AS a;
      -- id: 업무·감사·활동 + 담당자마다 감사·활동 2개. 자료의 상태 값은 Storybook과 같게
      -- (apps/web/stories/fixtures/synthetic.ts의 SEED_STATUSES) 기본 상태 정의의 이름으로 옮긴다(D-116).
      -- 모르는 값은 그대로 넘겨 app_create_task가 거절하게 한다.
      PERFORM app_create_task(v_item, pg_temp.take1(), v_row->>'title', NULL, v_row->>'description',
        CASE v_row->>'status' WHEN 'todo' THEN '할 일' WHEN 'in_progress' THEN '진행 중'
          WHEN 'done' THEN '완료' ELSE v_row->>'status' END,
        v_row->>'priority', v_today + (v_row->>'start')::integer,
        v_today + (v_row->>'due')::integer, (v_row->>'estimate')::integer,
        ARRAY(SELECT jsonb_array_elements_text(v_row->'tags')), v_assignees,
        (v_row->>'milestone')::boolean, pg_temp.take(3 + 2 * cardinality(v_assignees)));
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

-- 두 번째 묶음('dev_ui_seed_v2'): 프로젝트 모듈의 항목 몇 개와, 모듈 종류마다 하나씩(항목·항목 멤버) 그리고 긴 이름·공백
-- 없는 이름·이모지 이름의 빈 모듈을 더한다. v1 표시만 있는 기존 개발 DB에도 v1 워크스페이스에 한 번 더한다.
-- 그 워크스페이스나 프로젝트 모듈을 지웠으면 되살리지 않고, 이미 쓰인 키의 모듈은 건너뛴다(사용자 편집을 덮지 않는다).
DO $seed_v2$
DECLARE
  v_data jsonb := current_setting('fvoci.dev_ui_data')::jsonb;
  v_people jsonb;
  v_admin uuid;
  v_user uuid;
  v_ws uuid;
  v_project uuid;
  v_module uuid;
  v_item uuid;
  v_mod jsonb;
  v_row jsonb;
  v_members uuid[];
  v_version integer;
  v_outcome text;
  v_today date := current_date;
  v_modules integer := 0;
  v_items integer := 0;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('fvoci:dev_ui_seed_v2', 0));
  IF EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_ui_seed_v2') THEN
    RETURN;
  END IF;
  SELECT (value->>'workspace')::uuid INTO v_ws FROM instance_state WHERE key = 'dev_ui_seed_v1';
  IF v_ws IS NULL THEN
    RAISE EXCEPTION 'dev_ui_seed_v1 must run first';
  END IF;
  SELECT id INTO v_admin FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000a';
  SELECT id INTO v_user FROM users
    WHERE issuer = current_setting('fvoci.dev_issuer') AND subject = '00000000-0000-4000-8000-00000000000b';
  IF v_admin IS NULL OR v_user IS NULL THEN
    RAISE EXCEPTION 'existing dev test users are required for this issuer';
  END IF;
  v_people := jsonb_build_object('owner', v_admin, 'writer', v_user);

  BEGIN
    IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = v_ws AND deleted_at IS NULL) THEN
      INSERT INTO instance_state (key, value)
        VALUES ('dev_ui_seed_v2', jsonb_build_object('workspace', v_ws, 'skipped', 'workspace deleted'));
      RETURN;
    END IF;
    PERFORM set_config('app.user_id', v_admin::text, true);
    -- v1이 만든 프로젝트 모듈(지금 키가 그대로이고 지우지 않았을 때만).
    SELECT m.id INTO v_project FROM module_keys k JOIN modules m ON m.id = k.module_id
      WHERE k.workspace_id = v_ws AND k.key = v_data->'module'->>'key' AND k.retired_at IS NULL
        AND m.deleted_at IS NULL;

    FOR v_mod IN
      SELECT jsonb_build_object('module', v_project, 'items', v_data->'project_items')
        WHERE v_project IS NOT NULL
      UNION ALL
      SELECT m FROM jsonb_array_elements(v_data->'modules') AS m
    LOOP
      v_module := (v_mod->>'module')::uuid;
      IF v_module IS NULL THEN
        IF EXISTS (SELECT 1 FROM module_keys WHERE workspace_id = v_ws AND key = v_mod->>'key') THEN
          CONTINUE;
        END IF;
        v_module := app_create_module(v_ws, pg_temp.take1(), v_mod->>'kind', v_mod->>'name', v_mod->>'key',
                                      pg_temp.take(2));
        IF v_mod->>'description' <> '' THEN
          PERFORM app_set_module_description(v_module, v_mod->>'description');
        END IF;
        v_modules := v_modules + 1;
      END IF;
      -- 항목: 정보(상태·기간)는 처음 실행한 날 기준의 앞뒤 일수다. 새 항목은 비공개라 멤버로 연다.
      FOR v_row IN SELECT * FROM jsonb_array_elements(v_mod->'items') LOOP
        v_item := app_create_item(v_module, pg_temp.take1(), v_row->>'title', v_row->>'description',
                                  pg_temp.take(6));
        SELECT coalesce(array_agg((v_people->>p)::uuid), ARRAY[]::uuid[]) INTO v_members
          FROM jsonb_array_elements_text(v_row->'members') AS p;
        IF cardinality(v_members) > 0 THEN
          PERFORM app_set_item_members(v_item, v_members, ARRAY[]::uuid[],
                                       pg_temp.take(app_set_item_members_ids(v_item)));
        END IF;
        -- 정보 패널 저장은 읽은 판을 받는다(D-104). 멤버 정리도 판을 올릴 수 있어 바로 앞에서 읽는다.
        SELECT profile_version INTO STRICT v_version FROM items WHERE id = v_item;
        v_outcome := app_set_item_profile(v_item, v_version, v_admin, '', '',
          v_today + (v_row->>'start')::integer, v_today + (v_row->>'end')::integer, v_row->>'status',
          '{}'::jsonb, pg_temp.take(2));
        IF v_outcome <> 'updated' THEN
          RAISE EXCEPTION 'dev item profile was not saved: %', v_outcome;
        END IF;
        v_items := v_items + 1;
      END LOOP;
    END LOOP;

    PERFORM set_config('app.user_id', '', true);
    INSERT INTO instance_state (key, value) VALUES ('dev_ui_seed_v2', jsonb_build_object(
      'workspace', v_ws, 'project_module', v_project, 'modules', v_modules, 'items', v_items, 'seeded_on', v_today));
  EXCEPTION WHEN OTHERS THEN
    RAISE EXCEPTION 'dev UI examples (v2) not added: %', SQLERRM USING ERRCODE = SQLSTATE;
  END;
END
$seed_v2$;

SELECT key, value FROM instance_state WHERE key IN ('dev_ui_seed_v1', 'dev_ui_seed_v2') ORDER BY key;
