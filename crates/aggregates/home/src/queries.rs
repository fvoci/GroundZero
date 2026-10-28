//! 홈 집계의 읽기 질의. 응답·권한 의미는 추출 전과 같다.

use crate::dashboard::UP_NEXT_PER_BUCKET;
use fvoci_core::error::AppError;
use fvoci_core::ids::{ItemId, OrganizationId, WorkspaceId};
use fvoci_db::UserTx;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

/// Up Next 후보: 내가 담당한 업무 중 이번 주($1 + 7)까지 마감인 미완료(D-120).
/// 보관된 항목·모듈의 업무는 뺀다.
const UP_NEXT_MINE: &str = "SELECT t.id, t.item_id, t.workspace_id, t.title, s.name AS status, s.category AS status_category, t.due_date, t.number
     FROM tasks t JOIN task_statuses s ON s.id = t.status_id JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
     WHERE s.category <> 'done' AND t.due_date IS NOT NULL AND t.due_date <= $1 + 7
       AND i.archived_at IS NULL AND m.archived_at IS NULL
       AND t.workspace_id IN (SELECT app_my_workspace_ids($2))
       AND EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = app_user_id())";

pub(super) type TaskRow = (
    Uuid,
    ItemId,
    WorkspaceId,
    Option<String>,
    String,
    String,
    String,
    Option<Date>,
);

pub(super) type DocRow = (
    Uuid,
    String,
    Option<String>,
    Option<WorkspaceId>,
    Option<ItemId>,
    OffsetDateTime,
);
pub(super) type ItemRow = (
    ItemId,
    WorkspaceId,
    String,
    Option<String>,
    String,
    i64,
    i64,
    String,
);
pub(super) async fn today(tx: &mut UserTx, tz: Option<String>) -> Result<Date, AppError> {
    // 빈 `tz=`는 없는 것과 같다(검색과 같은 규칙): 저장된 시간대, 없으면 UTC.
    let tz = match tz.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(tz) => tz.to_owned(),
        None => sqlx::query_scalar(
            "SELECT coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC')",
        ).fetch_one(tx.conn()).await?,
    };
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(&tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let today: Date = sqlx::query_scalar("SELECT (now() AT TIME ZONE $1)::date")
        .bind(&tz)
        .fetch_one(tx.conn())
        .await?;

    Ok(today)
}

pub(super) async fn up_next(
    tx: &mut UserTx,
    today: Date,
    organization: OrganizationId,
) -> Result<Vec<TaskRow>, AppError> {
    let up_next: Vec<TaskRow> = sqlx::query_as(&format!(
        "WITH mine AS ({UP_NEXT_MINE}),
         ranked AS (
           SELECT mine.*, row_number() OVER (
                    PARTITION BY CASE WHEN due_date < $1 THEN 0 WHEN due_date = $1 THEN 1 ELSE 2 END
                    ORDER BY due_date, number, id) AS rn
           FROM mine
         )
         SELECT id, item_id, workspace_id, app_task_reference(id), title, status, status_category, due_date
         FROM ranked WHERE rn <= $3 ORDER BY due_date, number, id"
    ))
    .bind(today)
    .bind(organization)
    .bind(UP_NEXT_PER_BUCKET)
    .fetch_all(tx.conn())
    .await?;

    Ok(up_next)
}

pub(super) async fn counts(
    tx: &mut UserTx,
    today: Date,
    organization: OrganizationId,
) -> Result<(i64, i64, i64), AppError> {
    let (overdue, due_today, upcoming): (i64, i64, i64) = sqlx::query_as(&format!(
        "WITH mine AS ({UP_NEXT_MINE})
         SELECT count(*) FILTER (WHERE due_date < $1), count(*) FILTER (WHERE due_date = $1),
                count(*) FILTER (WHERE due_date > $1)
         FROM mine"
    ))
    .bind(today)
    .bind(organization)
    .fetch_one(tx.conn())
    .await?;

    Ok((overdue, due_today, upcoming))
}

pub(super) async fn recent_tasks(
    tx: &mut UserTx,
    organization: OrganizationId,
) -> Result<Vec<TaskRow>, AppError> {
    let recent_tasks: Vec<TaskRow> = sqlx::query_as(
        "SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id), t.title, s.name AS status, s.category AS status_category, t.due_date
         FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.updated_by = app_user_id() AND t.workspace_id IN (SELECT app_my_workspace_ids($1)) ORDER BY t.updated_at DESC, t.id LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;

    Ok(recent_tasks)
}

pub(super) async fn documents(
    tx: &mut UserTx,
    organization: OrganizationId,
) -> Result<Vec<DocRow>, AppError> {
    let recent_documents: Vec<DocRow> = sqlx::query_as(
        "SELECT d.id, d.title, app_document_reference(d.id), d.workspace_id, d.item_id, d.updated_at
         FROM documents d WHERE d.updated_by = app_user_id() AND (d.workspace_id IN (SELECT app_my_workspace_ids($1)) OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id())) ORDER BY d.updated_at DESC, d.id LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;

    Ok(recent_documents)
}

pub(super) async fn active_items(
    tx: &mut UserTx,
    organization: OrganizationId,
) -> Result<Vec<ItemRow>, AppError> {
    let active_items: Vec<ItemRow> = sqlx::query_as(
        "SELECT i.id, i.workspace_id, m.kind, app_item_reference(i.id), i.title, c.done, c.total,
                app_access('item', i.id)
         FROM items i JOIN modules m ON m.id = i.module_id
         CROSS JOIN LATERAL (
           SELECT count(*) FILTER (WHERE s.category = 'done') AS done, count(*) AS total, max(t.updated_at) AS active
           FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.item_id = i.id
         ) c
         WHERE i.status = 'active' AND i.archived_at IS NULL AND m.archived_at IS NULL
           AND i.workspace_id IN (SELECT app_my_workspace_ids($1))
         ORDER BY greatest(i.updated_at, c.active) DESC, i.id
         LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;

    Ok(active_items)
}
