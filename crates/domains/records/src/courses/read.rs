//! 학습 정보·단위·참여자 읽기.

use super::{Course, CourseDetails, Episode, MyProgress, Participant, TodayQuery};
use crate::RecordsState;
use axum::extract::Query;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::{ItemId, UserId},
};
use fvoci_http::session::CurrentUser;
use time::Date;
use uuid::Uuid;

type EpisodeRow = (
    Uuid,
    i32,
    String,
    String,
    i32,
    String,
    String,
    Option<Date>,
    Option<String>,
    Option<Date>,
    Option<Date>,
    Option<i32>,
    Option<i32>,
    Option<String>,
    i32,
);

/// 강좌(볼 수 있는 사람). 강의 모듈의 항목이 아니면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/course", operation_id = "get_course",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Course), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn get(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Course>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let access: String = sqlx::query_scalar("SELECT app_course_access($1)")
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 쓰기 검사(app_require_course_write)와 같이 항목이나 그 모듈이 보관되면 읽기 전용이다.
    let archived: bool = sqlx::query_scalar(
        "SELECT i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL
         FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_one(tx.conn())
    .await?;
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
    let details: Option<DetailsRow> = sqlx::query_as(
        "SELECT subject, instructor, platform, url, textbook, starts_on, expires_on, target_on
         FROM course_details WHERE item_id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    let rows: Vec<EpisodeRow> = sqlx::query_as(
        "SELECT e.id, e.number, e.title, e.chapter, e.duration_seconds, e.url, e.materials, e.planned_on,
                p.status, p.planned_on, p.completed_on, p.watch_count, p.last_position_seconds, p.note,
                e.version
         FROM course_episodes e
         LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
         WHERE e.item_id = $1 ORDER BY e.number, e.created_at, e.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
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
    let episodes = rows
        .into_iter()
        .map(|r| Episode {
            id: r.0,
            number: r.1,
            title: r.2,
            chapter: r.3,
            duration_seconds: r.4,
            url: r.5,
            materials: r.6,
            planned_on: r.7,
            mine: MyProgress {
                status: r.8.unwrap_or_else(|| "not_started".into()),
                planned_on: r.9,
                completed_on: r.10,
                watch_count: r.11.unwrap_or_default(),
                last_position_seconds: r.12,
                note: r.13.unwrap_or_default(),
            },
            version: r.14,
        })
        .collect();
    Ok(Json(Course {
        details,
        episodes,
        can_write: !archived && matches!(access.as_str(), "owner" | "write"),
    }))
}

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
    .map_err(AppError::from_db_function)?;
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
