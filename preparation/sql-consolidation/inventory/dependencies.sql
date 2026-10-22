-- 읽기 전용. 물리 OID를 결과에 남기지 않고 PostgreSQL 객체 식별자로 의존성을 기록한다.
-- SQL/PLpgSQL 문자열 본문의 호출은 pg_depend가 모두 기록하지 않으므로 function-map.json과 함께 읽는다.
BEGIN READ ONLY;
WITH dependencies AS (
  SELECT source.type AS source_type, source.schema AS source_schema, source.identity AS source,
         target.type AS target_type, target.schema AS target_schema, target.identity AS target,
         d.deptype AS dependency_type
  FROM pg_depend d
  CROSS JOIN LATERAL pg_identify_object(d.classid, d.objid, d.objsubid) source
  CROSS JOIN LATERAL pg_identify_object(d.refclassid, d.refobjid, d.refobjsubid) target
  WHERE source.schema = 'fvoci'
     OR (source.type IN ('constraint', 'trigger', 'policy', 'default value')
         AND source.identity LIKE '% on fvoci.%')
)
SELECT to_jsonb(d)
FROM dependencies d
ORDER BY source_type, source, dependency_type, target_type, target;
COMMIT;
