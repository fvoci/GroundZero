-- 개발 전용 준비(`cargo xtask dev --oidc`만 실행한다). 운영·셀프호스트 구성은 이 파일을 쓰지 않는다.
-- 테스트 realm(deploy/keycloak/realm-test.json)의 가짜 사용자 둘이 함께 쓰는 워크스페이스(실시간 공동 편집 켬)와
-- 공동 편집 문서 하나를 한 번만 만든다. 모두 앱과 같은 정의자 함수(번호·감사·권한 검사)를 거친다. instance_state의
-- 'dev_seed' 표시가 있으면 아무것도 바꾸지 않으므로 다시 실행해도 중복되거나 사용자 데이터가 초기화되지 않는다.
-- 입력: psql 변수 issuer(개발 Keycloak 발급자. 사용자 식별은 발급자 + subject).
SET search_path = fvoci, pg_temp;
SET fvoci.dev_issuer = :'issuer';

DO $seed$
DECLARE
  v_issuer text := current_setting('fvoci.dev_issuer');
  v_admin uuid;
  v_user uuid;
  v_ws uuid;
  v_inv uuid;
  v_doc uuid;
BEGIN
  IF EXISTS (SELECT 1 FROM instance_state WHERE key = 'dev_seed') THEN
    -- 처음 준비한 발급자와 다르면(개발 Keycloak 포트를 바꿈) 테스트 사용자가 새 계정으로 들어와 준비한 워크스페이스에
    -- 들어가지 못한다. 조용히 넘기지 않고 알린다(자동으로 지우거나 바꾸지 않는다).
    IF NOT EXISTS (SELECT 1 FROM users
                   WHERE issuer = v_issuer AND subject = '00000000-0000-4000-8000-00000000000a') THEN
      RAISE EXCEPTION 'the dev seed was made for another OIDC issuer than %; start with the first FVOCI_DEV_KEYCLOAK_PORT again, or reset with cargo xtask down',
        v_issuer;
    END IF;
    RETURN;
  END IF;
  -- 로그인 때와 같은 함수로 사용자를 만든다(발급자 + 고정 subject). 로그인하면 같은 행을 쓴다.
  v_admin := app_upsert_user(v_issuer, '00000000-0000-4000-8000-00000000000a',
                             'test-user@example.test', true, 'Test User');
  v_user := app_upsert_user(v_issuer, '00000000-0000-4000-8000-00000000000b',
                            'test-user-2@example.test', true, 'Second User');
  -- 초기 관리자(D-51) 경로 그대로: test-user가 첫 워크스페이스의 소유자. 이미 정해져 있으면 그 사람으로 새로 만든다.
  PERFORM set_config('app.user_id', '', true);
  v_ws := app_system_claim_initial_workspace(v_admin, '개발');
  PERFORM set_config('app.user_id', v_admin::text, true);
  IF v_ws IS NULL THEN
    v_ws := app_create_workspace(uuidv7(), '개발');
  END IF;
  PERFORM app_set_workspace_realtime(v_ws, true);
  v_inv := app_invite(v_ws, 'test-user-2@example.test', 'writer', 7 * 24 * 3600);
  v_doc := app_create_wiki_document(v_ws, uuidv7(), '공동 편집 연습', 1, jsonb_build_object(
    'type', 'doc',
    'content', jsonb_build_array(
      jsonb_build_object('type', 'paragraph', 'content', jsonb_build_array(
        jsonb_build_object('type', 'text', 'text', '두 창에서 같이 고쳐 보세요.'))),
      jsonb_build_object('type', 'taskList', 'content', jsonb_build_array(
        jsonb_build_object('type', 'taskItem', 'attrs', jsonb_build_object('checked', false),
          'content', jsonb_build_array(jsonb_build_object('type', 'paragraph', 'content',
            jsonb_build_array(jsonb_build_object('type', 'text', 'text', '첫 할 일')))))))
    )), NULL, 10);
  PERFORM set_config('app.user_id', v_user::text, true);
  PERFORM app_respond_invitation(v_inv, true);
  PERFORM set_config('app.user_id', '', true);
  INSERT INTO instance_state (key, value)
  VALUES ('dev_seed', jsonb_build_object('workspace', v_ws, 'document', v_doc));
END
$seed$;

SELECT value->>'workspace' AS workspace, value->>'document' AS document
FROM instance_state WHERE key = 'dev_seed';
