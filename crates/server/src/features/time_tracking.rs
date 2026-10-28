//! 스톱워치·시간 기록 (D-08, D-32, D-50).
//!
//! 시간 기록은 사용자 소유이며 워크스페이스 관리자에게도 보이지 않는다(RLS: 본인만). 진행 중인 타이머는
//! 사용자당 하나이고 서버 시각으로 재므로 앱을 닫아도 이어진다. 기록은 측정 뒤에 종류·대상을 정할 수 있고,
//! 고치면 본인만 보는 수정 이력에 이전 값이 남는다. 집계는 보는 사람의 시간대 날짜로 하며 기록은 시작한 날에
//! 센다. 시간 합계도 본인 기록만 돌려준다.
//! DB 정의는 `crates/db/migrations/202610250001_initial_state.sql`의 `app_start_timer`·`app_update_time_entry` 등에 있다.
//! 새 UUIDv7은 앱이 공급한다(D-88).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::features::tasks::{format_date, parse_date, validate_text};
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::WorkspaceId;
use fvoci_db::{UserTx, is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;

const MAX_MEMO_CHARS: usize = 2_000;
/// 목록·집계 한 번의 최대 기간(일).
const MAX_RANGE_DAYS: i64 = 400;

/// 시간 기록 종류. `study`는 순공부(강의 시청 제외), `lecture`는 강의 시청, `hobby`는 취미 활동, `rest`는 휴식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeKind {
    Work,
    Study,
    Lecture,
    Hobby,
    Rest,
    Other,
}

impl TimeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Study => "study",
            Self::Lecture => "lecture",
            Self::Hobby => "hobby",
            Self::Rest => "rest",
            Self::Other => "other",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "work" => Some(Self::Work),
            "study" => Some(Self::Study),
            "lecture" => Some(Self::Lecture),
            "hobby" => Some(Self::Hobby),
            "rest" => Some(Self::Rest),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// 연결 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Task,
    Document,
    Item,
    /// 강의 회차(D-32).
    Episode,
    /// 스터디 단위(D-32 공부).
    StudyUnit,
}

impl TargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Document => "document",
            Self::Item => "item",
            Self::Episode => "episode",
            Self::StudyUnit => "study_unit",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "task" => Some(Self::Task),
            "document" => Some(Self::Document),
            "item" => Some(Self::Item),
            "episode" => Some(Self::Episode),
            "study_unit" => Some(Self::StudyUnit),
            _ => None,
        }
    }
}

/// 연결 대상(보낼 때).
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
pub struct TargetInput {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 연결 대상(받을 때). 지금 볼 수 없으면 `label`이 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Target {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub label: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
}

/// 시간 기록.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeEntry {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub started_at: OffsetDateTime,
    /// 진행 중이면 없다.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ended_at: Option<OffsetDateTime>,
    /// 일시정지 중이면 그 시각.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub paused_at: Option<OffsetDateTime>,
    pub paused_seconds: i32,
    /// 지금까지의 길이(초). 진행 중이면 응답 시각 기준.
    pub seconds: i64,
    pub kind: TimeKind,
    pub memo: String,
    pub target: Option<Target>,
    pub version: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    id: Uuid,
    started_at: OffsetDateTime,
    ended_at: Option<OffsetDateTime>,
    paused_at: Option<OffsetDateTime>,
    paused_seconds: i32,
    seconds: i64,
    kind: String,
    memo: String,
    target_kind: Option<String>,
    target_id: Option<Uuid>,
    target_label: Option<String>,
    workspace_id: Option<WorkspaceId>,
    version: i32,
}

const ENTRY_SELECT: &str = "SELECT e.id, e.started_at, e.ended_at, e.paused_at, e.paused_seconds,
        app_entry_seconds(e) AS seconds, e.kind, e.memo, e.target_kind, e.target_id,
        app_time_target_label(e.target_kind, e.target_id) AS target_label, e.workspace_id, e.version
 FROM time_entries e";

fn to_entry(row: EntryRow) -> Result<TimeEntry, AppError> {
    let target = match (row.target_kind, row.target_id) {
        (Some(kind), Some(id)) => Some(Target {
            kind: TargetKind::parse(&kind)
                .ok_or_else(|| AppError::Config(format!("unknown target kind {kind}")))?,
            id,
            label: row.target_label,
            workspace_id: row.workspace_id,
        }),
        _ => None,
    };
    Ok(TimeEntry {
        id: row.id,
        started_at: row.started_at,
        ended_at: row.ended_at,
        paused_at: row.paused_at,
        paused_seconds: row.paused_seconds,
        seconds: row.seconds,
        kind: TimeKind::parse(&row.kind)
            .ok_or_else(|| AppError::Config(format!("unknown time kind {}", row.kind)))?,
        memo: row.memo,
        target,
        version: row.version,
    })
}

async fn load(tx: &mut UserTx, id: Uuid) -> Result<TimeEntry, AppError> {
    let row: Option<EntryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE e.id = $1 AND e.deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    to_entry(row.ok_or(AppError::NotFound)?)
}

fn parse_instant(raw: &str, field: &str) -> Result<OffsetDateTime, AppError> {
    OffsetDateTime::parse(raw.trim(), &Rfc3339).map_err(|_| {
        AppError::BadRequest(format!(
            "{field} must be a time like 2026-10-17T09:00:00+09:00"
        ))
    })
}

fn split_target(target: Option<TargetInput>) -> (Option<&'static str>, Option<Uuid>) {
    target.map_or((None, None), |t| (Some(t.kind.as_str()), Some(t.id)))
}

/// 진행 중인 타이머(없으면 null).
#[utoipa::path(get, path = "/api/me/timer", operation_id = "current_timer",
    responses((status = 200, body = Option<TimeEntry>)), tag = "time")]
pub async fn current(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Option<TimeEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let row: Option<EntryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE e.ended_at IS NULL AND e.deleted_at IS NULL"
    ))
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(row.map(to_entry).transpose()?))
}

/// 같은 대상에서 실행 중인 타이머를 다시 시작할 때의 선택.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerStartMode {
    /// 실행 중인 기록을 그대로 이어간다.
    Continue,
    /// 실행 중인 기록을 끝내고 새 기록을 시작한다.
    New,
}

impl TimerStartMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::New => "new",
        }
    }
}

/// 타이머 시작.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartTimer {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub kind: TimeKind,
    pub memo: String,
    pub target: Option<TargetInput>,
    /// 같은 대상이 실행 중이면 필수. 생략 시 409 `timer_running`.
    pub mode: Option<TimerStartMode>,
}

/// 새 타이머와 자동으로 끝낸 이전 기록.
#[derive(Debug, Serialize, ToSchema)]
pub struct StartedTimer {
    #[serde(flatten)]
    pub entry: TimeEntry,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub stopped_entry_id: Option<Uuid>,
}

/// 타이머 시작. 다른 대상의 기록은 자동으로 끝내며 같은 대상은 명시한 선택을 따른다.
#[utoipa::path(post, path = "/api/me/timer", operation_id = "start_timer", request_body = StartTimer,
    responses((status = 201, body = StartedTimer), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, description = "같은 대상 실행 중: timer_running, 그 밖의 상태 충돌: invalid_state", body = ErrorBody)), tag = "time")]
pub async fn start(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<StartTimer>,
) -> Result<(StatusCode, Json<StartedTimer>), AppError> {
    let memo = validate_text(&input.memo, MAX_MEMO_CHARS, "memo")?;
    let (target_kind, target_id) = split_target(input.target);
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let (id, stopped_entry_id): (Uuid, Option<Uuid>) = sqlx::query_as(
            "SELECT entry_id, stopped_entry_id FROM app_start_timer($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(input.request_id)
        .bind(input.kind.as_str())
        .bind(&memo)
        .bind(target_kind)
        .bind(target_id)
        .bind(new_ids(1))
        .bind(input.mode.map(TimerStartMode::as_str))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
        let entry = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok(StartedTimer {
            entry,
            stopped_entry_id,
        })
    };
    match run().await {
        Ok(entry) => Ok((StatusCode::CREATED, Json(entry))),
        // 같은 요청의 동시 재시도이거나 다른 타이머가 막 시작됐다. 다시 보면 둘 중 하나로 정해진다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 타이머 동작.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerAction {
    Pause,
    Resume,
    Stop,
}

/// 타이머 일시정지·재개·종료 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TimerActionInput {
    pub action: TimerAction,
}

/// 타이머 일시정지·재개·종료(본인). 이미 그 상태면 그대로.
#[utoipa::path(post, path = "/api/time-entries/{entry_id}/timer", operation_id = "timer_action",
    params(("entry_id" = String, Path, format = Uuid)), request_body = TimerActionInput,
    responses((status = 200, body = TimeEntry), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "time")]
pub async fn action(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TimerActionInput>,
) -> Result<Json<TimeEntry>, AppError> {
    let action = match input.action {
        TimerAction::Pause => "pause",
        TimerAction::Resume => "resume",
        TimerAction::Stop => "stop",
    };
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_timer_action($1, $2)")
        .bind(id)
        .bind(action)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let entry = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(entry))
}

/// 기간(보는 사람 시간대의 날짜, 끝 포함).
#[derive(Debug, Deserialize, IntoParams)]
pub struct TimeRange {
    pub from: String,
    pub to: String,
    pub tz: String,
}

struct Window {
    from_at: OffsetDateTime,
    to_at: OffsetDateTime,
}

async fn window(tx: &mut UserTx, range: &TimeRange) -> Result<Window, AppError> {
    let from = parse_date(Some(&range.from), "from")?
        .ok_or_else(|| AppError::BadRequest("from is required".into()))?;
    let to = parse_date(Some(&range.to), "to")?
        .ok_or_else(|| AppError::BadRequest("to is required".into()))?;
    if to < from || (to - from).whole_days() >= MAX_RANGE_DAYS {
        return Err(AppError::BadRequest(format!(
            "the range must be 1 to {MAX_RANGE_DAYS} days"
        )));
    }
    // 하루의 시작은 그 시간대에서 그날 0시가 처음 오는 시점이다. 0시가 두 번 오는 시간대(아바나·아소르스의 가을)에서
    // 기본 해석은 뒤의 0시라 앞의 0시~1시가 전날로 밀리고, 집계와 목록이 서로 다른 날에 두었다(감사 S21). 0시가 없는
    // 날(봄에 0시를 건너뜀)은 기본 해석(건너뛴 직후)을 쓴다. 집계의 날 경계(`summary`)도 같은 식이다.
    let (from_at, to_at): (OffsetDateTime, OffsetDateTime) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(from)
    .bind(to)
    .bind(range.tz.trim())
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    Ok(Window { from_at, to_at })
}

/// 내 시간 기록(기간 안에 시작한 것, 최신순).
#[utoipa::path(get, path = "/api/me/time-entries", operation_id = "list_time_entries", params(TimeRange),
    responses((status = 200, body = [TimeEntry]), (status = 400, body = ErrorBody)), tag = "time")]
pub async fn list(
    State(state): State<AppState>,
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

/// 기록 내용(수동 추가·고치기 공통).
#[derive(Debug, Deserialize, ToSchema)]
pub struct EntryInput {
    /// RFC 3339.
    pub started_at: String,
    /// RFC 3339. 진행 중인 타이머를 고칠 때만 없다.
    pub ended_at: Option<String>,
    pub paused_seconds: i32,
    pub kind: TimeKind,
    pub memo: String,
    pub target: Option<TargetInput>,
}

/// 수동 기록 추가.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddEntry {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    #[serde(flatten)]
    pub entry: EntryInput,
}

/// 기록 고치기. `base_version`이 지났으면 409와 서버 최신본.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEntry {
    pub base_version: i32,
    #[serde(flatten)]
    pub entry: EntryInput,
}

/// 409 응답: 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeEntryConflict {
    pub error: &'static str,
    pub current: TimeEntry,
}

struct ValidEntry {
    started: OffsetDateTime,
    ended: Option<OffsetDateTime>,
    memo: String,
}

fn validate_entry(input: &EntryInput) -> Result<ValidEntry, AppError> {
    let started = parse_instant(&input.started_at, "started_at")?;
    let ended = input
        .ended_at
        .as_deref()
        .map(|s| parse_instant(s, "ended_at"))
        .transpose()?;
    if input.paused_seconds < 0 {
        return Err(AppError::BadRequest(
            "paused_seconds must not be negative".into(),
        ));
    }
    Ok(ValidEntry {
        started,
        ended,
        memo: validate_text(&input.memo, MAX_MEMO_CHARS, "memo")?,
    })
}

/// 수동 기록 추가(끝난 기록). 같은 `request_id`면 같은 기록.
#[utoipa::path(post, path = "/api/me/time-entries", operation_id = "add_time_entry", request_body = AddEntry,
    responses((status = 201, body = TimeEntry), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "time")]
pub async fn add(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<AddEntry>,
) -> Result<(StatusCode, Json<TimeEntry>), AppError> {
    let valid = validate_entry(&input.entry)?;
    let ended = valid
        .ended
        .ok_or_else(|| AppError::BadRequest("ended_at is required".into()))?;
    let (target_kind, target_id) = split_target(input.entry.target);
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: Uuid =
            sqlx::query_scalar("SELECT app_add_time_entry($1, $2, $3, $4, $5, $6, $7, $8, $9)")
                .bind(input.request_id)
                .bind(valid.started)
                .bind(ended)
                .bind(input.entry.paused_seconds)
                .bind(input.entry.kind.as_str())
                .bind(&valid.memo)
                .bind(target_kind)
                .bind(target_id)
                .bind(new_ids(1))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let entry = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok(entry)
    };
    match run().await {
        Ok(entry) => Ok((StatusCode::CREATED, Json(entry))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 기록 고치기(본인). 이전 값은 수정 이력에 남는다. 진행 중이면 시작 시각·종류·메모·대상만 바꿀 수 있다.
#[utoipa::path(put, path = "/api/time-entries/{entry_id}", operation_id = "update_time_entry",
    params(("entry_id" = String, Path, format = Uuid)), request_body = UpdateEntry,
    responses((status = 200, body = TimeEntry), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = TimeEntryConflict)), tag = "time")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEntry>,
) -> Result<Response, AppError> {
    let valid = validate_entry(&input.entry)?;
    let (target_kind, target_id) = split_target(input.entry.target);
    let mut tx = state.db.begin_as(user).await?;
    let outcome: String =
        sqlx::query_scalar("SELECT app_update_time_entry($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)")
            .bind(id)
            .bind(input.base_version)
            .bind(valid.started)
            .bind(valid.ended)
            .bind(input.entry.paused_seconds)
            .bind(input.entry.kind.as_str())
            .bind(&valid.memo)
            .bind(target_kind)
            .bind(target_id)
            .bind(new_ids(1))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let entry = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        (
            StatusCode::CONFLICT,
            Json(TimeEntryConflict {
                error: "conflict",
                current: entry,
            }),
        )
            .into_response()
    } else {
        Json(entry).into_response()
    })
}

/// 기록 지우기(본인).
#[utoipa::path(delete, path = "/api/time-entries/{entry_id}", operation_id = "delete_time_entry",
    params(("entry_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_time_entry($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
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
    State(state): State<AppState>,
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

/// 집계 단위.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SummaryGroup {
    Day,
    Week,
    Month,
}

/// 집계 조건. 주는 `week_start`(`mon` 기본·`sun`)부터.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SummaryQuery {
    pub from: String,
    pub to: String,
    pub tz: String,
    pub group: SummaryGroup,
    /// 한 주 시작(D-81): `mon`(기본)·`sun`.
    #[serde(default)]
    pub week_start: Option<String>,
}

/// 기간·종류별 합계.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeSummaryRow {
    /// 기간의 첫날(`YYYY-MM-DD`).
    pub period: String,
    pub kind: TimeKind,
    pub seconds: i64,
}

/// 내 시간 집계(일·주·월, 보는 사람 시간대). 기록 원본은 그대로 두고 합계만 그 시간대의 자정에서 나눈다(D-81).
/// 일시정지 시각은 저장하지 않으므로 일시정지 시간은 경과 비율로 나눈다. 나눈 초의 합은 기록의 초와 같다.
#[utoipa::path(get, path = "/api/me/time-summary", operation_id = "time_summary", params(SummaryQuery),
    responses((status = 200, body = [TimeSummaryRow]), (status = 400, body = ErrorBody)), tag = "time")]
pub async fn summary(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<SummaryQuery>,
) -> Result<Json<Vec<TimeSummaryRow>>, AppError> {
    let range = TimeRange {
        from: query.from,
        to: query.to,
        tz: query.tz,
    };
    let unit = match query.group {
        SummaryGroup::Day => "day",
        SummaryGroup::Week => "week",
        SummaryGroup::Month => "month",
    };
    let week_offset: i32 = match query.week_start.as_deref().unwrap_or("mon") {
        "mon" => 0,
        "sun" => 1,
        _ => return Err(AppError::BadRequest("week_start must be mon or sun".into())),
    };
    let mut tx = state.db.begin_as(user).await?;
    let w = window(&mut tx, &range).await?;
    // 기록을 보는 사람 시간대의 날짜마다 조각으로 나누고, 조각의 초는 누적 경과 비율의 내림 차로 정해 조각의 합이
    // 기록의 초(app_entry_seconds)와 정확히 같게 한다. 기간 경계도 그 시간대의 자정이다. 날짜는 기간 안에서만
    // 펼친다(조각의 초는 기록 전체의 s·f로 나누므로 결과는 같다). 길이 제한이 없는 기록 하나가 조회마다 수백만 날을
    // 펼치지 않게 한다(감사 S20).
    let rows: Vec<(Date, String, i64)> = sqlx::query_as(
        "WITH e AS (
           SELECT e.kind, e.started_at AS s, coalesce(e.ended_at, now()) AS f, app_entry_seconds(e)::bigint AS active
           FROM time_entries e
           WHERE e.deleted_at IS NULL AND e.started_at < $2 AND coalesce(e.ended_at, now()) >= $1
         ),
         seg AS (
           SELECT e.kind, e.active, e.s, e.f, g.day, greatest(e.s, g.lo) AS ss, least(e.f, g.hi) AS sf
           FROM e
           CROSS JOIN LATERAL (
             SELECT d::date AS day,
                    coalesce(app_local_instant(d::date::timestamp, $3), d::date::timestamp AT TIME ZONE $3) AS lo,
                    coalesce(app_local_instant((d::date + 1)::timestamp, $3),
                             (d::date + 1)::timestamp AT TIME ZONE $3) AS hi
             FROM generate_series((greatest(e.s, $1) AT TIME ZONE $3)::date, (least(e.f, $2) AT TIME ZONE $3)::date,
                                  interval '1 day') d
           ) g
         ),
         part AS (
           SELECT kind, day, ss,
                  CASE WHEN f = s THEN CASE WHEN ss = s THEN active ELSE 0 END
                       ELSE floor(active * extract(epoch FROM sf - s) / extract(epoch FROM f - s))::bigint
                          - floor(active * extract(epoch FROM ss - s) / extract(epoch FROM f - s))::bigint
                  END AS seconds
           FROM seg
           WHERE sf > ss OR f = s
         )
         SELECT CASE $4
                  WHEN 'day' THEN day
                  WHEN 'week' THEN (date_trunc('week', day + $5) - make_interval(days => $5))::date
                  ELSE date_trunc('month', day)::date
                END AS period,
                kind, sum(seconds)::bigint
         FROM part
         WHERE ss >= $1 AND ss < $2
         GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .bind(w.from_at)
    .bind(w.to_at)
    .bind(range.tz.trim())
    .bind(unit)
    .bind(week_offset)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(period, kind, seconds)| {
            Ok(TimeSummaryRow {
                period: format_date(Some(period)).unwrap_or_default(),
                kind: TimeKind::parse(&kind)
                    .ok_or_else(|| AppError::Config(format!("unknown time kind {kind}")))?,
                seconds,
            })
        })
        .collect::<Result<_, AppError>>()
        .map(Json)
}

/// 업무의 예상 시간과 실제 시간.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskTime {
    pub estimate_minutes: Option<i32>,
    /// 내 기록의 합계(초).
    pub mine_seconds: i64,
}

/// 업무의 예상·실제 시간(업무를 볼 수 있는 사람). 기록 하나하나는 돌려주지 않는다.
#[utoipa::path(get, path = "/api/tasks/{task_id}/time", operation_id = "task_time",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = TaskTime), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn task_time(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskTime>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let mine_seconds: i64 = sqlx::query_scalar("SELECT app_task_time($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let estimate_minutes: Option<i32> =
        sqlx::query_scalar("SELECT estimate_minutes FROM tasks WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    tx.commit().await?;
    Ok(Json(TaskTime {
        estimate_minutes,
        mine_seconds,
    }))
}

/// 항목의 시간 합계 한 종류.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemTimeKind {
    pub kind: TimeKind,
    /// 내 끝난 기록의 합계(초).
    pub mine_seconds: i64,
}

/// 항목의 시간 합계(D-78): 항목 자체와 그 아래 업무·문서·강의 회차·스터디 단위의 기록을 종류별로 모은다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemTime {
    /// 항목의 휴지통에 없는 업무 예상 시간 합(분).
    pub estimate_minutes: i64,
    /// 기록이 있는 종류만, 종류 이름 순.
    pub kinds: Vec<ItemTimeKind>,
}

/// 항목의 예상·실제 시간(항목을 볼 수 있는 사람). 기록 하나하나는 돌려주지 않는다.
#[utoipa::path(get, path = "/api/items/{item_id}/time", operation_id = "item_time",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = ItemTime), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn item_time(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ItemTime>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT entry_kind, mine_seconds FROM app_item_time($1)")
            .bind(id)
            .fetch_all(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let estimate_minutes: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(estimate_minutes), 0)::bigint FROM tasks
         WHERE item_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    tx.commit().await?;
    let kinds = rows
        .into_iter()
        .filter_map(|(kind, mine_seconds)| {
            TimeKind::parse(&kind).map(|kind| ItemTimeKind { kind, mine_seconds })
        })
        .collect();
    Ok(Json(ItemTime {
        estimate_minutes,
        kinds,
    }))
}
