//! 스터디 변경 명령.

use super::{AddStudyUnits, PlanUnits, SetUnitProgress, UnitsAdded, UnitsPlanned, UpdateStudyUnit};
use crate::RecordsState;
use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::ItemId,
};
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;
use uuid::Uuid;

/// 단위 여러 개 추가(쓸 수 있는 사람). 같은 요청 id는 한 번만.
#[utoipa::path(post, path = "/api/items/{item_id}/study/units", operation_id = "add_study_units",
    params(("item_id" = String, Path, format = Uuid)), request_body = AddStudyUnits,
    responses((status = 200, body = UnitsAdded), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "study")]
pub async fn add_units(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<AddStudyUnits>,
) -> Result<Json<UnitsAdded>, AppError> {
    let rows = serde_json::to_value(&input.units)
        .map_err(|e| AppError::BadRequest(format!("units: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    // 단위마다 하나, 그 뒤 감사 기록 하나.
    let ids = new_ids(input.units.len() + 1);
    let added: i32 = sqlx::query_scalar("SELECT app_add_study_units($1, $2, $3)")
        .bind(item)
        .bind(rows)
        .bind(ids)
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
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(unit): Path<Uuid>,
    Json(input): Json<UpdateStudyUnit>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_update_study_unit($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(unit)
        .bind(input.version)
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
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(unit): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_study_unit($1, $2)")
        .bind(unit)
        .bind(new_ids(1))
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
    State(state): State<RecordsState>,
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
    State(state): State<RecordsState>,
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
