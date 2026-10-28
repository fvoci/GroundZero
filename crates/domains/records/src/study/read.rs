//! 스터디 자원과 참여자 읽기.

use super::{MyUnitProgress, StudyParticipant, StudyPlan, StudyUnit};
use crate::RecordsState;
use crate::courses::TodayQuery;
use axum::extract::Query;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::ids::UserId;
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::ItemId,
};
use fvoci_http::session::CurrentUser;
use time::Date;
use uuid::Uuid;

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
    i32,
);

/// 스터디 계획(볼 수 있는 사람). 스터디 모듈의 항목이 아니면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/study", operation_id = "get_study_plan",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = StudyPlan), (status = 404, body = ErrorBody)), tag = "study")]
pub async fn get(
    State(state): State<RecordsState>,
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
                p.status, p.done_amount, p.planned_on, p.completed_on, p.note, s.version
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
            version: r.12,
        })
        .collect();
    Ok(Json(StudyPlan {
        units,
        can_write: !archived && matches!(access.as_str(), "owner" | "write"),
    }))
}

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
