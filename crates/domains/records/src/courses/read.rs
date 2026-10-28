//! 강좌 정보·회차·개인 진도를 각각 읽는다.

use super::{Course, CourseDetails, Episode, MyProgress};
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
use time::Date;
use uuid::Uuid;

type DetailsRow = (
    String,
    String,
    String,
    String,
    String,
    Option<Date>,
    Option<Date>,
    Option<Date>,
);

#[utoipa::path(get, path = "/api/items/{item_id}/course", operation_id = "get_course",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Course), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn get(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Course>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let access = learning::access(&mut tx, item, LearningKind::Course).await?;
    let details: Option<DetailsRow> = sqlx::query_as(
        "SELECT subject, instructor, platform, url, textbook, starts_on, expires_on, target_on
         FROM course_details WHERE item_id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    let details = details.map_or_else(CourseDetails::default, |d| CourseDetails {
        subject: d.0,
        instructor: d.1,
        platform: d.2,
        url: d.3,
        textbook: d.4,
        starts_on: d.5,
        expires_on: d.6,
        target_on: d.7,
    });
    Ok(Json(Course { details, access }))
}

#[utoipa::path(get, path = "/api/items/{item_id}/episodes", operation_id = "course_episodes",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Episode]), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn episodes(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Episode>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    learning::access(&mut tx, item, LearningKind::Course).await?;
    let rows = sqlx::query_as(
        "SELECT id, number, title, chapter, duration_seconds, url, materials, planned_on, version
         FROM course_episodes WHERE item_id = $1 ORDER BY number, created_at, id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

#[utoipa::path(get, path = "/api/items/{item_id}/episode-progress", operation_id = "my_episode_progress",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [MyProgress]), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn progress(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<MyProgress>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    learning::access(&mut tx, item, LearningKind::Course).await?;
    let rows = sqlx::query_as(
        "SELECT p.episode_id, p.status, p.planned_on, p.completed_on, p.watch_count,
                p.last_position_seconds, p.note
         FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
         WHERE e.item_id = $1 AND p.user_id = app_user_id() ORDER BY e.number, e.created_at, e.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

pub(crate) async fn load_episode(tx: &mut UserTx, id: Uuid) -> Result<Episode, AppError> {
    sqlx::query_as(
        "SELECT id, number, title, chapter, duration_seconds, url, materials, planned_on, version
         FROM course_episodes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)
}

use super::{Participant, TodayQuery};
use axum::extract::Query;
use fvoci_core::ids::UserId;

/// 함께 수강하는 사람들의 진도 집계(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/course/participants", operation_id = "course_participants",
    params(("item_id" = String, Path, format = Uuid), TodayQuery),
    responses((status = 200, body = [Participant]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn participants(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Query(query): Query<TodayQuery>,
) -> Result<Json<Vec<Participant>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(UserId, String, i32, i32, i32, i64)> = sqlx::query_as(
        "SELECT user_id, display_name, done, in_progress, overdue, done_seconds
         FROM app_course_participants($1, $2)",
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
                |(user_id, display_name, done, in_progress, overdue, done_seconds)| Participant {
                    user_id,
                    display_name,
                    done,
                    in_progress,
                    overdue,
                    done_seconds,
                },
            )
            .collect(),
    ))
}
