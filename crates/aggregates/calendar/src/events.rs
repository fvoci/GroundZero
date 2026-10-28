//! 달력에 필요한 일정 정의 조회와 응답 변환.

use crate::model::{Event, EventKind};
use fvoci_core::access::Grade as Access;
use fvoci_core::error::AppError;
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_core::recurrence::RecurrenceRule;
use fvoci_core::validate::format_date;
use fvoci_db::UserTx;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct EventRow {
    pub(super) id: Uuid,
    pub(super) item_id: ItemId,
    workspace_id: WorkspaceId,
    kind: String,
    title: String,
    description: String,
    all_day: bool,
    start_date: Option<Date>,
    end_date: Option<Date>,
    starts_at: Option<OffsetDateTime>,
    ends_at: Option<OffsetDateTime>,
    timezone: String,
    recurrence: Option<serde_json::Value>,
    excluded_dates: Vec<Date>,
    version: i32,
    access: String,
}

const EVENT_SELECT: &str =
    "SELECT e.id, e.item_id, e.workspace_id, e.kind, e.title, e.description, e.all_day,
        e.start_date, e.end_date, e.starts_at, e.ends_at, e.timezone, e.recurrence, e.excluded_dates, e.version,
        app_access('event', e.id) AS access
 FROM events e";

const EVENT_ORDER: &str =
    "coalesce(e.starts_at, e.start_date::timestamp AT TIME ZONE e.timezone), e.id";

pub(super) fn to_event(row: EventRow) -> Result<Event, AppError> {
    Ok(Event {
        id: row.id,
        item_id: row.item_id,
        workspace_id: row.workspace_id,
        kind: EventKind::parse(&row.kind)
            .ok_or_else(|| AppError::Config(format!("unknown event kind {}", row.kind)))?,
        title: row.title,
        description: row.description,
        all_day: row.all_day,
        start_date: format_date(row.start_date),
        end_date: format_date(row.end_date),
        starts_at: row.starts_at,
        ends_at: row.ends_at,
        timezone: row.timezone,
        recurrence: row.recurrence.map(RecurrenceRule::from_db).transpose()?,
        excluded_dates: row
            .excluded_dates
            .into_iter()
            .filter_map(|d| format_date(Some(d)))
            .collect(),
        version: row.version,
        access: Access::parse_present(&row.access)?,
    })
}

pub(super) async fn load(
    tx: &mut UserTx,
    occurrence_rows: &[crate::sources::OccurrenceRow],
) -> Result<Vec<EventRow>, AppError> {
    let mut event_ids: Vec<Uuid> = occurrence_rows.iter().map(|r| r.0).collect();
    event_ids.sort_unstable();
    event_ids.dedup();
    let event_rows: Vec<EventRow> = sqlx::query_as(&format!(
        "{EVENT_SELECT} WHERE e.id = ANY($1) AND e.deleted_at IS NULL ORDER BY {EVENT_ORDER}"
    ))
    .bind(&event_ids)
    .fetch_all(tx.conn())
    .await?;
    Ok(event_rows)
}
