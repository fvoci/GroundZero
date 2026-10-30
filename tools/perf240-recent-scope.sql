-- #240 / #245: a user with zero eligible workspaces must not evaluate target
-- access for unrelated activity. This checks work performed, not a latency target.
SELECT pg_stat_statements_reset();
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'recipient', true);
SELECT count(*) FROM fvoci.app_recent_activity(:'organization'::uuid,20);
ROLLBACK;
DO $$
DECLARE access_calls bigint;
BEGIN
  SELECT coalesce(sum(calls),0) INTO access_calls FROM pg_stat_statements
  WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
    AND query LIKE '%SELECT app_access_as(p_kind, p_id, app_user_id())%';
  IF access_calls <> 0 THEN
    RAISE EXCEPTION 'scope-first evaluation: expected zero target access calls, got %',access_calls;
  END IF;
  RAISE NOTICE 'PASS zero-workspace activity: zero target access calls';
END $$;
