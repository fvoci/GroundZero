-- 개발 전용 준비 SQL의 앞부분(`cargo xtask dev --oidc`가 dev-seed.sql·dev-module-seed.sql 앞에 붙인다).
-- 새 행의 id는 DB가 만들지 않는다(D-88): xtask가 UUIDv7 목록을 만들어 세션 설정 `fvoci.dev_ids`로 넘기고, 준비
-- SQL은 `pg_temp.take(n)`·`pg_temp.take1()`로 차례로 꺼내 앱 함수에 넘긴다. 모자라면 멈춘다.
CREATE TEMP TABLE dev_ids AS
  SELECT n::integer AS n, id FROM unnest(current_setting('fvoci.dev_ids')::uuid[]) WITH ORDINALITY AS t(id, n);

CREATE FUNCTION pg_temp.take(p_n integer) RETURNS uuid[]
  LANGUAGE plpgsql
  AS $$
  DECLARE
    v_ids uuid[];
  BEGIN
    WITH taken AS (
      DELETE FROM dev_ids WHERE n IN (SELECT n FROM dev_ids ORDER BY n LIMIT p_n) RETURNING n, id
    )
    SELECT coalesce(array_agg(id ORDER BY n), '{}') INTO v_ids FROM taken;
    IF cardinality(v_ids) < p_n THEN
      RAISE EXCEPTION 'cargo xtask dev passed too few ids for the dev seed';
    END IF;
    RETURN v_ids;
  END
  $$;

CREATE FUNCTION pg_temp.take1() RETURNS uuid
  LANGUAGE sql
  AS $$ SELECT (pg_temp.take(1))[1] $$;
