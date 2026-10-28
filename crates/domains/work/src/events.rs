//! 회의·발표·행사 일정 자원과 명령(D-31·D-50·D-57).
//!
//! 일정은 항목에 속하고 권한은 SQL `app_access`를 따른다. 시각이 있는 일정은 UTC 시각과 일정 시간대를 함께 저장하고,
//! 종일 일정은 날짜 그대로 둔다(D-50). 반복 규칙 명령은 `event_recurrence`에 있고, 달력 집계는 별도 조각이 맡는다.
//! DB 정의는 `crates/db/migrations/202610280005_work.sql`에 있다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime, UtcOffset};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::WorkState;
use crate::recurrence::RecurrenceRule;
use fvoci_core::access::Grade;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_core::validate::{format_date, parse_date, validate_text};
use fvoci_db::{UserTx, is_unique_violation, new_ids};
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::CurrentUser;

const MAX_DESCRIPTION_CHARS: usize = 10_000;
/// 일정 하나의 최대 길이(일).
const MAX_EVENT_DAYS: i64 = 366;

/// 일정 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Meeting,
    Presentation,
    Event,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Meeting => "meeting",
            Self::Presentation => "presentation",
            Self::Event => "event",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "meeting" => Some(Self::Meeting),
            "presentation" => Some(Self::Presentation),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

/// 일정.
#[derive(Debug, Serialize, ToSchema)]
pub struct Event {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub workspace_id: WorkspaceId,
    pub kind: EventKind,
    pub title: String,
    pub description: String,
    /// 종일이면 `start_date`·`end_date`(끝 포함), 아니면 `starts_at`·`ends_at`.
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub starts_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// 일정에 지정한 시간대(IANA). 반복 계산의 기준이다.
    pub timezone: String,
    /// 반복 규칙(없으면 한 번). 회차는 달력에서 펼친다.
    pub recurrence: Option<RecurrenceRule>,
    /// 뺀 회차(일정 시간대 기준 날짜).
    pub excluded_dates: Vec<String>,
    pub version: i32,
    pub access: Grade,
}

/// 일정 내용(만들기·고치기 공통).
#[derive(Debug, Deserialize, ToSchema)]
pub struct EventInput {
    pub kind: EventKind,
    pub title: String,
    #[schema(max_length = 10_000)]
    pub description: String,
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// RFC 3339 시각(오프셋 포함).
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    #[schema(min_length = 1, max_length = 64)]
    pub timezone: String,
}

/// 일정 만들기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEvent {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    #[serde(flatten)]
    pub event: EventInput,
}

/// 일정 고치기. `base_version`이 지났으면 409와 서버 최신본.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEvent {
    pub base_version: i32,
    #[serde(flatten)]
    pub event: EventInput,
}

/// 일정 판 충돌은 최신 자원과 다른 칸, 보관·계정 충돌은 공통 오류다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum EventWriteConflict {
    Stale(Box<StaleWrite<Event>>),
    State(ErrorBody),
}

#[derive(Debug, Clone, Copy)]
enum When {
    AllDay {
        start: Date,
        end: Date,
    },
    Timed {
        start: OffsetDateTime,
        end: OffsetDateTime,
    },
}

impl When {
    fn start_date(self) -> Option<Date> {
        match self {
            Self::AllDay { start, .. } => Some(start),
            Self::Timed { .. } => None,
        }
    }
    fn end_date(self) -> Option<Date> {
        match self {
            Self::AllDay { end, .. } => Some(end),
            Self::Timed { .. } => None,
        }
    }
    fn starts_at(self) -> Option<OffsetDateTime> {
        match self {
            Self::Timed { start, .. } => Some(start),
            Self::AllDay { .. } => None,
        }
    }
    fn ends_at(self) -> Option<OffsetDateTime> {
        match self {
            Self::Timed { end, .. } => Some(end),
            Self::AllDay { .. } => None,
        }
    }
}

struct Valid {
    title: String,
    description: String,
    when: When,
    timezone: String,
}

fn parse_instant(raw: Option<&str>, field: &str) -> Result<OffsetDateTime, AppError> {
    let raw = raw.ok_or_else(|| {
        AppError::invalid(field, InvalidReason::Required, None, "a time is required")
    })?;
    OffsetDateTime::parse(raw.trim(), &Rfc3339)
        .map(|instant| instant.to_offset(UtcOffset::UTC))
        .map_err(|_| {
            AppError::invalid(
                field,
                InvalidReason::InvalidFormat,
                None,
                "use an RFC 3339 time with an offset",
            )
        })
}

fn validate(state: &WorkState, input: &EventInput) -> Result<Valid, AppError> {
    let title = validate_title(&input.title, state.max_title_chars, "title")?;
    let description = validate_text(&input.description, MAX_DESCRIPTION_CHARS, "description")?;
    let timezone = input.timezone.trim().to_owned();
    if timezone.is_empty() {
        return Err(AppError::invalid(
            "timezone",
            InvalidReason::Required,
            None,
            "timezone is required",
        ));
    }
    if timezone.len() > 64 {
        return Err(AppError::invalid(
            "timezone",
            InvalidReason::TooLong,
            Some(64),
            "timezone must be an IANA name",
        ));
    }
    let when = if input.all_day {
        all_day(input)?
    } else {
        timed(input)?
    };
    Ok(Valid {
        title,
        description,
        when,
        timezone,
    })
}

fn all_day(input: &EventInput) -> Result<When, AppError> {
    for (field, value) in [("starts_at", &input.starts_at), ("ends_at", &input.ends_at)] {
        if value.is_some() {
            return Err(AppError::invalid(
                field,
                InvalidReason::InvalidFormat,
                None,
                "an all-day event has dates, not times",
            ));
        }
    }
    let start = parse_date(input.start_date.as_deref(), "start_date")?.ok_or_else(|| {
        AppError::invalid(
            "start_date",
            InvalidReason::Required,
            None,
            "start_date is required",
        )
    })?;
    let end = parse_date(input.end_date.as_deref(), "end_date")?.unwrap_or(start);
    if end < start || (end - start).whole_days() >= MAX_EVENT_DAYS {
        return Err(AppError::invalid(
            "end_date",
            InvalidReason::OutOfRange,
            Some(MAX_EVENT_DAYS.unsigned_abs()),
            "the end date must follow the start within 366 days",
        ));
    }
    Ok(When::AllDay { start, end })
}

fn timed(input: &EventInput) -> Result<When, AppError> {
    for (field, value) in [
        ("start_date", &input.start_date),
        ("end_date", &input.end_date),
    ] {
        if value.is_some() {
            return Err(AppError::invalid(
                field,
                InvalidReason::InvalidFormat,
                None,
                "a timed event has times, not dates",
            ));
        }
    }
    let start = parse_instant(input.starts_at.as_deref(), "starts_at")?;
    let end = parse_instant(input.ends_at.as_deref(), "ends_at")?;
    if end < start || end - start > Duration::days(MAX_EVENT_DAYS) {
        return Err(AppError::invalid(
            "ends_at",
            InvalidReason::OutOfRange,
            Some(MAX_EVENT_DAYS.unsigned_abs()),
            "the end time must follow the start within 366 days",
        ));
    }
    Ok(When::Timed { start, end })
}

/// DB에서 돌려주는 UTC 표기와 맞춰 같은 시각의 오프셋 차이를 바뀐 칸으로 세지 않는다.
fn sent_fields(input: &EventInput, valid: &Valid) -> Result<serde_json::Value, AppError> {
    let instant = |value: Option<OffsetDateTime>| {
        value
            .map(|value| {
                value
                    .format(&Rfc3339)
                    .map_err(|e| AppError::Config(format!("format event time: {e}")))
            })
            .transpose()
    };
    Ok(serde_json::json!({
        "kind": input.kind, "title": valid.title, "description": valid.description,
        "all_day": input.all_day, "start_date": format_date(valid.when.start_date()),
        "end_date": format_date(valid.when.end_date()), "starts_at": instant(valid.when.starts_at())?,
        "ends_at": instant(valid.when.ends_at())?, "timezone": valid.timezone
    }))
}

fn event_error(error: sqlx::Error) -> AppError {
    if let Some(db) = error.as_database_error()
        && db.code().as_deref() == Some("22023")
        && db.message() == "unknown time zone"
    {
        return AppError::invalid(
            "timezone",
            InvalidReason::InvalidFormat,
            None,
            "unknown time zone",
        );
    }
    AppError::from_db_function(error)
}

#[derive(Debug, sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    item_id: ItemId,
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

fn to_event(row: EventRow) -> Result<Event, AppError> {
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
        access: Grade::parse_present(&row.access)?,
    })
}

pub(crate) async fn load(tx: &mut UserTx, id: Uuid) -> Result<Event, AppError> {
    let row: Option<EventRow> = sqlx::query_as(&format!(
        "{EVENT_SELECT} WHERE e.id = $1 AND e.deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    to_event(row.ok_or(AppError::NotFound)?)
}

/// 항목의 일정(시작 순). 항목을 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/events", operation_id = "list_item_events",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Event]), (status = 404, body = ErrorBody)), tag = "events")]
pub async fn list(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Event>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let rows: Vec<EventRow> = sqlx::query_as(&format!(
        "{EVENT_SELECT} WHERE e.item_id = $1 AND e.deleted_at IS NULL ORDER BY {EVENT_ORDER}"
    ))
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(to_event)
        .collect::<Result<_, _>>()
        .map(Json)
}

/// 일정 하나. 볼 수 없거나 지웠으면 404.
#[utoipa::path(get, path = "/api/events/{event_id}", operation_id = "get_event",
    params(("event_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Event), (status = 404, body = ErrorBody)), tag = "events")]
pub async fn get(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let event = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(event))
}

/// 일정 만들기(항목 쓰기 권한). 같은 `request_id`로 재시도하면 같은 일정을 돌려준다.
#[utoipa::path(post, path = "/api/items/{item_id}/events", operation_id = "create_event",
    params(("item_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "있으면 세션 사용자와 비교한다(D-96).")), request_body = CreateEvent,
    responses((status = 201, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "events")]
pub async fn create(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateEvent>,
) -> Result<(StatusCode, Json<Event>), AppError> {
    let valid = validate(&state, &input.event)?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: Uuid = sqlx::query_scalar(
            "SELECT app_create_event($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(item)
        .bind(input.request_id)
        .bind(input.event.kind.as_str())
        .bind(&valid.title)
        .bind(&valid.description)
        .bind(matches!(valid.when, When::AllDay { .. }))
        .bind(valid.when.start_date())
        .bind(valid.when.end_date())
        .bind(valid.when.starts_at())
        .bind(valid.when.ends_at())
        .bind(&valid.timezone)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(event_error)?;
        let event = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok(event)
    };
    match run().await {
        Ok(event) => Ok((StatusCode::CREATED, Json(event))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 일정 고치기(쓰기 권한). 기준 버전이 지났으면 409와 서버 최신본.
#[utoipa::path(put, path = "/api/events/{event_id}", operation_id = "update_event",
    params(("event_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "있으면 세션 사용자와 비교한다(D-96).")), request_body = UpdateEvent,
    responses((status = 200, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = EventWriteConflict)), tag = "events")]
pub async fn update(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEvent>,
) -> Result<Response, AppError> {
    let valid = validate(&state, &input.event)?;
    let mut tx = state.db.begin_as(user).await?;
    let sent = sent_fields(&input.event, &valid)?;
    let outcome: String = sqlx::query_scalar(
        "SELECT app_update_event($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(input.base_version)
    .bind(input.event.kind.as_str())
    .bind(&valid.title)
    .bind(&valid.description)
    .bind(matches!(valid.when, When::AllDay { .. }))
    .bind(valid.when.start_date())
    .bind(valid.when.end_date())
    .bind(valid.when.starts_at())
    .bind(valid.when.ends_at())
    .bind(&valid.timezone)
    .bind(new_ids(2))
    .fetch_one(tx.conn())
    .await
    .map_err(event_error)?;
    let event = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        StaleWrite::new("conflict", "the event changed", event, &sent).into_response()
    } else {
        Json(event).into_response()
    })
}

/// 일정 지우기(쓰기 권한). 행은 남기고 목록·달력에서 뺀다.
#[utoipa::path(delete, path = "/api/events/{event_id}", operation_id = "delete_event",
    params(("event_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "있으면 세션 사용자와 비교한다(D-96).")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "events")]
pub async fn delete(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_event($1, $2)")
        .bind(id)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(event_error)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
