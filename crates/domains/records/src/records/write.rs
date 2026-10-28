//! 기록 생성·수정·삭제 명령.

use super::{EditRecord, NewRecord, RecordVersion, validate};
use crate::RecordsState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::ItemId,
};
use fvoci_db::{is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;
use serde_json::Value;
use uuid::Uuid;

/// 기록 만들기(항목에 쓸 수 있는 사람). 같은 요청 id면 같은 기록.
#[utoipa::path(post, path = "/api/items/{item_id}/records", operation_id = "create_record",
    params(("item_id" = String, Path, format = Uuid)), request_body = NewRecord,
    responses((status = 201, body = Object), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "records")]
pub async fn create(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<NewRecord>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let (fields, files) = validate(
            &mut tx,
            item,
            &input.kind,
            &input.title,
            input.fields.clone(),
            None,
            &input.files,
        )
        .await?;
        let id: Uuid =
            sqlx::query_scalar("SELECT app_create_record($1, $2, $3, $4, $5, $6, $7, $8, $9)")
                .bind(input.request_id)
                .bind(item)
                .bind(&input.kind)
                .bind(input.title.trim())
                .bind(input.occurred_on)
                .bind(&input.body)
                .bind(fields)
                .bind(files)
                .bind(new_ids(2))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        tx.commit().await?;
        Ok::<Uuid, AppError>(id)
    };
    // 같은 요청 id의 앞 요청이 아직 끝나지 않았으면 고유 제약에서 기다린 뒤 23505로 끝난다. 그때는 한 번 더 돌려 앞
    // 요청이 만든 기록을 돌려준다(감사 S10, 전에는 500).
    let id = match run().await {
        Ok(id) => id,
        Err(AppError::Database(e)) if is_unique_violation(&e) => run().await?,
        Err(e) => return Err(e),
    };
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// 기록 고치기(항목에 쓸 수 있는 사람). 읽은 뒤 바뀌었으면 409.
#[utoipa::path(put, path = "/api/records/{record_id}", operation_id = "update_record",
    params(("record_id" = String, Path, format = Uuid)), request_body = EditRecord,
    responses((status = 200, body = RecordVersion), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "records")]
pub async fn update(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(record): Path<Uuid>,
    Json(input): Json<EditRecord>,
) -> Result<Json<RecordVersion>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let found: Option<(ItemId, String, Value, i32)> =
        sqlx::query_as("SELECT item_id, kind, fields, version FROM item_records WHERE id = $1")
            .bind(record)
            .fetch_optional(tx.conn())
            .await?;
    let (item, kind, stored, stored_version) = found.ok_or(AppError::NotFound)?;
    // 이미 저장된 id를 다시 확인하지 않는 기준(`stored`)은 바꿀 그 판이어야 한다. 읽은 판이 보낸 판과 다르면 바로 409다
    // (아직 커밋되지 않은 다음 판을 짐작해 보내면 옛 판의 id로 확인을 건너뛰고, 다음 판이 커밋된 뒤 판이 맞아 저장했다).
    // 판은 늘기만 하므로, 여기와 아래 app_update_record(행 잠금)에서 모두 같으면 그 사이 바뀐 것이 없다.
    if stored_version != input.version {
        return Err(AppError::Conflict("the record changed; reload it".into()));
    }
    let (fields, files) = validate(
        &mut tx,
        item,
        &kind,
        &input.title,
        input.fields,
        stored.as_object(),
        &input.files,
    )
    .await?;
    let version: i32 = sqlx::query_scalar("SELECT app_update_record($1, $2, $3, $4, $5, $6, $7)")
        .bind(record)
        .bind(input.version)
        .bind(input.title.trim())
        .bind(input.occurred_on)
        .bind(&input.body)
        .bind(fields)
        .bind(files)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(RecordVersion { version }))
}

/// 기록 지우기(만든 사람 또는 항목 소유 권한자).
#[utoipa::path(delete, path = "/api/records/{record_id}", operation_id = "delete_record",
    params(("record_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "records")]
pub async fn delete(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(record): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_record($1, $2)")
        .bind(record)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
