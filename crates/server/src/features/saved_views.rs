//! 저장한 보기 (D-31). 항목 업무 화면의 보기 방식·필터·정렬을 사람마다 이름을 붙여 저장한다(본인만 읽음).
//! 보기 내용은 화면이 해석하는 JSON이고 서버는 크기·형식만 확인한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::ItemId;

/// 저장한 보기.
#[derive(Debug, Serialize, ToSchema)]
pub struct SavedView {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub name: String,
    #[schema(value_type = Object)]
    pub config: Value,
}

/// 보기 저장. 같은 이름이면 내용을 바꾼다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveView {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub name: String,
    #[schema(value_type = Object)]
    pub config: Map<String, Value>,
}

/// 이 항목의 내 저장한 보기(이름순).
#[utoipa::path(get, path = "/api/items/{item_id}/views", operation_id = "saved_views",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [SavedView]), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<SavedView>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_item_access($1) IS NOT NULL")
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let rows: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "SELECT id, name, config FROM saved_views WHERE item_id = $1 ORDER BY name, created_at",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, config)| SavedView { id, name, config })
            .collect(),
    ))
}

/// 보기 저장(그 항목을 볼 수 있는 사람). 같은 이름이면 덮어쓴다.
#[utoipa::path(post, path = "/api/items/{item_id}/views", operation_id = "save_view",
    params(("item_id" = String, Path, format = Uuid)), request_body = SaveView,
    responses((status = 201, body = SavedView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn save(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<SaveView>,
) -> Result<(StatusCode, Json<SavedView>), AppError> {
    let name = input.name.trim().to_owned();
    if !(1..=100).contains(&name.chars().count()) {
        return Err(AppError::BadRequest(
            "a view needs a name of 1 to 100 characters".into(),
        ));
    }
    let config = Value::Object(input.config);
    if config.to_string().len() > 4000 {
        return Err(AppError::BadRequest("the view is too large".into()));
    }
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_save_view($1, $2, $3, $4)")
        .bind(input.request_id)
        .bind(item)
        .bind(&name)
        .bind(&config)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (name, config): (String, Value) =
        sqlx::query_as("SELECT name, config FROM saved_views WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(SavedView { id, name, config })))
}

/// 내 저장한 보기 지우기.
#[utoipa::path(delete, path = "/api/views/{view_id}", operation_id = "delete_view",
    params(("view_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(view): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_view($1)")
        .bind(view)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
