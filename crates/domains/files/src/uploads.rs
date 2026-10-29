//! 업로드 시작·재개·조각 수신·중단.
use crate::{
    FilesState,
    model::{CreateUpload, EXPECTED_USER_DOC, PART_SHA256_HEADER, UploadView},
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{FileId, UploadId};
use fvoci_db::{UserTx, new_ids};
use fvoci_http::session::{CurrentUser, ExpectedUser};
use fvoci_storage::sha256_hex;
use time::OffsetDateTime;
use uuid::Uuid;
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct UploadRow {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) size: i64,
    pub(crate) chunk_bytes: i64,
    pub(crate) storage_key: String,
    pub(crate) storage_upload_id: Option<String>,
    pub(crate) state: String,
    pub(crate) failure: Option<String>,
    pub(crate) version_id: Option<Uuid>,
    pub(crate) expires_at: OffsetDateTime,
    /// 마치기 시도의 점유를 구별한다(`app_begin_complete`가 그 시도에 매긴 시각, 0088).
    pub(crate) state_changed_at: OffsetDateTime,
}

pub(crate) const UPLOAD_COLUMNS: &str = "id, name, size, chunk_bytes, storage_key, storage_upload_id, state, failure, \
                              version_id, expires_at, state_changed_at";

pub(crate) fn parts_total(size: i64, chunk: i64) -> i64 {
    ((size + chunk - 1) / chunk).max(1)
}

pub(crate) async fn my_upload(tx: &mut UserTx, id: UploadId) -> Result<UploadRow, AppError> {
    sqlx::query_as(&format!("SELECT {UPLOAD_COLUMNS} FROM app_my_upload($1)"))
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)
}

pub(crate) async fn view(tx: &mut UserTx, row: UploadRow) -> Result<UploadView, AppError> {
    let received: Vec<i32> =
        sqlx::query_scalar("SELECT number FROM upload_parts WHERE upload_id = $1 ORDER BY number")
            .bind(row.id)
            .fetch_all(tx.conn())
            .await?;
    let done: Option<(FileId, i32)> = match row.version_id {
        Some(v) => {
            sqlx::query_as("SELECT file_id, version FROM file_versions WHERE id = $1")
                .bind(v)
                .fetch_optional(tx.conn())
                .await?
        }
        None => None,
    };
    Ok(UploadView {
        id: UploadId(row.id),
        state: row.state,
        name: row.name,
        size: row.size,
        chunk_bytes: row.chunk_bytes,
        parts_total: parts_total(row.size, row.chunk_bytes),
        received,
        expires_at: row.expires_at,
        file_id: done.map(|(f, _)| f),
        version: done.map(|(_, v)| v),
        failure: row.failure,
    })
}

/// 업로드 시작(같은 요청 id면 같은 업로드). 쓸 수 있는 문서·항목·파일에만. 크기·저장량 한도를 넘으면 400·409.
///
/// 기대 사용자 머리가 필요하다(`ExpectedUser`). 웹은 만료된 업로드를 스스로 다시 시작하므로, 그사이 다른 탭에서 계정을
/// 바꿨으면 새 계정의 업로드를 만들지 않고 409 `account_changed`.
#[utoipa::path(post, path = "/api/uploads", operation_id = "create_upload", request_body = CreateUpload,
    params(("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_USER_DOC)),
    responses((status = 201, body = UploadView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "저장량 한도 초과(`invalid_state`) 또는 `account_changed`(기대 사용자가 세션의 사용자와 다름, 아무것도 만들지 않음)")),
    tag = "files")]
pub async fn create_upload(
    State(state): State<FilesState>,
    ExpectedUser(user): ExpectedUser,
    Json(input): Json<CreateUpload>,
) -> Result<(StatusCode, Json<UploadView>), AppError> {
    let limits = state.limits;
    let (kind, target) = match (&input.file_id, &input.target) {
        (Some(_), None) => (None, None),
        (None, Some(t)) => (Some(t.kind.clone()), Some(t.id)),
        _ => {
            return Err(AppError::invalid(
                "target",
                fvoci_core::error::InvalidReason::InvalidFormat,
                None,
                "give either file_id (new version) or target (new file)",
            ));
        }
    };
    if let Some(target) = &input.target
        && !matches!(target.kind.as_str(), "document" | "item")
    {
        return Err(AppError::invalid(
            "target.kind",
            fvoci_core::error::InvalidReason::InvalidFormat,
            None,
            "target kind must be document or item",
        ));
    }
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar(
        "SELECT app_create_upload($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(input.request_id)
    .bind(input.file_id)
    .bind(kind)
    .bind(target)
    .bind(&input.name)
    .bind(&input.note)
    .bind(&input.content_type)
    .bind(input.size)
    .bind(i64::try_from(limits.chunk_bytes).unwrap_or(i64::MAX))
    .bind(i32::try_from(limits.upload_ttl.as_secs()).unwrap_or(i32::MAX))
    .bind(i64::try_from(limits.max_file_bytes).unwrap_or(i64::MAX))
    .bind(i64::try_from(limits.max_storage_bytes).unwrap_or(i64::MAX))
    .bind(new_ids(1))
    .fetch_one(tx.conn())
    .await
    .map_err(crate::validation::from_file_function)?;
    let row = my_upload(&mut tx, UploadId(id)).await?;
    tx.commit().await?;

    if row.storage_upload_id.is_none() && row.state == "uploading" {
        let started = state.storage.begin(&row.storage_key).await?;
        let mut tx = state.db.begin_as(user).await?;
        sqlx::query("SELECT app_set_storage_upload($1, $2)")
            .bind(id)
            .bind(&started)
            .execute(tx.conn())
            .await
            .map_err(crate::validation::from_file_function)?;
        let row = my_upload(&mut tx, UploadId(id)).await?;
        tx.commit().await?;
        // 같은 요청이 동시에 와서 다른 쪽이 먼저 기록했으면 이쪽 저장소 업로드는 버린다.
        if row.storage_upload_id.as_deref() != Some(started.as_str()) {
            state.storage.abort(&row.storage_key, &started).await?;
        }
    }
    let mut tx = state.db.begin_as(user).await?;
    let row = my_upload(&mut tx, UploadId(id)).await?;
    let out = view(&mut tx, row).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// 업로드 상태(올린 사람만). 이어 올릴 때 받은 조각을 본다.
#[utoipa::path(get, path = "/api/uploads/{upload_id}", operation_id = "get_upload",
    params(("upload_id" = String, Path, format = Uuid)),
    responses((status = 200, body = UploadView), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn get_upload(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<UploadId>,
) -> Result<Json<UploadView>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let row = my_upload(&mut tx, id).await?;
    let out = view(&mut tx, row).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// 조각 하나 보내기(1부터). 본문은 날것 그대로, `x-content-sha256`에 본문의 SHA-256. 크기는 조각 크기(마지막만
/// 나머지)와 같아야 한다. 같은 번호를 다시 보내면 바꾼다.
#[utoipa::path(put, path = "/api/uploads/{upload_id}/parts/{number}", operation_id = "put_upload_part",
    params(("upload_id" = String, Path, format = Uuid), ("number" = i32, Path), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "보내면 세션 사용자와 비교한다(D-96). 생략할 수 있다."), ("x-content-sha256" = String, Header, description = "조각 본문의 SHA-256(16진수)")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "files")]
pub async fn put_part(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path((id, number)): Path<(UploadId, i32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let row = my_upload(&mut tx, id).await?;
    tx.commit().await?;
    if row.state != "uploading" || row.expires_at <= OffsetDateTime::now_utc() {
        return Err(AppError::Conflict("the upload is no longer open".into()));
    }
    let Some(storage_upload) = row.storage_upload_id.as_deref() else {
        return Err(AppError::Conflict("the upload has not started".into()));
    };
    let total = parts_total(row.size, row.chunk_bytes);
    if number < 1 || i64::from(number) > total {
        return Err(AppError::invalid(
            "number",
            fvoci_core::error::InvalidReason::OutOfRange,
            u64::try_from(total).ok(),
            format!("part number must be 1..={total}"),
        ));
    }
    let expected = if i64::from(number) == total {
        row.size - row.chunk_bytes * (total - 1)
    } else {
        row.chunk_bytes
    };
    if i64::try_from(body.len()).unwrap_or(i64::MAX) != expected {
        return Err(AppError::invalid(
            "size",
            fvoci_core::error::InvalidReason::InvalidFormat,
            u64::try_from(expected).ok(),
            format!("part {number} must be {expected} bytes"),
        ));
    }
    let claimed = headers
        .get(PART_SHA256_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            AppError::invalid(
                PART_SHA256_HEADER,
                fvoci_core::error::InvalidReason::Required,
                None,
                format!("missing {PART_SHA256_HEADER}"),
            )
        })?;
    let actual = sha256_hex(&body);
    if claimed != actual {
        return Err(AppError::invalid(
            PART_SHA256_HEADER,
            fvoci_core::error::InvalidReason::InvalidFormat,
            Some(64),
            "the part does not match its SHA-256; send it again",
        ));
    }
    let number_u32 = u32::try_from(number).unwrap_or_default();
    let etag = state
        .storage
        .put_part(&row.storage_key, storage_upload, number_u32, body)
        .await?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_record_part($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(number)
        .bind(expected)
        .bind(&actual)
        .bind(&etag)
        .execute(tx.conn())
        .await
        .map_err(crate::validation::from_file_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 업로드 그만두기(올린 사람). 받은 조각은 백그라운드 작업이 지운다.
#[utoipa::path(delete, path = "/api/uploads/{upload_id}", operation_id = "abort_upload",
    params(("upload_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "보내면 세션 사용자와 비교한다(D-96). 생략할 수 있다.")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "files")]
pub async fn abort_upload(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<UploadId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_abort_upload($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(crate::validation::from_file_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::parts_total;
    #[test]
    fn counts_parts() {
        assert_eq!(parts_total(1, 5), 1);
        assert_eq!(parts_total(5, 5), 1);
        assert_eq!(parts_total(6, 5), 2);
    }
}
