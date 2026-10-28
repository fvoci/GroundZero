//! 달력 출처별 읽기. 행 접근과 개인 진도는 DB 역할의 RLS를 따른다.

use fvoci_core::error::AppError;
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_db::UserTx;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

pub(super) type TaskRow = (
    Uuid,
    ItemId,
    Option<String>,
    String,
    String,
    String,
    bool,
    Option<Date>,
    Option<Date>,
);

pub(super) type ProjectedRow = (
    Uuid,
    Uuid,
    ItemId,
    String,
    bool,
    Date,
    Option<Date>,
    Option<Date>,
);

pub(super) type OccurrenceRow = (
    Uuid,
    Date,
    Option<Date>,
    Option<Date>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    bool,
);

pub(super) type DateRow = (String, Uuid, ItemId, String, Date, bool, bool);

pub(super) async fn tasks(
    tx: &mut UserTx,
    ws: WorkspaceId,
    from: Date,
    to: Date,
) -> Result<Vec<TaskRow>, AppError> {
    sqlx::query_as(
        "SELECT t.id, t.item_id, app_task_reference(t.id), t.title, s.name AS status, s.category AS status_category, t.milestone, t.start_date, t.due_date
         FROM tasks t JOIN task_statuses s ON s.id = t.status_id
         WHERE t.workspace_id = $1
           AND coalesce(t.start_date, t.due_date) <= $3 AND coalesce(t.due_date, t.start_date) >= $2
         ORDER BY coalesce(t.start_date, t.due_date), t.number",
    )
    .bind(ws)
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await.map_err(AppError::from)
}

pub(super) async fn projected(
    tx: &mut UserTx,
    ws: WorkspaceId,
    from: Date,
    to: Date,
) -> Result<Vec<ProjectedRow>, AppError> {
    sqlx::query_as(
        "SELECT series_id, root_task_id, item_id, title, milestone, occurrence, start_date, due_date
         FROM app_projected_occurrences($1, NULL, $2, $3) ORDER BY occurrence, series_id",
    )
    .bind(ws)
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await.map_err(AppError::from)
}

pub(super) async fn occurrences(
    tx: &mut UserTx,
    ws: WorkspaceId,
    from: Date,
    to: Date,
    from_at: OffsetDateTime,
    to_at: OffsetDateTime,
) -> Result<Vec<OccurrenceRow>, AppError> {
    sqlx::query_as("SELECT * FROM app_event_occurrences($1, $2, $3, $4, $5)")
        .bind(ws)
        .bind(from_at)
        .bind(to_at)
        .bind(from)
        .bind(to)
        .fetch_all(tx.conn())
        .await
        .map_err(AppError::from)
}

pub(super) async fn dates(
    tx: &mut UserTx,
    ws: WorkspaceId,
    from: Date,
    to: Date,
) -> Result<Vec<DateRow>, AppError> {
    // 강의·스터디 계획일은 내 계획일이 있으면 그것(본인 진도 행만 읽힌다), 없으면 공통 계획일. 투고·산출물은 기록의
    // 날짜. 모두 그 항목을 볼 수 있을 때만 나온다(RLS·app_access).
    sqlx::query_as(
        "SELECT 'episode', e.id, e.item_id, e.number::text || '. ' || e.title,
                coalesce(p.planned_on, e.planned_on), p.planned_on IS NOT NULL, coalesce(p.status = 'done', false)
         FROM course_episodes e
         LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
         WHERE e.workspace_id = $1 AND coalesce(p.planned_on, e.planned_on) BETWEEN $2 AND $3
           AND app_access('item', e.item_id) <> 'none'
         UNION ALL
         SELECT 'study_unit', u.id, u.item_id, u.number::text || '. ' || u.title,
                coalesce(p.planned_on, u.planned_on), p.planned_on IS NOT NULL, coalesce(p.status = 'done', false)
         FROM study_units u
         LEFT JOIN unit_progress p ON p.unit_id = u.id AND p.user_id = app_user_id()
         WHERE u.workspace_id = $1 AND coalesce(p.planned_on, u.planned_on) BETWEEN $2 AND $3
           AND app_access('item', u.item_id) <> 'none'
         UNION ALL
         SELECT 'submission', r.id, r.item_id, r.title, r.occurred_on, false,
                coalesce(r.fields->>'status' IN ('published', 'rejected'), false)
         FROM item_records r
         WHERE r.workspace_id = $1 AND r.kind = 'submission' AND r.occurred_on BETWEEN $2 AND $3
           AND app_access('item', r.item_id) <> 'none'
         UNION ALL
         SELECT 'revision_due', r.id, r.item_id, r.title, (r.fields->>'revision_due')::date, false,
                coalesce(r.fields->>'status' IN ('accepted', 'published', 'rejected'), false)
         FROM item_records r
         WHERE r.workspace_id = $1 AND r.kind = 'submission'
           AND r.fields->>'revision_due' ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'
           AND (r.fields->>'revision_due')::date BETWEEN $2 AND $3
           AND app_access('item', r.item_id) <> 'none'
         UNION ALL
         SELECT 'deliverable', r.id, r.item_id, r.title, r.occurred_on, false,
                coalesce(r.fields->>'status' IN ('done', 'accepted'), false)
         FROM item_records r
         WHERE r.workspace_id = $1 AND r.kind = 'deliverable' AND r.occurred_on BETWEEN $2 AND $3
           AND app_access('item', r.item_id) <> 'none'
         ORDER BY 5, 1, 4",
    )
    .bind(ws)
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await.map_err(AppError::from)
}
