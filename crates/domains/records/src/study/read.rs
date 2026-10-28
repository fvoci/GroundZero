//! 스터디 정보·단위·개인 진행을 각각 읽는다.

use super::{MyUnitProgress, StudyPlan, StudyUnit};
use crate::{
    RecordsState,
    learning::{self, LearningKind},
};
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::ItemId,
};
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;
use uuid::Uuid;

#[utoipa::path(get, path = "/api/items/{item_id}/study", operation_id = "get_study_plan",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = StudyPlan), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn get(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<StudyPlan>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let access = learning::access(&mut tx, item, LearningKind::Study).await?;
    tx.commit().await?;
    Ok(Json(StudyPlan { access }))
}

#[utoipa::path(get, path = "/api/items/{item_id}/study/units", operation_id = "study_units",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [StudyUnit]), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn units(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<StudyUnit>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    learning::access(&mut tx, item, LearningKind::Study).await?;
    let rows = sqlx::query_as(
        "SELECT id, number, title, chapter, unit, amount, planned_on, version
         FROM study_units WHERE item_id = $1 ORDER BY number, created_at, id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

#[utoipa::path(get, path = "/api/items/{item_id}/study/progress", operation_id = "my_study_progress",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [MyUnitProgress]), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn progress(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<MyUnitProgress>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    learning::access(&mut tx, item, LearningKind::Study).await?;
    let rows = sqlx::query_as(
        "SELECT p.unit_id, p.status, p.done_amount, p.planned_on, p.completed_on, p.note
         FROM unit_progress p JOIN study_units s ON s.id = p.unit_id
         WHERE s.item_id = $1 AND p.user_id = app_user_id() ORDER BY s.number, s.created_at, s.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

pub(crate) async fn load_unit(tx: &mut UserTx, id: Uuid) -> Result<StudyUnit, AppError> {
    sqlx::query_as(
        "SELECT id, number, title, chapter, unit, amount, planned_on, version
         FROM study_units WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)
}

use super::StudyParticipant;
use crate::courses::TodayQuery;
use axum::extract::Query;
use fvoci_core::ids::UserId;

/// 함께 하는 사람들의 진행 집계(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/study/participants", operation_id = "study_participants",
    params(("item_id" = String, Path, format = Uuid), TodayQuery),
    responses((status = 200, body = [StudyParticipant]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn participants(
    State(state): State<RecordsState>,
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
    .map_err(|error| crate::validation::from_db(error, "today"))?;
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
