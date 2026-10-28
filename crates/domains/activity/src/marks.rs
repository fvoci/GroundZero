//! 본인의 고정·즐겨찾기 읽기와 명령.
use crate::ActivityState;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
pub use fvoci_core::marks::MarkTarget;
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// SQL `app_set_mark`의 종류별 현재 보이는 표시 수 상한.
const MARK_LIMIT: u64 = 100;

/// 고정 또는 즐겨찾기.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Pin,
    Favorite,
}

impl MarkKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pin => "pin",
            Self::Favorite => "favorite",
        }
    }
}

/// 고정·즐겨찾기 켜기·끄기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetMark {
    /// 종류마다 현재 보이는 대상 100개까지 표시할 수 있다.
    pub mark: MarkKind,
    pub on: bool,
}

/// 한 대상의 표시 상태.
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkState {
    pub pin: bool,
    pub favorite: bool,
}

/// 대상의 고정·즐겨찾기 켜기·끄기(본인). 켤 때 볼 수 없으면 404.
#[utoipa::path(put, path = "/api/me/marks/{target_kind}/{target_id}", operation_id = "set_mark",
    params(("target_kind" = MarkTarget, Path), ("target_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<Uuid>, Header, description = EXPECTED_USER_IF_SENT_DOC)),
    request_body = SetMark,
    responses((status = 409, body = ErrorBody), (status = 200, body = MarkState), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "home")]
pub async fn set_mark(
    State(state): State<ActivityState>,
    CurrentUser(user): CurrentUser,
    Path((kind, id)): Path<(MarkTarget, Uuid)>,
    Json(input): Json<SetMark>,
) -> Result<Json<MarkState>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_mark($1, $2, $3, $4)")
        .bind(input.mark.as_str())
        .bind(kind.as_str())
        .bind(id)
        .bind(input.on)
        .execute(tx.conn())
        .await
        .map_err(|err| match AppError::from_db_function(err) {
            AppError::BadRequest(message) if message == "too many marks" => {
                AppError::invalid("mark", InvalidReason::TooMany, Some(MARK_LIMIT), message)
            }
            error => error,
        })?;
    let marks = mark_state(&mut tx, kind, id).await?;
    tx.commit().await?;
    Ok(Json(marks))
}

async fn mark_state(
    tx: &mut fvoci_db::UserTx,
    kind: MarkTarget,
    id: Uuid,
) -> Result<MarkState, AppError> {
    let (pin, favorite): (bool, bool) = sqlx::query_as(
        "SELECT bool_or(mark = 'pin') IS TRUE, bool_or(mark = 'favorite') IS TRUE
         FROM user_marks WHERE target_kind = $1 AND target_id = $2",
    )
    .bind(kind.as_str())
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(MarkState { pin, favorite })
}

/// 한 대상의 내 표시 상태.
#[utoipa::path(get, path = "/api/me/marks/{target_kind}/{target_id}", operation_id = "get_mark",
    params(("target_kind" = MarkTarget, Path), ("target_id" = String, Path, format = Uuid)),
    responses((status = 200, body = MarkState)), tag = "home")]
pub async fn get_mark(
    State(state): State<ActivityState>,
    CurrentUser(user): CurrentUser,
    Path((kind, id)): Path<(MarkTarget, Uuid)>,
) -> Result<Json<MarkState>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let marks = mark_state(&mut tx, kind, id).await?;
    tx.commit().await?;
    Ok(Json(marks))
}
