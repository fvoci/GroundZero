-- 표시 설정(D-81): 표시 언어·한 주 시작·시각 표시. 시간대(0043)와 같은 표에 사람마다 따로 둔다.
-- 빈 값은 "아직 고르지 않음"이고 화면이 브라우저·로케일 관례로 제안한다. 저장한 값만 계정에 남아 기기 사이에 같다.
-- 표시만 바꾸며 저장된 일정 시각·날짜는 그대로다.

ALTER TABLE user_settings ADD COLUMN locale text NOT NULL DEFAULT '' CHECK (locale IN ('', 'ko', 'en'));
ALTER TABLE user_settings ADD COLUMN week_start text NOT NULL DEFAULT '' CHECK (week_start IN ('', 'mon', 'sun'));
ALTER TABLE user_settings ADD COLUMN hour_cycle text NOT NULL DEFAULT '' CHECK (hour_cycle IN ('', 'h12', 'h23'));

-- 표시 설정 바꾸기. NULL인 인자는 그대로 둔다(예전 화면은 시간대만 보낸다).
CREATE FUNCTION app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_zone text := btrim(p_zone);
  BEGIN
    IF app_user_id() IS NULL THEN
      RAISE EXCEPTION 'sign in first' USING ERRCODE = '42501';
    END IF;
    IF v_zone IS NOT NULL AND v_zone <> '' AND NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = v_zone) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
    IF p_locale IS NOT NULL AND p_locale NOT IN ('', 'ko', 'en') THEN
      RAISE EXCEPTION 'unknown language' USING ERRCODE = '22023';
    END IF;
    IF p_week_start IS NOT NULL AND p_week_start NOT IN ('', 'mon', 'sun') THEN
      RAISE EXCEPTION 'unknown week start' USING ERRCODE = '22023';
    END IF;
    IF p_hour_cycle IS NOT NULL AND p_hour_cycle NOT IN ('', 'h12', 'h23') THEN
      RAISE EXCEPTION 'unknown hour cycle' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_settings (user_id) VALUES (app_user_id()) ON CONFLICT (user_id) DO NOTHING;
    UPDATE user_settings SET
      time_zone = coalesce(v_zone, time_zone),
      locale = coalesce(p_locale, locale),
      week_start = coalesce(p_week_start, week_start),
      hour_cycle = coalesce(p_hour_cycle, hour_cycle)
    WHERE user_id = app_user_id();
  END
  $$;
REVOKE ALL ON FUNCTION app_set_display_settings(text, text, text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_display_settings(text, text, text, text) TO fvoci_app;
