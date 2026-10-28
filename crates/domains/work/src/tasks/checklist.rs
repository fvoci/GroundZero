//! 업무 체크리스트 명령: 덧붙이기, 항목 고치기·지우기.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::model::{
    AddChecklist, EXPECTED_TASK_USER_DOC, MAX_CHECKLIST_BATCH, NewChecklistItem, TaskDetail,
    UpdateChecklistItem,
};
use super::read::load_detail;
use crate::state::WorkState;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::validate::validate_title;
use fvoci_db::new_ids;
use fvoci_http::session::ExpectedUser;

/// 체크리스트 덧붙이기(쓰기 권한). 항목별 `request_id`로 재시도해도 중복되지 않는다.
#[utoipa::path(post, path = "/api/tasks/{task_id}/checklist",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = AddChecklist,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn add_checklist(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AddChecklist>,
) -> Result<Json<TaskDetail>, AppError> {
    if input.items.len() > MAX_CHECKLIST_BATCH {
        return Err(AppError::invalid(
            "items",
            InvalidReason::TooMany,
            Some(MAX_CHECKLIST_BATCH as u64),
            format!("at most {MAX_CHECKLIST_BATCH} items at once"),
        ));
    }
    let mut items = Vec::with_capacity(input.items.len());
    for item in input.items {
        let text = validate_title(&item.text, state.max_title_chars, "text")?;
        items.push(NewChecklistItem { text, ..item });
    }
    let payload = serde_json::to_value(&items)
        .map_err(|e| AppError::BadRequest(format!("invalid items: {e}")))?;
    // 줄마다 하나.
    let ids = new_ids(items.len());
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_add_checklist_items($1, $2, $3)")
        .bind(id)
        .bind(payload)
        .bind(ids)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 체크리스트 항목 고치기(쓰기 권한). 버전 없이 바로 반영하되 보낸 칸만 바꾼다: 문구만 고치면 그 사이 다른 사람이 바꾼
/// 완료 상태를, 완료만 바꾸면 고친 문구를 되돌리지 않는다.
#[utoipa::path(put, path = "/api/checklist-items/{item_id}",
    params(("item_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = UpdateChecklistItem,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn update_checklist_item(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateChecklistItem>,
) -> Result<StatusCode, AppError> {
    if input.text.is_none() && input.checked.is_none() {
        return Err(AppError::BadRequest("text or checked is required".into()));
    }
    let text = input
        .text
        .as_deref()
        .map(|t| validate_title(t, state.max_title_chars, "text"))
        .transpose()?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_update_checklist_item($1, $2, $3)")
        .bind(id)
        .bind(&text)
        .bind(input.checked)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 체크리스트 항목 지우기(쓰기 권한).
#[utoipa::path(delete, path = "/api/checklist-items/{item_id}",
    params(("item_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn delete_checklist_item(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_checklist_item($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
