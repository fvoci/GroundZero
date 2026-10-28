//! 회의·발표·행사 일정과 워크스페이스 달력 (D-19, D-31, D-50, D-57).
//!
//! 일정은 항목에 속하고 권한은 항목을 따른다(DB 함수 `app_access`). 시각이 있는 일정은 UTC 시각과 일정의
//! 시간대를 함께 저장하고, 종일 일정은 날짜 그대로 둔다(D-50). 달력은 워크스페이스 단위의 기간 조회로, 볼 수 있는
//! 업무(시작·마감)와 일정을 함께 돌려준다(홈의 주간 달력도 같은 API를 쓴다).
//! DB 정의는 `crates/db/migrations/202610280002_organizations.sql`의 `app_access`,
//! `crates/db/migrations/202610280005_work.sql`의 `app_event_occurrences` 등에 있다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::documents::Access;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_core::validate::{format_date, parse_date, validate_text};
use fvoci_db::{UserTx, is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;
use fvoci_work::recurrence::RecurrenceRule;

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
    pub access: Access,
}

/// 일정 내용(만들기·고치기 공통).
#[derive(Debug, Deserialize, ToSchema)]
pub struct EventInput {
    pub kind: EventKind,
    pub title: String,
    pub description: String,
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// RFC 3339 시각(오프셋 포함).
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
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

/// 409 응답: 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventConflict {
    pub error: &'static str,
    pub current: Event,
}

struct When {
    start_date: Option<Date>,
    end_date: Option<Date>,
    starts_at: Option<OffsetDateTime>,
    ends_at: Option<OffsetDateTime>,
}

struct Valid {
    title: String,
    description: String,
    when: When,
    timezone: String,
}

fn parse_instant(raw: Option<&str>, field: &str) -> Result<OffsetDateTime, AppError> {
    raw.and_then(|s| OffsetDateTime::parse(s.trim(), &Rfc3339).ok())
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "{field} must be a time like 2026-10-17T09:00:00+09:00"
            ))
        })
}

fn validate(state: &AppState, input: &EventInput) -> Result<Valid, AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let description = validate_text(&input.description, MAX_DESCRIPTION_CHARS, "description")?;
    let timezone = input.timezone.trim().to_owned();
    if timezone.is_empty() || timezone.len() > 64 {
        return Err(AppError::BadRequest("timezone must be an IANA name".into()));
    }
    let too_long = || AppError::BadRequest(format!("an event lasts at most {MAX_EVENT_DAYS} days"));
    let when = if input.all_day {
        if input.starts_at.is_some() || input.ends_at.is_some() {
            return Err(AppError::BadRequest(
                "an all-day event has dates, not times".into(),
            ));
        }
        let start = parse_date(input.start_date.as_deref(), "start_date")?
            .ok_or_else(|| AppError::BadRequest("start_date is required".into()))?;
        let end = parse_date(input.end_date.as_deref(), "end_date")?.unwrap_or(start);
        if end < start {
            return Err(AppError::BadRequest(
                "end_date must not be before start_date".into(),
            ));
        }
        if (end - start).whole_days() >= MAX_EVENT_DAYS {
            return Err(too_long());
        }
        When {
            start_date: Some(start),
            end_date: Some(end),
            starts_at: None,
            ends_at: None,
        }
    } else {
        if input.start_date.is_some() || input.end_date.is_some() {
            return Err(AppError::BadRequest(
                "a timed event has times, not dates".into(),
            ));
        }
        let starts = parse_instant(input.starts_at.as_deref(), "starts_at")?;
        let ends = parse_instant(input.ends_at.as_deref(), "ends_at")?;
        if ends < starts {
            return Err(AppError::BadRequest(
                "ends_at must not be before starts_at".into(),
            ));
        }
        if ends - starts > Duration::days(MAX_EVENT_DAYS) {
            return Err(too_long());
        }
        When {
            start_date: None,
            end_date: None,
            starts_at: Some(starts),
            ends_at: Some(ends),
        }
    };
    Ok(Valid {
        title,
        description,
        when,
        timezone,
    })
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
        access: Access::parse_present(&row.access)?,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    params(("item_id" = String, Path, format = Uuid)), request_body = CreateEvent,
    responses((status = 201, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "events")]
pub async fn create(
    State(state): State<AppState>,
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
        .bind(input.event.all_day)
        .bind(valid.when.start_date)
        .bind(valid.when.end_date)
        .bind(valid.when.starts_at)
        .bind(valid.when.ends_at)
        .bind(&valid.timezone)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
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
    params(("event_id" = String, Path, format = Uuid)), request_body = UpdateEvent,
    responses((status = 200, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = EventConflict)), tag = "events")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEvent>,
) -> Result<Response, AppError> {
    let valid = validate(&state, &input.event)?;
    let mut tx = state.db.begin_as(user).await?;
    let outcome: String = sqlx::query_scalar(
        "SELECT app_update_event($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(input.base_version)
    .bind(input.event.kind.as_str())
    .bind(&valid.title)
    .bind(&valid.description)
    .bind(input.event.all_day)
    .bind(valid.when.start_date)
    .bind(valid.when.end_date)
    .bind(valid.when.starts_at)
    .bind(valid.when.ends_at)
    .bind(&valid.timezone)
    .bind(new_ids(2))
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    let event = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        (
            StatusCode::CONFLICT,
            Json(EventConflict {
                error: "conflict",
                current: event,
            }),
        )
            .into_response()
    } else {
        Json(event).into_response()
    })
}

/// 일정 지우기(쓰기 권한). 행은 남기고 목록·달력에서 뺀다.
#[utoipa::path(delete, path = "/api/events/{event_id}", operation_id = "delete_event",
    params(("event_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "events")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_event($1, $2)")
        .bind(id)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
