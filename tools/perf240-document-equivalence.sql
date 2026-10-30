-- #240 / #245: internal-grade equivalence across every fixture document and user.
-- The owner executes internal functions; the product list itself is also measured
-- separately as fvoci_app with its original documents_select RLS policy enabled.
BEGIN;
SET LOCAL ROLE fvoci_migrator;
SELECT set_config('perf240.owner', :'owner', true),
       set_config('perf240.reader', :'reader', true),
       set_config('perf240.recipient', :'recipient', true);
DO $$
DECLARE mismatches bigint;
BEGIN
  SELECT count(*) INTO mismatches
  FROM fvoci.documents d CROSS JOIN unnest(ARRAY[
    current_setting('perf240.owner')::uuid,current_setting('perf240.reader')::uuid,
    current_setting('perf240.recipient')::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,NULL::uuid]) u(id)
  WHERE fvoci.perf240_document_reference(d.id,d.workspace_id,d.item_id,d.owner_user_id,u.id)
        IS DISTINCT FROM fvoci.app_grade_document_at(d.id,d.workspace_id,d.item_id,d.owner_user_id,u.id);
  IF mismatches <> 0 THEN RAISE EXCEPTION 'document grade mismatches: %', mismatches; END IF;
  RAISE NOTICE 'PASS document grades: all fixture documents x five users';
END $$;
ROLLBACK;
