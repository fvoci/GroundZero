//! 업무 명령: 만들기, 고치기(승인한 재배치 포함), 칸반 이동, 상위 업무, 지우기.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use super::edit::{Edit, Saved, save_edit};
use super::model::{
    CheckedFields, CreateTask, EXPECTED_TASK_USER_DOC, MoveTask, SetTaskParent, TaskConflict,
    TaskDetail, TaskWriteConflict, UpdateTask, check_fields, validate_status,
};
use super::read::load_detail;

use crate::state::WorkState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::ItemId;
use fvoci_core::trash::Trashed;
use fvoci_db::{is_unique_violation, new_ids};
use fvoci_http::session::ExpectedUser;

/// 업무 만들기(항목 쓰기 권한). 모든 칸을 한 트랜잭션에서 확인·저장한다(D-105). 같은 `request_id`는 같은 업무를 돌려주고,
/// 그 업무를 지웠으면(휴지통) 409 `task_deleted`(항목을 볼 수 없으면 404).
#[utoipa::path(post, path = "/api/items/{item_id}/tasks", operation_id = "create_task",
    params(("item_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = CreateTask,
    responses((status = 201, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; task_deleted: 같은 요청으로 만든 업무가 휴지통에 있음; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn create(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskDetail>), AppError> {
    let fields = check_fields(
        &state,
        &input.title,
        &input.description,
        input.start_date.as_deref(),
        input.due_date.as_deref(),
        input.estimate_minutes,
        &input.tags,
    )?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: Uuid = sqlx::query_scalar(
            "SELECT app_create_task($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(item)
        .bind(input.request_id)
        .bind(&fields.title)
        .bind(input.parent_id)
        .bind(&fields.description)
        .bind(input.status.as_deref().map(validate_status).transpose()?)
        .bind(input.priority.as_str())
        .bind(fields.start)
        .bind(fields.due)
        .bind(input.estimate_minutes)
        .bind(&fields.tags)
        .bind(&input.assignee_ids)
        .bind(input.milestone)
        // 업무·감사·활동과 담당자마다 감사·알림의 상한.
        .bind(new_ids(3 + 2 * input.assignee_ids.len()))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
        let detail = load_detail(&mut tx, id).await?;
        tx.commit().await?;
        Ok(detail)
    };
    match run().await {
        Ok(detail) => Ok((StatusCode::CREATED, Json(detail))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 업무 고치기(쓰기 권한). 기준 버전이 지났으면 409와 서버 최신본(입력은 클라이언트가 보존).
#[utoipa::path(put, path = "/api/tasks/{task_id}", operation_id = "update_task",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = UpdateTask,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = TaskWriteConflict, description = "conflict·shift_conflict면 최신 업무 포함; account_changed·invalid_state면 ErrorBody. 아무것도 바꾸지 않음")), tag = "tasks")]
pub async fn update(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateTask>,
) -> Result<Response, AppError> {
    let CheckedFields {
        title,
        description,
        start,
        due,
        tags,
    } = check_fields(
        &state,
        &input.title,
        &input.description,
        input.start_date.as_deref(),
        input.due_date.as_deref(),
        input.estimate_minutes,
        &input.tags,
    )?;
    let edit = Edit {
        title,
        description,
        status: validate_status(&input.status)?,
        priority: input.priority,
        start,
        due,
        estimate_minutes: input.estimate_minutes,
        tags,
        assignee_ids: input.assignee_ids,
        milestone: input.milestone,
    };
    let saved = save_edit(
        &state,
        user,
        id,
        input.base_version,
        &edit,
        &input.successor_shifts,
    )
    .await?;
    let mut tx = state.db.begin_as(user).await?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    let error = match saved {
        Saved::Updated => return Ok(Json(detail).into_response()),
        Saved::Conflict => "conflict",
        Saved::ShiftConflict => "shift_conflict",
    };
    Ok((
        StatusCode::CONFLICT,
        Json(TaskConflict {
            error,
            current: detail,
        }),
    )
        .into_response())
}

/// 칸반 이동(쓰기 권한). `base_move_version`이 없으면 판과 상관없이 바로 반영한다(D-79). 있으면 행을 잠근 같은
/// 트랜잭션에서 이동 판을 비교하고, 그사이 열·위치가 바뀌었으면 아무것도 바꾸지 않고 409 `move_conflict`와 지금
/// 업무를 돌려준다(D-89).
#[utoipa::path(put, path = "/api/tasks/{task_id}/position",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = MoveTask,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = TaskWriteConflict, description = "move_conflict면 최신 업무 포함; account_changed·invalid_state면 ErrorBody. 아무것도 바꾸지 않음")), tag = "tasks")]
pub async fn move_task(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<MoveTask>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let moved = sqlx::query("SELECT app_move_task($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(validate_status(&input.status)?)
        .bind(input.before_id)
        .bind(input.base_move_version)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await;
    if let Err(err) = moved {
        if err
            .as_database_error()
            .and_then(|db| db.code())
            .is_some_and(|code| code == "FV003")
        {
            // 아무것도 바뀌지 않았다(트랜잭션을 버린다). 지금 업무를 새 트랜잭션으로 읽어 함께 돌려준다.
            drop(tx);
            let mut fresh = state.db.begin_as(user).await?;
            let current = load_detail(&mut fresh, id).await?;
            fresh.commit().await?;
            return Ok((
                StatusCode::CONFLICT,
                Json(TaskConflict {
                    error: "move_conflict",
                    current,
                }),
            )
                .into_response());
        }
        return Err(AppError::from_db_function(err));
    }
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail).into_response())
}

/// 상위 업무 바꾸기(같은 항목, 한 단계).
#[utoipa::path(put, path = "/api/tasks/{task_id}/parent", operation_id = "set_task_parent",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = SetTaskParent,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn set_parent(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SetTaskParent>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_task_parent($1, $2, $3)")
        .bind(id)
        .bind(input.parent_id)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 업무 지우기(하위 업무까지, 쓰기 권한).
#[utoipa::path(delete, path = "/api/tasks/{task_id}", operation_id = "trash_task",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    responses((status = 200, body = Trashed), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "trash")]
pub async fn trash_task(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Trashed>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 휴지통 묶음과 감사 기록의 id 2개.
    let batch_id: Uuid = sqlx::query_scalar("SELECT app_trash_task($1, $2)")
        .bind(id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(Trashed { batch_id }))
}
