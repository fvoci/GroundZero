//! 강의 모듈: 강좌 정보·회차·개인별 진도 (D-37). 진도 계산(진행률·오늘·밀린 회차·하루 필요 분량)은 화면이
//! 회차 목록에서 한다. 서버는 저장과 권한, 함께 수강하는 사람의 집계만 맡는다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{ItemId, UserId};

time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 강좌 정보.
#[derive(Debug, Serialize, Deserialize, ToSchema, Default)]
pub struct CourseDetails {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub instructor: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub textbook: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub starts_on: Option<Date>,
    /// 수강권 만료일.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub expires_on: Option<Date>,
    /// 목표 완료일.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub target_on: Option<Date>,
}

/// 내 진도.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    /// 내 계획일(없으면 공통 계획일).
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    /// 처음 완료한 날.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_on: Option<Date>,
    /// 완료로 표시한 횟수(다시 보기 포함). 완료 회차 수는 한 번만 센다.
    pub watch_count: i32,
    pub last_position_seconds: Option<i32>,
    /// 개인 메모(나만 봄).
    pub note: String,
}

/// 회차.
#[derive(Debug, Serialize, ToSchema)]
pub struct Episode {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub number: i32,
    pub title: String,
    pub chapter: String,
    pub duration_seconds: i32,
    pub url: String,
    pub materials: String,
    /// 공통 계획일.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub mine: MyProgress,
}

/// 강좌: 정보·회차(번호순)·내 진도.
#[derive(Debug, Serialize, ToSchema)]
pub struct Course {
    pub details: CourseDetails,
    pub episodes: Vec<Episode>,
    /// 강좌 정보·회차를 고칠 수 있는지(진도는 볼 수 있으면 누구나 자기 것).
    pub can_write: bool,
}

/// 추가할 회차.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewEpisode {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 없으면 마지막 다음 번호.
    pub number: Option<i32>,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub duration_seconds: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub materials: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 회차 여러 개 추가(직접 입력·CSV 일괄 등록, 최대 1000개).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddEpisodes {
    pub episodes: Vec<NewEpisode>,
}

/// 추가한 수(같은 요청 id로 이미 있던 것은 빠짐).
#[derive(Debug, Serialize, ToSchema)]
pub struct EpisodesAdded {
    pub added: i32,
}

/// 회차 고치기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEpisode {
    pub number: i32,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub duration_seconds: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub materials: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 진도 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub last_position_seconds: Option<i32>,
    #[serde(default)]
    pub note: String,
    /// 사용자 시간대의 오늘(완료한 날). 없으면 서버 날짜.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub today: Option<Date>,
}

/// 함께 수강하는 사람의 진도 집계(메모·위치·시간은 없음).
#[derive(Debug, Serialize, ToSchema)]
pub struct Participant {
    pub user_id: UserId,
    pub display_name: String,
    pub done: i32,
    pub in_progress: i32,
    /// 계획일이 지났는데 완료하지 않은 회차 수.
    pub overdue: i32,
    pub done_seconds: i64,
}

/// 오늘(사용자 시간대의 날짜).
#[derive(Debug, Deserialize, IntoParams)]
pub struct TodayQuery {
    #[param(value_type = String, format = Date)]
    #[serde(with = "day")]
    pub today: Date,
}

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
);

/// 강좌(볼 수 있는 사람). 강의 모듈의 항목이 아니면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/course", operation_id = "get_course",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Course), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn get(
    State(state): State<AppState>,
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
                p.status, p.planned_on, p.completed_on, p.watch_count, p.last_position_seconds, p.note
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
        })
        .collect();
    Ok(Json(Course {
        details,
        episodes,
        can_write: !archived && matches!(access.as_str(), "owner" | "write"),
    }))
}

/// 강좌 정보 고치기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/items/{item_id}/course", operation_id = "set_course_details",
    params(("item_id" = String, Path, format = Uuid)), request_body = CourseDetails,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "courses")]
pub async fn set_details(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CourseDetails>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_course_details($1, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(item)
        .bind(&input.subject)
        .bind(&input.instructor)
        .bind(&input.platform)
        .bind(&input.url)
        .bind(&input.textbook)
        .bind(input.starts_on)
        .bind(input.expires_on)
        .bind(input.target_on)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 회차 추가(쓸 수 있는 사람). 직접 입력과 CSV 일괄 등록이 같은 길을 쓴다.
#[utoipa::path(post, path = "/api/items/{item_id}/episodes", operation_id = "add_episodes",
    params(("item_id" = String, Path, format = Uuid)), request_body = AddEpisodes,
    responses((status = 200, body = EpisodesAdded), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "courses")]
pub async fn add_episodes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<AddEpisodes>,
) -> Result<Json<EpisodesAdded>, AppError> {
    let rows = serde_json::to_value(&input.episodes)
        .map_err(|e| AppError::BadRequest(format!("episodes: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    let added: i32 = sqlx::query_scalar("SELECT app_add_episodes($1, $2)")
        .bind(item)
        .bind(rows)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(EpisodesAdded { added }))
}

/// 회차 고치기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/episodes/{episode_id}", operation_id = "update_episode",
    params(("episode_id" = String, Path, format = Uuid)), request_body = UpdateEpisode,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "courses")]
pub async fn update_episode(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
    Json(input): Json<UpdateEpisode>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_update_episode($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(episode)
        .bind(input.number)
        .bind(&input.title)
        .bind(&input.chapter)
        .bind(input.duration_seconds)
        .bind(&input.url)
        .bind(&input.materials)
        .bind(input.planned_on)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 회차 지우기(쓸 수 있는 사람). 모든 사람의 그 회차 진도도 지워진다.
#[utoipa::path(delete, path = "/api/episodes/{episode_id}", operation_id = "delete_episode",
    params(("episode_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn delete_episode(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_episode($1)")
        .bind(episode)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 내 진도 바꾸기(강좌를 볼 수 있으면 누구나 자기 것). 완료는 직접 표시한다.
#[utoipa::path(put, path = "/api/episodes/{episode_id}/progress", operation_id = "set_episode_progress",
    params(("episode_id" = String, Path, format = Uuid)), request_body = SetProgress,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn set_progress(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
    Json(input): Json<SetProgress>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_episode_progress($1, $2, $3, $4, $5, $6)")
        .bind(episode)
        .bind(&input.status)
        .bind(input.planned_on)
        .bind(input.last_position_seconds)
        .bind(&input.note)
        .bind(input.today)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 함께 수강하는 사람들의 진도 집계(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/course/participants", operation_id = "course_participants",
    params(("item_id" = String, Path, format = Uuid), TodayQuery),
    responses((status = 200, body = [Participant]), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn participants(
    State(state): State<AppState>,
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
