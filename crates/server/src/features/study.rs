//! 스터디 계획과 개인별 진행 (D-36). 계획 단위(교육과정·챕터·단계, 분량, 공통 계획일)는 함께 보고, 진행(상태·한
//! 분량·내 계획일·메모)은 사람마다 따로 둔다. 계획 대비 진행·남은 분량 계산은 화면이 단위 목록에서 한다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::courses::TodayQuery;
use crate::http::AppState;
use crate::ids::{ItemId, UserId};

time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 내 진행.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyUnitProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    /// 한 분량(단위의 분량과 같은 단위).
    pub done_amount: i32,
    /// 내 계획일(없으면 공통 계획일).
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_on: Option<Date>,
    /// 개인 메모(나만 봄).
    pub note: String,
}

/// 계획 단위.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub number: i32,
    pub title: String,
    /// 교육과정·챕터·단계.
    pub chapter: String,
    /// `pages`·`problems`·`minutes`·`items`.
    pub unit: String,
    pub amount: i32,
    /// 공통 계획일.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub mine: MyUnitProgress,
}

/// 스터디 계획.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyPlan {
    pub units: Vec<StudyUnit>,
    /// 단위를 고칠 수 있는지(진행은 볼 수 있으면 누구나 자기 것).
    pub can_write: bool,
}

/// 추가할 단위.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewStudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub number: Option<i32>,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    #[serde(default)]
    pub amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

fn default_unit() -> String {
    "pages".into()
}

/// 단위 여러 개 추가(최대 1000개).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddStudyUnits {
    pub units: Vec<NewStudyUnit>,
}

/// 추가한 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnitsAdded {
    pub added: i32,
}

/// 단위 고치기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateStudyUnit {
    pub number: i32,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    pub unit: String,
    #[serde(default)]
    pub amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 진행 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetUnitProgress {
    pub status: String,
    #[serde(default)]
    pub done_amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(default)]
    pub note: String,
    /// 사용자 시간대의 오늘(완료한 날).
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub today: Option<Date>,
}

/// 내 계획일 하나.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PlanEntry {
    #[schema(value_type = String, format = Uuid)]
    pub unit_id: Uuid,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 계획일 여러 개(밀린 단위 재배치). 날짜는 사용자가 정한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanUnits {
    pub entries: Vec<PlanEntry>,
}

/// 바꾼 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnitsPlanned {
    pub planned: i32,
}

/// 함께 하는 사람의 진행 집계(메모·개인 날짜는 없음).
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyParticipant {
    pub user_id: UserId,
    pub display_name: String,
    pub done: i32,
    pub in_progress: i32,
    pub overdue: i32,
    pub done_amount: i64,
}

type UnitRow = (
    Uuid,
    i32,
    String,
    String,
    String,
    i32,
    Option<Date>,
    Option<String>,
    Option<i32>,
    Option<Date>,
    Option<Date>,
    Option<String>,
);

/// 스터디 계획(볼 수 있는 사람). 스터디 모듈의 항목이 아니면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/study", operation_id = "get_study_plan",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = StudyPlan), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<StudyPlan>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let access: String = sqlx::query_scalar("SELECT app_study_access($1)")
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let archived: bool = sqlx::query_scalar(
        "SELECT i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL
         FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_one(tx.conn())
    .await?;
    let rows: Vec<UnitRow> = sqlx::query_as(
        "SELECT s.id, s.number, s.title, s.chapter, s.unit, s.amount, s.planned_on,
                p.status, p.done_amount, p.planned_on, p.completed_on, p.note
         FROM study_units s
         LEFT JOIN unit_progress p ON p.unit_id = s.id AND p.user_id = app_user_id()
         WHERE s.item_id = $1 ORDER BY s.number, s.created_at, s.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let units = rows
        .into_iter()
        .map(|r| StudyUnit {
            id: r.0,
            number: r.1,
            title: r.2,
            chapter: r.3,
            unit: r.4,
            amount: r.5,
            planned_on: r.6,
            mine: MyUnitProgress {
                status: r.7.unwrap_or_else(|| "not_started".into()),
                done_amount: r.8.unwrap_or_default(),
                planned_on: r.9,
                completed_on: r.10,
                note: r.11.unwrap_or_default(),
            },
        })
        .collect();
    Ok(Json(StudyPlan {
        units,
        can_write: !archived && matches!(access.as_str(), "owner" | "write"),
    }))
}

/// 단위 여러 개 추가(쓸 수 있는 사람). 같은 요청 id는 한 번만.
#[utoipa::path(post, path = "/api/items/{item_id}/study/units", operation_id = "add_study_units",
    params(("item_id" = String, Path, format = Uuid)), request_body = AddStudyUnits,
    responses((status = 200, body = UnitsAdded), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "study")]
pub async fn add_units(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<AddStudyUnits>,
) -> Result<Json<UnitsAdded>, AppError> {
    let rows = serde_json::to_value(&input.units)
        .map_err(|e| AppError::BadRequest(format!("units: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    let added: i32 = sqlx::query_scalar("SELECT app_add_study_units($1, $2)")
        .bind(item)
        .bind(rows)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(UnitsAdded { added }))
}

/// 단위 고치기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/study-units/{unit_id}", operation_id = "update_study_unit",
    params(("unit_id" = String, Path, format = Uuid)), request_body = UpdateStudyUnit,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "study")]
pub async fn update_unit(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(unit): Path<Uuid>,
    Json(input): Json<UpdateStudyUnit>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_update_study_unit($1, $2, $3, $4, $5, $6, $7)")
        .bind(unit)
        .bind(input.number)
        .bind(&input.title)
        .bind(&input.chapter)
        .bind(&input.unit)
        .bind(input.amount)
        .bind(input.planned_on)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 단위 지우기(쓸 수 있는 사람). 모든 사람의 그 단위 진행도 지워진다.
#[utoipa::path(delete, path = "/api/study-units/{unit_id}", operation_id = "delete_study_unit",
    params(("unit_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn delete_unit(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(unit): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_study_unit($1)")
        .bind(unit)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 내 진행 바꾸기(스터디를 볼 수 있으면 누구나 자기 것).
#[utoipa::path(put, path = "/api/study-units/{unit_id}/progress", operation_id = "set_unit_progress",
    params(("unit_id" = String, Path, format = Uuid)), request_body = SetUnitProgress,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn set_progress(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(unit): Path<Uuid>,
    Json(input): Json<SetUnitProgress>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_unit_progress($1, $2, $3, $4, $5, $6)")
        .bind(unit)
        .bind(&input.status)
        .bind(input.done_amount)
        .bind(input.planned_on)
        .bind(&input.note)
        .bind(input.today)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 내 계획일 여러 개 바꾸기(밀린 단위 재배치).
#[utoipa::path(put, path = "/api/items/{item_id}/study/plan", operation_id = "plan_study_units",
    params(("item_id" = String, Path, format = Uuid)), request_body = PlanUnits,
    responses((status = 200, body = UnitsPlanned), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "study")]
pub async fn plan(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<PlanUnits>,
) -> Result<Json<UnitsPlanned>, AppError> {
    let entries = serde_json::to_value(&input.entries)
        .map_err(|e| AppError::BadRequest(format!("entries: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    let planned: i32 = sqlx::query_scalar("SELECT app_plan_units($1, $2)")
        .bind(item)
        .bind(entries)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(UnitsPlanned { planned }))
}

/// 함께 하는 사람들의 진행 집계(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/study/participants", operation_id = "study_participants",
    params(("item_id" = String, Path, format = Uuid), TodayQuery),
    responses((status = 200, body = [StudyParticipant]), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn participants(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Query(query): Query<TodayQuery>,
) -> Result<Json<Vec<StudyParticipant>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(UserId, String, i32, i32, i32, i64)> = sqlx::query_as(
        "SELECT user_id, display_name, done, in_progress, overdue, done_amount
         FROM app_study_participants($1, $2)",
    )
    .bind(item)
    .bind(query.today)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(user_id, display_name, done, in_progress, overdue, done_amount)| {
                    StudyParticipant {
                        user_id,
                        display_name,
                        done,
                        in_progress,
                        overdue,
                        done_amount,
                    }
                },
            )
            .collect(),
    ))
}
