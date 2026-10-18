-- 반복 일정의 DST 처리(D-81, 사용자 선택). 반복 회차는 일정 자체의 시간대와 현지 시각으로 만든다.
-- 그 시간대에 없는 현지 시각(봄에 건너뛰는 시각)의 회차는 만들지 않고 "건너뜀"으로 돌려준다(RFC 5545 §3.3.10:
-- 반복 규칙은 존재하지 않는 시각을 무시). 두 번 나타나는 시각(가을에 겹치는 시각)은 앞선 시각에 한 번만 둔다.
-- 지금까지는 PostgreSQL의 기본 해석(없는 시각은 뒤로 밀기, 겹친 시각은 표준시)을 썼다.

-- 그 시간대의 현지 시각을 실제 시점으로. 두 번 있으면 앞선 시점, 없으면 NULL.
CREATE FUNCTION app_local_instant(p_local timestamp, p_zone text) RETURNS timestamptz
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT min(c) FROM (
      SELECT (p_local - ((t AT TIME ZONE p_zone) - (t AT TIME ZONE 'UTC'))) AT TIME ZONE 'UTC' AS c
      FROM (VALUES ((p_local - interval '1 day') AT TIME ZONE 'UTC'),
                   ((p_local + interval '1 day') AT TIME ZONE 'UTC')) AS around (t)
    ) candidates
    WHERE (c AT TIME ZONE p_zone) = p_local
  $$;

DROP FUNCTION app_event_occurrences(uuid, timestamptz, timestamptz, date, date);
CREATE FUNCTION app_event_occurrences(p_workspace uuid, p_from timestamptz, p_to timestamptz,
                                      p_from_date date, p_to_date date)
  RETURNS TABLE (event_id uuid, occurrence date, start_date date, end_date date,
                 starts_at timestamptz, ends_at timestamptz, skipped boolean)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH visible AS (
      SELECT e.*, coalesce(e.start_date, (e.starts_at AT TIME ZONE e.timezone)::date) AS anchor
      FROM events e
      WHERE e.workspace_id = p_workspace AND e.deleted_at IS NULL AND app_event_access(e.id) IS NOT NULL
    ),
    shaped AS (
      SELECT v.id, v.anchor AS occ, v.start_date AS sd, v.end_date AS ed, v.starts_at AS sa, v.ends_at AS ea,
             false AS skipped
      FROM visible v WHERE v.recurrence IS NULL
      UNION ALL
      SELECT v.id, o.occurrence,
             CASE WHEN v.all_day THEN v.start_date + (o.occurrence - v.anchor) END,
             CASE WHEN v.all_day THEN v.end_date + (o.occurrence - v.anchor) END,
             l.at,
             l.at + (v.ends_at - v.starts_at),
             NOT v.all_day AND l.at IS NULL
      FROM visible v
      CROSS JOIN LATERAL app_recurrence_dates(v.recurrence, v.anchor, p_from_date - 400, p_to_date + 2) o
      CROSS JOIN LATERAL (
        SELECT CASE WHEN NOT v.all_day
                    THEN app_local_instant(o.occurrence + (v.starts_at AT TIME ZONE v.timezone)::time, v.timezone)
               END AS at
      ) l
      WHERE v.recurrence IS NOT NULL AND NOT (o.occurrence = ANY (v.excluded_dates))
    )
    SELECT id, occ, sd, ed, sa, ea, skipped FROM shaped
    WHERE (sd IS NOT NULL AND sd <= p_to_date AND ed >= p_from_date)
       OR (sa IS NOT NULL AND sa < p_to AND (ea > p_from OR sa >= p_from))
       OR (skipped AND occ BETWEEN p_from_date AND p_to_date)
    ORDER BY coalesce(sa, sd::timestamp AT TIME ZONE 'UTC', occ::timestamp AT TIME ZONE 'UTC'), id
  $$;
REVOKE ALL ON FUNCTION app_event_occurrences(uuid, timestamptz, timestamptz, date, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_event_occurrences(uuid, timestamptz, timestamptz, date, date) TO fvoci_app;
