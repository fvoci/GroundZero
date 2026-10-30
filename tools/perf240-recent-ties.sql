-- Same-timestamp activity and a limit cutting through that tie, via product SQL.
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'owner', true),
       set_config('perf240.owner', :'owner', true),
       set_config('perf240.reader', :'reader', true),
       set_config('perf240.organization', :'organization', true);
SELECT fvoci.app_create_wiki_document(:'workspace'::uuid,:'newid_1'::uuid,'tie one',1,
  '{"type":"doc","content":[]}',NULL,20,ARRAY[:'newid_2'::uuid,:'newid_3'::uuid,:'newid_4'::uuid]) AS tie_one \gset
SELECT fvoci.app_create_wiki_document(:'workspace'::uuid,:'newid_5'::uuid,'tie two',1,
  '{"type":"doc","content":[]}',NULL,20,ARRAY[:'newid_6'::uuid,:'newid_7'::uuid,:'newid_8'::uuid]) AS tie_two \gset
SELECT set_config('perf240.tie_one', :'tie_one', true),set_config('perf240.tie_two', :'tie_two', true);
DO $$
DECLARE who uuid; lim integer; expected jsonb; actual jsonb; times integer; events integer;
BEGIN
  SELECT count(*),count(DISTINCT a.created_at) INTO events,times FROM fvoci.app_recent_activity(
    current_setting('perf240.organization')::uuid,20) a
  WHERE a.target_id IN (current_setting('perf240.tie_one')::uuid,current_setting('perf240.tie_two')::uuid);
  IF events <> 2 OR times <> 1 THEN RAISE EXCEPTION 'fixture must exercise two identical timestamps'; END IF;
  FOREACH who IN ARRAY ARRAY[current_setting('perf240.owner')::uuid,current_setting('perf240.reader')::uuid]
  LOOP
    PERFORM set_config('app.user_id',who::text,true);
    FOREACH lim IN ARRAY ARRAY[1,2,20] LOOP
      SELECT jsonb_agg(to_jsonb(r) ORDER BY n) INTO expected
      FROM fvoci.perf240_recent_reference(current_setting('perf240.organization')::uuid,lim) WITH ORDINALITY r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n);
      SELECT jsonb_agg(to_jsonb(r) ORDER BY n) INTO actual
      FROM fvoci.app_recent_activity(current_setting('perf240.organization')::uuid,lim) WITH ORDINALITY r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n);
      IF actual IS DISTINCT FROM expected THEN RAISE EXCEPTION 'tie/order mismatch'; END IF;
    END LOOP;
  END LOOP;
  RAISE NOTICE 'PASS tied timestamps and limits 1,2,20 for owner and readonly';
END $$;
ROLLBACK;
