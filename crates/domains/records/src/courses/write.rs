//! 학습 변경 명령.

use super::{
    AddEpisodes, CourseDetails, EpisodeWriteConflict, EpisodesAdded, SetProgress, UpdateEpisode,
};
use crate::{RecordsState, validation};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::ItemId,
};
use fvoci_db::new_ids;
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};
use uuid::Uuid;

/// 강좌 정보 고치기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/items/{item_id}/course", operation_id = "set_course_details",
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC), ("item_id" = String, Path, format = Uuid)), request_body = CourseDetails,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "courses")]
pub async fn set_details(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CourseDetails>,
) -> Result<StatusCode, AppError> {
    input.validate()?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_course_details($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)")
        .bind(item)
        .bind(&input.subject)
        .bind(&input.instructor)
        .bind(&input.platform)
        .bind(&input.url)
        .bind(&input.textbook)
        .bind(input.starts_on)
        .bind(input.expires_on)
        .bind(input.target_on)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(|error| validation::from_db(error, "course"))?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 회차 추가(쓸 수 있는 사람). 직접 입력과 CSV 일괄 등록이 같은 길을 쓴다.
#[utoipa::path(post, path = "/api/items/{item_id}/episodes", operation_id = "add_episodes",
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC), ("item_id" = String, Path, format = Uuid)), request_body = AddEpisodes,
    responses((status = 200, body = EpisodesAdded), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "courses")]
pub async fn add_episodes(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<AddEpisodes>,
) -> Result<Json<EpisodesAdded>, AppError> {
    input.validate()?;
    let rows = serde_json::to_value(&input.episodes)
        .map_err(|e| validation::invalid("episodes", format!("episodes: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    // 회차마다 하나, 그 뒤 감사 기록 하나.
    let ids = new_ids(input.episodes.len() + 1);
    let added: i32 = sqlx::query_scalar("SELECT app_add_episodes($1, $2, $3)")
        .bind(item)
        .bind(rows)
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(|error| validation::from_db(error, "course"))?;
    tx.commit().await?;
    Ok(Json(EpisodesAdded { added }))
}

/// 회차 고치기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/episodes/{episode_id}", operation_id = "update_episode",
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC), ("episode_id" = String, Path, format = Uuid)), request_body = UpdateEpisode,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = EpisodeWriteConflict)), tag = "courses")]
pub async fn update_episode(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
    Json(input): Json<UpdateEpisode>,
) -> Result<Response, AppError> {
    input.validate()?;
    let sent = validation::sent(&input)?;
    let mut tx = state.db.begin_as(user).await?;
    let saved = sqlx::query("SELECT app_update_episode($1, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(episode)
        .bind(input.version)
        .bind(input.number)
        .bind(&input.title)
        .bind(&input.chapter)
        .bind(input.duration_seconds)
        .bind(&input.url)
        .bind(&input.materials)
        .bind(input.planned_on)
        .execute(tx.conn())
        .await;
    if let Err(error) = saved {
        drop(tx);
        if !validation::is_stale(&error, "episode") {
            return Err(validation::from_db(error, "episode"));
        }
        let mut tx = state.db.begin_as(user).await?;
        let current = super::load_episode(&mut tx, episode).await?;
        tx.commit().await?;
        return Ok(
            StaleWrite::new("conflict", "the episode changed; reload it", current, &sent)
                .into_response(),
        );
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// 회차 지우기(쓸 수 있는 사람). 모든 사람의 그 회차 진도도 지워진다.
#[utoipa::path(delete, path = "/api/episodes/{episode_id}", operation_id = "delete_episode",
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC), ("episode_id" = String, Path, format = Uuid)),
    responses((status = 400, body = ErrorBody), (status = 409, body = ErrorBody), (status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn delete_episode(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_episode($1, $2)")
        .bind(episode)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 내 진도 바꾸기(강좌를 볼 수 있으면 누구나 자기 것). 완료는 직접 표시한다.
#[utoipa::path(put, path = "/api/episodes/{episode_id}/progress", operation_id = "set_episode_progress",
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC), ("episode_id" = String, Path, format = Uuid)), request_body = SetProgress,
    responses((status = 409, body = ErrorBody), (status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "courses")]
pub async fn set_progress(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(episode): Path<Uuid>,
    Json(input): Json<SetProgress>,
) -> Result<StatusCode, AppError> {
    input.validate()?;
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
        .map_err(|error| validation::from_db(error, "course"))?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
