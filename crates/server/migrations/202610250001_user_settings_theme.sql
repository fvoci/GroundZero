-- 화면 밝기(D-109): 시스템 설정 따르기·밝게·어둡게를 다른 표시 설정(D-50·D-81)처럼 계정에 둔다.
-- 기존 사용자는 'system'이라 운영체제 설정을 그대로 따른다. 설정 함수는 인자가 하나 늘어 새 서명으로 바꾼다.

ALTER TABLE fvoci.user_settings
    ADD COLUMN theme text DEFAULT 'system'::text NOT NULL,
    ADD CONSTRAINT user_settings_theme_check CHECK ((theme = ANY (ARRAY['system'::text, 'light'::text, 'dark'::text])));

DROP FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text);

CREATE FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    -- 다른 칸과 달리 "고르지 않음"(빈 값)이 없다. 기본이 시스템 설정 따르기다.
    IF p_theme IS NOT NULL AND p_theme NOT IN ('system', 'light', 'dark') THEN
      RAISE EXCEPTION 'unknown theme' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_settings (user_id) VALUES (app_user_id()) ON CONFLICT (user_id) DO NOTHING;
    UPDATE user_settings SET
      time_zone = coalesce(v_zone, time_zone),
      locale = coalesce(p_locale, locale),
      week_start = coalesce(p_week_start, week_start),
      hour_cycle = coalesce(p_hour_cycle, hour_cycle),
      theme = coalesce(p_theme, theme)
    WHERE user_id = app_user_id();
  END
  $$;

ALTER FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) OWNER TO fvoci_migrator;

REVOKE ALL ON FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) TO fvoci_app;
