-- #240 / #245: compare every output column AND order, including clamped limits.
-- Run with the app role, against a reference cloned before candidate (b).
BEGIN;
SELECT set_config('perf240.owner', :'owner', true),
       set_config('perf240.reader', :'reader', true),
       set_config('perf240.recipient', :'recipient', true),
       set_config('perf240.organization', :'organization', true);
SET LOCAL ROLE fvoci_app;
DO $$
DECLARE
  who uuid; org uuid; lim integer; expected jsonb; actual jsonb; cases integer := 0;
BEGIN
  FOREACH who IN ARRAY ARRAY[current_setting('perf240.owner')::uuid,
                            current_setting('perf240.reader')::uuid,
                            current_setting('perf240.recipient')::uuid,
                            '00000000-0000-0000-0000-000000000001'::uuid, NULL::uuid]
  LOOP
    PERFORM set_config('app.user_id', coalesce(who::text,''), true);
    FOREACH org IN ARRAY ARRAY[current_setting('perf240.organization')::uuid, NULL::uuid,
                              '00000000-0000-0000-0000-000000000002'::uuid]
    LOOP
      FOREACH lim IN ARRAY ARRAY[NULL::integer,-1,0,1,20,100,101]
      LOOP
        SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY n), '[]') INTO expected
        FROM fvoci.perf240_recent_reference(org,lim) WITH ORDINALITY r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n);
        SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY n), '[]') INTO actual
        FROM fvoci.app_recent_activity(org,lim) WITH ORDINALITY r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n);
        IF actual IS DISTINCT FROM expected THEN
          RAISE EXCEPTION 'recent mismatch: actor %, organization %, limit %', who,org,lim;
        END IF;
        cases := cases + 1;
      END LOOP;
    END LOOP;
  END LOOP;
  RAISE NOTICE 'PASS recent activity: % ordered result comparisons', cases;
END $$;
ROLLBACK;
