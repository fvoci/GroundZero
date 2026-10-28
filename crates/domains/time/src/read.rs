use crate::{
    TimeState,
    model::{ENTRY_SELECT, EntryRow, TimeEntry, TimeKind, load, to_entry},
    range::{TimeRange, window},
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_http::session::CurrentUser;
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

/// 내 시간 기록(기간 안에 시작한 것, 최신순).
#[utoipa::path(get, path = "/api/me/time-entries", operation_id = "list_time_entries", params(TimeRange),
    responses((status = 200, body = [TimeEntry]), (status = 400, body = ErrorBody)), tag = "time")]
pub async fn list(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Query(range): Query<TimeRange>,
) -> Result<Json<Vec<TimeEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let w = window(&mut tx, &range).await?;
    let rows: Vec<EntryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE e.deleted_at IS NULL AND e.started_at >= $1 AND e.started_at < $2
         ORDER BY e.started_at DESC, e.id"
    ))
    .bind(w.from_at)
    .bind(w.to_at)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(to_entry)
        .collect::<Result<_, _>>()
        .map(Json)
}

/// 수정 이력 한 건(고치기 전 값).
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeEntryRevision {
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ended_at: Option<OffsetDateTime>,
    pub paused_seconds: i32,
    pub kind: TimeKind,
    pub memo: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub changed_at: OffsetDateTime,
}

/// 기록의 수정 이력(본인, 최신순).
#[utoipa::path(get, path = "/api/time-entries/{entry_id}/revisions", operation_id = "time_entry_revisions",
    params(("entry_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [TimeEntryRevision]), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn revisions(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<TimeEntryRevision>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    load(&mut tx, id).await?;
    type Row = (
        OffsetDateTime,
        Option<OffsetDateTime>,
        i32,
        String,
        String,
        OffsetDateTime,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT started_at, ended_at, paused_seconds, kind, memo, changed_at FROM time_entry_revisions
         WHERE entry_id = $1 ORDER BY changed_at DESC, id DESC LIMIT 100",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(
            |(started_at, ended_at, paused_seconds, kind, memo, changed_at)| {
                Ok(TimeEntryRevision {
                    started_at,
                    ended_at,
                    paused_seconds,
                    kind: TimeKind::parse(&kind)
                        .ok_or_else(|| AppError::Config(format!("unknown time kind {kind}")))?,
                    memo,
                    changed_at,
                })
            },
        )
        .collect::<Result<_, AppError>>()
        .map(Json)
}
