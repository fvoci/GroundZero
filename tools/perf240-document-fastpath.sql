-- Run after perf240-document-plan.sql resets statistics and reads the owner list.
-- This checks elimination of a redundant branch, not a latency target.
DO $$
DECLARE share_calls bigint;
BEGIN
  SELECT coalesce(sum(calls),0) INTO share_calls FROM pg_stat_statements
  WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
    AND query LIKE '%SELECT s.permission%FROM document_shares s%';
  IF share_calls <> 0 THEN
    RAISE EXCEPTION 'owner-grade short circuit: expected zero share reads, got %',share_calls;
  END IF;
  RAISE NOTICE 'PASS owner list: no redundant share lookup';
END $$;
