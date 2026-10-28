//! 회의·발표·행사 일정과 워크스페이스 달력 (D-19, D-31, D-50, D-57).
//!
//! 일정은 항목에 속하고 권한은 항목을 따른다(DB 함수 `app_access`). 시각이 있는 일정은 UTC 시각과 일정의
//! 시간대를 함께 저장하고, 종일 일정은 날짜 그대로 둔다(D-50). 달력은 워크스페이스 단위의 기간 조회로, 볼 수 있는
//! 업무(시작·마감)와 일정을 함께 돌려준다(홈의 주간 달력도 같은 API를 쓴다).
//! DB 정의는 `crates/db/migrations/202610280002_organizations.sql`의 `app_access`,
//! `crates/db/migrations/202610280005_work.sql`의 `app_event_occurrences` 등에 있다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::features::documents::Access;
use crate::features::recurrence::RecurrenceRule;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_core::validate::{format_date, parse_date, validate_text};
use fvoci_db::{UserTx, is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;

const MAX_DESCRIPTION_CHARS: usize = 10_000;
/// 일정 하나의 최대 길이(일).
const MAX_EVENT_DAYS: i64 = 366;
/// 달력 한 번 조회의 최대 기간(일).
const MAX_RANGE_DAYS: i64 = 62;

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

/// 달력 기간. `from`·`to`는 날짜(끝 포함), `tz`는 보는 사람의 시간대(시각 있는 일정의 날짜 경계).
/// `module_id`·`item_id`로 범위를 그 모듈·항목으로 좁힌다(D-80, 같은 데이터를 거를 뿐 사본은 없다).
#[derive(Debug, Deserialize, IntoParams)]
pub struct CalendarRange {
    pub from: String,
    pub to: String,
    pub tz: String,
    /// 이 모듈의 항목만.
    #[serde(default)]
    #[param(value_type = Option<String>, format = Uuid)]
    pub module_id: Option<Uuid>,
    /// 이 항목만.
    #[serde(default)]
    #[param(value_type = Option<String>, format = Uuid)]
    pub item_id: Option<Uuid>,
}

/// 업무·일정 밖의 날짜 출처(D-80). 원래 자리의 날짜를 그대로 읽는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarDateKind {
    /// 강의 회차 수강 예정일(내 계획일, 없으면 공통 계획일).
    Episode,
    /// 스터디 계획 단위 계획일(내 계획일, 없으면 공통 계획일).
    StudyUnit,
    /// 연구 투고 제출 예정일.
    Submission,
    /// 연구 투고 수정 마감일.
    RevisionDue,
    /// 프로젝트 산출물 마감일.
    Deliverable,
}

impl CalendarDateKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "episode" => Some(Self::Episode),
            "study_unit" => Some(Self::StudyUnit),
            "submission" => Some(Self::Submission),
            "revision_due" => Some(Self::RevisionDue),
            "deliverable" => Some(Self::Deliverable),
            _ => None,
        }
    }
}

/// 달력의 날짜 하나(D-80).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarDate {
    pub kind: CalendarDateKind,
    /// 원래 대상(회차·스터디 단위·기록)의 id.
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub title: String,
    pub date: String,
    /// 내 계획일이면 참(본인만 보는 개인 일정), 공통 계획일·기록 날짜면 거짓.
    pub personal: bool,
    /// 끝난 것(내가 이수·완료, 게재·거절·승인·완료 상태).
    pub done: bool,
}

/// 달력의 업무(시작일~마감일).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarTask {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub milestone: bool,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 달력에 나온 항목의 이름(업무·일정이 어디에 속하는지 보이기 위해).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarItem {
    pub id: ItemId,
    pub reference: Option<String>,
    pub title: String,
}

/// 일정의 회차 하나(반복이 아니면 그 일정 자체). 종일이면 날짜, 아니면 UTC 시각.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventOccurrence {
    #[schema(value_type = String, format = Uuid)]
    pub event_id: Uuid,
    /// 일정 시간대 기준의 회차 날짜(회차 빼기에 쓴다).
    pub occurrence: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub starts_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// 반복 회차의 현지 시각이 그 시간대에 없어(여름시간 시작) 건너뛴 회차(D-81). 시각은 없다.
    pub skipped: bool,
}

/// 아직 만들지 않은 반복 업무 회차(예정). 읽기 전용 계산이며 쓰기 권한이 있는 사람이 항목을 열면 실제로 만든다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectedTask {
    #[schema(value_type = String, format = Uuid)]
    pub series_id: Uuid,
    /// 반복의 첫 회차(틀).
    #[schema(value_type = String, format = Uuid)]
    pub root_task_id: Uuid,
    pub item_id: ItemId,
    pub title: String,
    pub milestone: bool,
    pub occurrence: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 기간 안의 볼 수 있는 업무·일정.
#[derive(Debug, Serialize, ToSchema)]
pub struct Calendar {
    pub tasks: Vec<CalendarTask>,
    pub projected_tasks: Vec<ProjectedTask>,
    /// 기간 안에 회차가 있는 일정의 정의.
    pub events: Vec<Event>,
    pub occurrences: Vec<EventOccurrence>,
    /// 강의·스터디 계획일, 투고·산출물 날짜(D-80).
    pub dates: Vec<CalendarDate>,
    pub items: Vec<CalendarItem>,
}

/// 워크스페이스 달력(멤버). 볼 수 있는 업무·예정 회차·일정 회차만 나온다. 기간은 62일까지. 읽기 전용이다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/calendar", operation_id = "workspace_calendar",
    params(("workspace_id" = String, Path, format = Uuid), CalendarRange),
    responses((status = 200, body = Calendar), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "events")]
pub async fn calendar(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Query(range): Query<CalendarRange>,
) -> Result<Json<Calendar>, AppError> {
    let from = parse_date(Some(&range.from), "from")?
        .ok_or_else(|| AppError::BadRequest("from is required".into()))?;
    let to = parse_date(Some(&range.to), "to")?
        .ok_or_else(|| AppError::BadRequest("to is required".into()))?;
    if to < from || (to - from).whole_days() >= MAX_RANGE_DAYS {
        return Err(AppError::BadRequest(format!(
            "the range must be 1 to {MAX_RANGE_DAYS} days"
        )));
    }
    let tz = range.tz.trim();
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_visible(&mut tx, crate::access::Kind::Workspace, ws).await?;
    // 보는 사람의 시간대: 브라우저가 아는 이름만(일정 시간대와 같은 규칙).
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 보는 사람의 날짜 경계. 하루의 시작은 그날 0시가 처음 오는 시점이다(0시가 두 번 오는 날은 앞의 0시, 없는 날은
    // 건너뛴 직후). 시간 기록·검색의 날 경계(`time_tracking.rs`, `search_api.rs`, 감사 S21)와 같은 식.
    let (from_at, to_at): (OffsetDateTime, OffsetDateTime) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(from)
    .bind(to)
    .bind(tz)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    type TaskRow = (
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
    let mut task_rows: Vec<TaskRow> = sqlx::query_as(
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
    .await?;
    type ProjectedRow = (
        Uuid,
        Uuid,
        ItemId,
        String,
        bool,
        Date,
        Option<Date>,
        Option<Date>,
    );
    let mut projected_rows: Vec<ProjectedRow> = sqlx::query_as(
        "SELECT series_id, root_task_id, item_id, title, milestone, occurrence, start_date, due_date
         FROM app_projected_occurrences($1, NULL, $2, $3) ORDER BY occurrence, series_id",
    )
    .bind(ws)
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await?;
    type OccurrenceRow = (
        Uuid,
        Date,
        Option<Date>,
        Option<Date>,
        Option<OffsetDateTime>,
        Option<OffsetDateTime>,
        bool,
    );
    let mut occurrence_rows: Vec<OccurrenceRow> =
        sqlx::query_as("SELECT * FROM app_event_occurrences($1, $2, $3, $4, $5)")
            .bind(ws)
            .bind(from_at)
            .bind(to_at)
            .bind(from)
            .bind(to)
            .fetch_all(tx.conn())
            .await?;
    let mut event_ids: Vec<Uuid> = occurrence_rows.iter().map(|r| r.0).collect();
    event_ids.sort_unstable();
    event_ids.dedup();
    let mut event_rows: Vec<EventRow> = sqlx::query_as(&format!(
        "{EVENT_SELECT} WHERE e.id = ANY($1) AND e.deleted_at IS NULL ORDER BY {EVENT_ORDER}"
    ))
    .bind(&event_ids)
    .fetch_all(tx.conn())
    .await?;
    // 강의·스터디 계획일은 내 계획일이 있으면 그것(본인 진도 행만 읽힌다), 없으면 공통 계획일. 투고·산출물은 기록의
    // 날짜. 모두 그 항목을 볼 수 있을 때만 나온다(RLS·app_access).
    type DateRow = (String, Uuid, ItemId, String, Date, bool, bool);
    let mut date_rows: Vec<DateRow> = sqlx::query_as(
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
    .await?;
    // 범위(D-80): 모듈·항목으로 좁히면 그 안의 볼 수 있는 항목만 남긴다. 범위 밖이거나 볼 수 없으면 404.
    if let Some(scope) = calendar_scope(&mut tx, ws, &range).await? {
        let keep = |item: &ItemId| scope.contains(item);
        task_rows.retain(|r| keep(&r.1));
        projected_rows.retain(|r| keep(&r.2));
        event_rows.retain(|e| keep(&e.item_id));
        let kept: std::collections::HashSet<Uuid> = event_rows.iter().map(|e| e.id).collect();
        occurrence_rows.retain(|r| kept.contains(&r.0));
        date_rows.retain(|r| keep(&r.2));
    }
    let mut item_ids: Vec<ItemId> = task_rows.iter().map(|r| r.1).collect();
    item_ids.extend(date_rows.iter().map(|r| r.2));
    item_ids.extend(projected_rows.iter().map(|r| r.2));
    item_ids.extend(event_rows.iter().map(|r| r.item_id));
    item_ids.sort_unstable_by_key(|id| id.0);
    item_ids.dedup();
    let items: Vec<(ItemId, Option<String>, String)> = sqlx::query_as(
        "SELECT i.id, app_item_reference(i.id), i.title FROM items i WHERE i.id = ANY($1) ORDER BY i.title, i.id",
    )
    .bind(&item_ids)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let tasks = task_rows
        .into_iter()
        .map(
            |(id, item_id, reference, title, status, status_category, milestone, start, due)| {
                Ok(CalendarTask {
                    id,
                    item_id,
                    reference,
                    title,
                    status,
                    status_category,
                    milestone,
                    start_date: format_date(start),
                    due_date: format_date(due),
                })
            },
        )
        .collect::<Result<_, AppError>>()?;
    Ok(Json(Calendar {
        tasks,
        projected_tasks: projected_rows
            .into_iter()
            .map(
                |(series_id, root_task_id, item_id, title, milestone, occurrence, start, due)| {
                    ProjectedTask {
                        series_id,
                        root_task_id,
                        item_id,
                        title,
                        milestone,
                        occurrence: format_date(Some(occurrence)).unwrap_or_default(),
                        start_date: format_date(start),
                        due_date: format_date(due),
                    }
                },
            )
            .collect(),
        events: event_rows
            .into_iter()
            .map(to_event)
            .collect::<Result<_, _>>()?,
        occurrences: occurrence_rows
            .into_iter()
            .map(
                |(event_id, occurrence, start_date, end_date, starts_at, ends_at, skipped)| {
                    EventOccurrence {
                        event_id,
                        occurrence: format_date(Some(occurrence)).unwrap_or_default(),
                        start_date: format_date(start_date),
                        end_date: format_date(end_date),
                        starts_at,
                        ends_at,
                        skipped,
                    }
                },
            )
            .collect(),
        dates: date_rows
            .into_iter()
            .map(|(kind, id, item_id, title, date, personal, done)| {
                Ok(CalendarDate {
                    kind: CalendarDateKind::parse(&kind)
                        .ok_or_else(|| AppError::Config(format!("unknown calendar date {kind}")))?,
                    id,
                    item_id,
                    title,
                    date: format_date(Some(date)).unwrap_or_default(),
                    personal,
                    done,
                })
            })
            .collect::<Result<_, AppError>>()?,
        items: items
            .into_iter()
            .map(|(id, reference, title)| CalendarItem {
                id,
                reference,
                title,
            })
            .collect(),
    }))
}

/// 달력 범위(D-80): 항목이면 그 항목 하나, 모듈이면 그 모듈의 볼 수 있는 항목들. 범위가 없으면 `None`(워크스페이스 전체).
/// 그 워크스페이스에 없거나 볼 수 없으면 404.
async fn calendar_scope(
    tx: &mut UserTx,
    ws: WorkspaceId,
    range: &CalendarRange,
) -> Result<Option<std::collections::HashSet<ItemId>>, AppError> {
    if let Some(item) = range.item_id {
        let found: Option<ItemId> =
            sqlx::query_scalar("SELECT id FROM items WHERE id = $1 AND workspace_id = $2")
                .bind(item)
                .bind(ws)
                .fetch_optional(tx.conn())
                .await?;
        let item = found.ok_or(AppError::NotFound)?;
        if let Some(module) = range.module_id {
            let inside: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM items WHERE id = $1 AND module_id = $2)",
            )
            .bind(item)
            .bind(module)
            .fetch_one(tx.conn())
            .await?;
            if !inside {
                return Err(AppError::NotFound);
            }
        }
        return Ok(Some(std::iter::once(item).collect()));
    }
    if let Some(module) = range.module_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM modules WHERE id = $1 AND workspace_id = $2)",
        )
        .bind(module)
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
        if !exists {
            return Err(AppError::NotFound);
        }
        let items: Vec<ItemId> = sqlx::query_scalar("SELECT id FROM items WHERE module_id = $1")
            .bind(module)
            .fetch_all(tx.conn())
            .await?;
        return Ok(Some(items.into_iter().collect()));
    }
    Ok(None)
}
