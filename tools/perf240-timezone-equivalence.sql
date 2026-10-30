-- #240 / #245: full installed name set plus rejected-name families. A nonzero
-- difference is a decision classification, never a reason to weaken this test.
\if :{?candidate}
\else
\set candidate app_check_timezone
\endif
\pset format unaligned
\pset tuples_only on
SELECT set_config('perf240.candidate', :'candidate', false);
CREATE FUNCTION pg_temp.perf240_timezone_accepts(p_name text) RETURNS boolean
LANGUAGE plpgsql STABLE AS $$
BEGIN
  EXECUTE format('SELECT fvoci.%I($1)',current_setting('perf240.candidate')) USING p_name;
  RETURN true;
EXCEPTION WHEN invalid_parameter_value OR invalid_time_zone_displacement_value THEN
  RETURN false;
END $$;
CREATE TEMP TABLE perf240_names AS SELECT name FROM pg_timezone_names;
CREATE TEMP TABLE perf240_inputs AS
SELECT name FROM perf240_names
UNION SELECT lower(name) FROM perf240_names
UNION SELECT upper(name) FROM perf240_names
UNION SELECT 'posix/'||name FROM perf240_names
UNION SELECT 'right/'||name FROM perf240_names
UNION SELECT 'SystemV/'||name FROM perf240_names
UNION SELECT abbrev FROM pg_timezone_abbrevs
UNION SELECT unnest(ARRAY[NULL,'','Factory','factory','FACTORY','localtime','LOCALTIME',
  'posixrules','POSIXRULES','EST','est','PST','CST','UTC','utc','GMT','GMT+3','UTC+03',
  '+09','-09','+09:00','GMT+25','GMT-25','EST5EDT','xxx0','UTC0',':Asia/Seoul',':UTC',
  'Asia/Seoul ','Asia/Seoul','Not/AZone','POSIX/Asia/Seoul','RIGHT/Asia/Seoul',
  'systemv/EST5EDT','SystemV/EST5EDT','24:00','252:00']);
CREATE TEMP TABLE perf240_timezone_results AS
SELECT i.name,
  coalesce(i.name IS NOT NULL AND i.name !~ '^(posix|right|SystemV)/'
    AND i.name NOT IN ('Factory','localtime','posixrules')
    AND EXISTS(SELECT 1 FROM perf240_names n WHERE n.name=i.name),false) AS expected,
  pg_temp.perf240_timezone_accepts(i.name) AS actual
FROM perf240_inputs i;
SELECT jsonb_build_object('event','timezone.matrix',
  'inputs',count(*),'baseline_allowed',count(*) FILTER(WHERE expected),
  'candidate_allowed',count(*) FILTER(WHERE actual),
  'differences',count(*) FILTER(WHERE expected IS DISTINCT FROM actual),
  'canonical_lost',count(*) FILTER(WHERE expected AND NOT actual))
FROM perf240_timezone_results;
SELECT jsonb_build_object('event','timezone.examples','rows',jsonb_agg(to_jsonb(r) ORDER BY name))
FROM perf240_timezone_results r
WHERE name IS NULL OR name IN ('','Factory','factory','FACTORY','localtime','LOCALTIME',
  'posixrules','POSIXRULES','EST','est','PST','CST','UTC','utc','GMT','GMT+3','UTC+03',
  '+09','-09','+09:00','GMT+25','GMT-25','EST5EDT','xxx0','UTC0',':Asia/Seoul',':UTC',
  'Asia/Seoul ','Asia/Seoul','Not/AZone','POSIX/Asia/Seoul','RIGHT/Asia/Seoul',
  'systemv/EST5EDT','SystemV/EST5EDT','24:00','252:00');
DO $$
DECLARE differences bigint;
BEGIN
  SELECT count(*) INTO differences FROM perf240_timezone_results WHERE expected IS DISTINCT FROM actual;
  IF differences <> 0 THEN RAISE EXCEPTION 'time zone acceptance differs for % inputs: decision required',differences; END IF;
  RAISE NOTICE 'PASS identical timezone acceptance in installed-name and rejection matrix';
END $$;
