-- #240 / #245 A-stage experiment: decision required if the acceptance set differs.
-- Install only in the held measurement database; the product function is unchanged.
CREATE FUNCTION fvoci.perf240_timezone_candidate(p_tz text) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_tz IS NULL OR p_tz ~ '^(posix|right|SystemV)/' OR p_tz IN ('Factory', 'localtime', 'posixrules') THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
    BEGIN
      PERFORM timestamp '2000-01-01 00:00:00' AT TIME ZONE p_tz;
    EXCEPTION WHEN invalid_parameter_value OR invalid_time_zone_displacement_value THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END;
  END
  $$;
ALTER FUNCTION fvoci.perf240_timezone_candidate(text) OWNER TO fvoci_migrator;
REVOKE ALL ON FUNCTION fvoci.perf240_timezone_candidate(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.perf240_timezone_candidate(text) TO fvoci_app;
