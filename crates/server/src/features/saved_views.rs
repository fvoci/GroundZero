//! 저장한 보기 (D-31). 항목 업무 화면의 보기 방식·필터·정렬을 사람마다 이름을 붙여 저장한다(본인만 읽음).
//! 보기 내용은 화면이 해석하는 JSON이고 서버는 크기·형식만 확인한다. 새로 만들기와 기존 보기 고치기를 나누고, 고치기는
//! 읽은 판(`base_version`)을 확인한다(D-103): 다른 탭이 그 뒤 저장한 설정을 옛 요청·재시도가 덮지 못한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, is_unique_violation};
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
    /// 고칠 때마다 1 오른다. 고치기의 `base_version`으로 보낸다.
    pub version: i32,
}

/// 새 보기. 같은 이름이 이미 있으면 만들지 않고 409(`exists`)와 그 보기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateView {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub name: String,
    #[schema(value_type = Object)]
    pub config: Map<String, Value>,
}

/// 기존 보기 고치기. `base_version`이 지났으면 409(`conflict`)와 최신 보기. 같은 `request_id`의 재시도는 이미 반영한
/// 결과를 돌려준다(마지막 고치기의 요청 id만 알아본다).
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateView {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub base_version: i32,
    #[schema(value_type = Object)]
    pub config: Map<String, Value>,
}

/// 409 응답: 이름이 이미 있거나(`exists`) 읽은 판이 지났다(`conflict`). 아무것도 바꾸지 않았고 `current`가 지금 보기다.
#[derive(Debug, Serialize, ToSchema)]
pub struct SavedViewConflict {
    pub error: &'static str,
    pub current: SavedView,
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
    let visible: bool = sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let rows: Vec<(Uuid, String, Value, i32)> = sqlx::query_as(
        "SELECT id, name, config, version FROM saved_views WHERE item_id = $1 ORDER BY name, created_at",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, config, version)| SavedView {
                id,
                name,
                config,
                version,
            })
            .collect(),
    ))
}

fn checked_config(config: Map<String, Value>) -> Result<Value, AppError> {
    let config = Value::Object(config);
    if config.to_string().len() > 4000 {
        return Err(AppError::BadRequest("the view is too large".into()));
    }
    Ok(config)
}

async fn load(tx: &mut UserTx, id: Uuid) -> Result<SavedView, AppError> {
    let (name, config, version): (String, Value, i32) =
        sqlx::query_as("SELECT name, config, version FROM saved_views WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    Ok(SavedView {
        id,
        name,
        config,
        version,
    })
}

fn conflict(error: &'static str, current: SavedView) -> Response {
    (
        StatusCode::CONFLICT,
        Json(SavedViewConflict { error, current }),
    )
        .into_response()
}

/// 새 보기(그 항목을 볼 수 있는 사람). 같은 이름이 이미 있으면 바꾸지 않고 409와 그 보기(고치려면 `PUT /api/views/{id}`).
/// 같은 `request_id`는 처음 만든 보기를 돌려준다.
#[utoipa::path(post, path = "/api/items/{item_id}/views", operation_id = "create_view",
    params(("item_id" = String, Path, format = Uuid)), request_body = CreateView,
    responses((status = 201, body = SavedView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = SavedViewConflict)),
    tag = "tasks")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateView>,
) -> Result<Response, AppError> {
    let name = input.name.trim().to_owned();
    if !(1..=100).contains(&name.chars().count()) {
        return Err(AppError::BadRequest(
            "a view needs a name of 1 to 100 characters".into(),
        ));
    }
    let config = checked_config(input.config)?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let (id, outcome): (Uuid, String) =
            sqlx::query_as("SELECT view_id, outcome FROM app_create_view($1, $2, $3, $4, $5)")
                .bind(input.request_id)
                .bind(item)
                .bind(&name)
                .bind(&config)
                .bind(crate::db::new_ids(1))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let view = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok::<_, AppError>((view, outcome))
    };
    // 같은 요청 id나 같은 이름을 동시에 만들면 한쪽이 유일성 위반으로 끝난다. 다시 보면 처음 것을 돌려주거나 409다.
    let (view, outcome) = match run().await {
        Err(AppError::Database(e)) if is_unique_violation(&e) => run().await?,
        other => other?,
    };
    Ok(if outcome == "exists" {
        conflict("exists", view)
    } else {
        (StatusCode::CREATED, Json(view)).into_response()
    })
}

/// 내 보기 고치기. 읽은 판이 지났으면(다른 탭이 저장함) 아무것도 바꾸지 않고 409와 최신 보기. 확인과 고치기는 같은
/// 트랜잭션이다. 마지막 고치기와 같은 `request_id`면 다시 쓰지 않고 그 결과를 돌려준다.
#[utoipa::path(put, path = "/api/views/{view_id}", operation_id = "update_view",
    params(("view_id" = String, Path, format = Uuid)), request_body = UpdateView,
    responses((status = 200, body = SavedView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = SavedViewConflict)),
    tag = "tasks")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(view): Path<Uuid>,
    Json(input): Json<UpdateView>,
) -> Result<Response, AppError> {
    let config = checked_config(input.config)?;
    let mut tx = state.db.begin_as(user).await?;
    let outcome: String = sqlx::query_scalar("SELECT app_update_view($1, $2, $3, $4)")
        .bind(input.request_id)
        .bind(view)
        .bind(input.base_version)
        .bind(&config)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let current = load(&mut tx, view).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        conflict("conflict", current)
    } else {
        Json(current).into_response()
    })
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
