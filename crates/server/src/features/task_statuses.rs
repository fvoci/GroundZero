//! 항목별 업무 상태 정의(D-116). 열 이름·색·범주(todo/doing/done)·순서는 항목마다 정하고 업무는 상태 id로 열에 들어간다.
//!
//! 권한·불변식(마지막 상태·30개 상한·이름 중복·업무 있는 열 삭제 시 옮길 열 필수)은 정의자 함수
//! `app_task_statuses`·`app_create_task_status`·`app_update_task_status`·`app_delete_task_status`가 검사한다. 이 모듈은
//! 입력 검증과 응답 변환만 한다. DB 정의는 `migrations/202610250001_initial_state.sql`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::workspaces::validate_title;
use crate::http::AppState;
use crate::ids::ItemId;

/// 상태 이름 상한(글자 수). 열 머리에 들어가는 짧은 라벨이다.
const MAX_STATUS_NAME: usize = 50;

/// 상태 범주. 완료 판정과 기본 열 선택은 범주로 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StatusCategory {
    Todo,
    Doing,
    Done,
}

impl StatusCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Done => "done",
        }
    }

    fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "todo" => Ok(Self::Todo),
            "doing" => Ok(Self::Doing),
            "done" => Ok(Self::Done),
            other => Err(AppError::Config(format!(
                "unknown status category in database: {other}"
            ))),
        }
    }
}

/// 항목의 상태 한 줄.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskStatus {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub name: String,
    /// 색 토큰(빈 문자열이면 기본).
    pub color: String,
    /// 열 순서(오름차순).
    pub position: f64,
    pub category: StatusCategory,
    /// 이 열에 있는(지우지 않은) 업무 수.
    pub task_count: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NewTaskStatus {
    pub name: String,
    #[serde(default)]
    pub color: String,
    pub category: StatusCategory,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTaskStatus {
    pub name: Option<String>,
    pub color: Option<String>,
    pub category: Option<StatusCategory>,
    pub position: Option<f64>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct DeleteStatusQuery {
    /// 이 열의 업무를 옮길 같은 항목의 상태. 업무가 있으면 필수.
    #[param(value_type = Option<String>, format = Uuid)]
    pub move_to: Option<Uuid>,
}

/// 삭제 결과: 옮긴 업무 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeletedTaskStatus {
    pub moved: i32,
}

type StatusRow = (Uuid, String, String, f64, String, i32);

async fn list_rows(tx: &mut UserTx, item: ItemId) -> Result<Vec<TaskStatus>, AppError> {
    let rows: Vec<StatusRow> = sqlx::query_as(
        "SELECT id, name, color, position::double precision, category, task_count FROM app_task_statuses($1)",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    rows.into_iter()
        .map(|(id, name, color, position, category, task_count)| {
            Ok(TaskStatus {
                id,
                name,
                color,
                position,
                category: StatusCategory::parse(&category)?,
                task_count,
            })
        })
        .collect()
}

fn validate_color(raw: &str) -> Result<String, AppError> {
    let color = raw.trim();
    if color.chars().count() > 32 || color.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "color must be a short token without control characters".to_owned(),
        ));
    }
    Ok(color.to_owned())
}

/// 항목의 상태 목록(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/statuses", operation_id = "list_task_statuses",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [TaskStatus]), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<TaskStatus>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows = list_rows(&mut tx, item).await?;
    tx.commit().await?;
    Ok(Json(rows))
}

/// 상태 만들기(항목 소유 권한). 이름 중복은 409, 30개 상한은 400, 보관된 항목은 409.
#[utoipa::path(post, path = "/api/items/{item_id}/statuses", operation_id = "create_task_status",
    params(("item_id" = String, Path, format = Uuid)), request_body = NewTaskStatus,
    responses((status = 201, body = TaskStatus), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<NewTaskStatus>,
) -> Result<(StatusCode, Json<TaskStatus>), AppError> {
    let name = validate_title(&input.name, MAX_STATUS_NAME, "name")?;
    let color = validate_color(&input.color)?;
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_create_task_status($1, $2, $3, $4, $5)")
        .bind(item)
        .bind(&name)
        .bind(&color)
        .bind(input.category.as_str())
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let created = list_rows(&mut tx, item)
        .await?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// 상태 고치기(항목 소유 권한). 범주를 done으로 바꾸면 그 열 업무의 완료 시각이 채워진다.
#[utoipa::path(put, path = "/api/items/{item_id}/statuses/{status_id}", operation_id = "update_task_status",
    params(("item_id" = String, Path, format = Uuid), ("status_id" = String, Path, format = Uuid)),
    request_body = UpdateTaskStatus,
    responses((status = 200, body = TaskStatus), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((item, status)): Path<(ItemId, Uuid)>,
    Json(input): Json<UpdateTaskStatus>,
) -> Result<Json<TaskStatus>, AppError> {
    let name = input
        .name
        .as_deref()
        .map(|n| validate_title(n, MAX_STATUS_NAME, "name"))
        .transpose()?;
    let color = input.color.as_deref().map(validate_color).transpose()?;
    let mut tx = state.db.begin_as(user).await?;
    // 경로의 항목과 상태가 맞는지 먼저 본다(남의 항목 상태 id를 이 항목 경로로 고치지 못하게).
    let belongs = list_rows(&mut tx, item)
        .await?
        .into_iter()
        .any(|s| s.id == status);
    if !belongs {
        return Err(AppError::NotFound);
    }
    sqlx::query("SELECT app_update_task_status($1, $2, $3, $4, $5, $6)")
        .bind(status)
        .bind(name)
        .bind(color)
        .bind(input.category.map(StatusCategory::as_str))
        .bind(input.position)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let updated = list_rows(&mut tx, item)
        .await?
        .into_iter()
        .find(|s| s.id == status)
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(Json(updated))
}

/// 상태 지우기(항목 소유 권한). 업무가 있으면 `move_to`로 옮길 열이 필요하고(400), 마지막 상태는 지울 수 없다(409).
#[utoipa::path(delete, path = "/api/items/{item_id}/statuses/{status_id}", operation_id = "delete_task_status",
    params(("item_id" = String, Path, format = Uuid), ("status_id" = String, Path, format = Uuid), DeleteStatusQuery),
    responses((status = 200, body = DeletedTaskStatus), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((item, status)): Path<(ItemId, Uuid)>,
    Query(query): Query<DeleteStatusQuery>,
) -> Result<Json<DeletedTaskStatus>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let belongs = list_rows(&mut tx, item)
        .await?
        .into_iter()
        .any(|s| s.id == status);
    if !belongs {
        return Err(AppError::NotFound);
    }
    let moved: i32 = sqlx::query_scalar("SELECT app_delete_task_status($1, $2, $3)")
        .bind(status)
        .bind(query.move_to)
        .bind(new_ids(1))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(DeletedTaskStatus { moved }))
}
