//! 파일 첨부·버전·업로드 (D-06, D-35, D-52).
//!
//! 파일은 문서나 항목에 붙고 그 권한을 따른다. 업로드는 세션을 만들고(같은 요청 id면 같은 세션) 조각마다 보낸
//! 뒤(조각마다 SHA-256 확인, 끊기면 받은 조각을 보고 이어 보냄) 마친다. 마칠 때 서버가 저장된 내용 전체를 다시
//! 읽어 크기와 SHA-256을 확인하고 파일·버전을 만든다. 내려받기는 권한을 확인한 뒤 서버를 거쳐 흘려보낸다. 브라우저
//! 안에서 여는 것은 이미지·PDF·텍스트만이고 나머지는 늘 내려받기로 보낸다.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{DocumentId, FileId, ItemId, UploadId, UserId};
use crate::storage::{Part, sha256_hex};

/// 조각 본문의 SHA-256(16진수) 헤더.
pub const PART_SHA256_HEADER: &str = "x-content-sha256";

/// 새 파일을 붙일 곳.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadTarget {
    /// `document` 또는 `item`.
    pub kind: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 업로드 시작. `file_id`가 있으면 그 파일의 새 버전, 없으면 `target`에 새 파일.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUpload {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub file_id: Option<FileId>,
    pub target: Option<UploadTarget>,
    /// 파일 이름(새 버전이면 비워도 된다).
    #[serde(default)]
    pub name: String,
    /// 버전 설명.
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub content_type: String,
    /// 전체 크기(바이트).
    pub size: i64,
}

/// 업로드 상태.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadView {
    pub id: UploadId,
    /// `uploading`·`completing`·`done`·`aborted`·`expired`·`failed`.
    pub state: String,
    pub name: String,
    pub size: i64,
    /// 조각 크기. 마지막 조각만 작다.
    pub chunk_bytes: i64,
    pub parts_total: i64,
    /// 이미 받은 조각 번호.
    pub received: Vec<i32>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// 끝났으면 만든 파일과 버전.
    pub file_id: Option<FileId>,
    pub version: Option<i32>,
    pub failure: Option<String>,
}

/// 버전 하나.
#[derive(Debug, Serialize, ToSchema)]
pub struct VersionView {
    pub version: i32,
    pub size: i64,
    pub sha256: String,
    pub content_type: String,
    pub note: String,
    pub uploaded_by: UserId,
    pub uploaded_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// 첨부 파일.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileView {
    pub id: FileId,
    pub name: String,
    pub document_id: Option<DocumentId>,
    pub item_id: Option<ItemId>,
    pub current_version: i32,
    pub size: i64,
    pub content_type: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 새 버전·이름 바꾸기·지우기를 할 수 있는지.
    pub can_write: bool,
    /// 내려받을 수 있는지(D-20: 문서 첨부는 고칠 수 있는 사람, 또는 소유자가 복제·다운로드를 허용한 문서).
    /// 아니어도 안전한 형식은 브라우저 안에서 볼 수 있다.
    pub can_download: bool,
}

/// 파일과 모든 버전(최신 먼저).
#[derive(Debug, Serialize, ToSchema)]
pub struct FileDetail {
    pub file: FileView,
    pub versions: Vec<VersionView>,
}

/// 이름 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameFile {
    pub name: String,
}

/// 지운 파일의 휴지통 묶음.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileTrashed {
    #[schema(value_type = String, format = Uuid)]
    pub batch_id: Uuid,
}

/// 내려받기 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ContentQuery {
    /// 버전(없으면 최신).
    pub version: Option<i32>,
    /// 브라우저 안에서 열기(이미지·PDF·텍스트만, 나머지는 무시).
    #[serde(default)]
    pub inline: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct UploadRow {
    id: Uuid,
    name: String,
    size: i64,
    chunk_bytes: i64,
    storage_key: String,
    storage_upload_id: Option<String>,
    state: String,
    failure: Option<String>,
    version_id: Option<Uuid>,
    expires_at: OffsetDateTime,
}

const UPLOAD_COLUMNS: &str = "id, name, size, chunk_bytes, storage_key, storage_upload_id, state, failure, \
                              version_id, expires_at";

fn parts_total(size: i64, chunk: i64) -> i64 {
    ((size + chunk - 1) / chunk).max(1)
}

async fn my_upload(tx: &mut UserTx, id: UploadId) -> Result<UploadRow, AppError> {
    sqlx::query_as(&format!("SELECT {UPLOAD_COLUMNS} FROM app_my_upload($1)"))
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)
}

async fn view(tx: &mut UserTx, row: UploadRow) -> Result<UploadView, AppError> {
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
#[utoipa::path(post, path = "/api/uploads", operation_id = "create_upload", request_body = CreateUpload,
    responses((status = 201, body = UploadView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "files")]
pub async fn create_upload(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateUpload>,
) -> Result<(StatusCode, Json<UploadView>), AppError> {
    let limits = state.config.files;
    let (kind, target) = match (&input.file_id, &input.target) {
        (Some(_), None) => (None, None),
        (None, Some(t)) => (Some(t.kind.clone()), Some(t.id)),
        _ => {
            return Err(AppError::BadRequest(
                "give either file_id (new version) or target (new file)".into(),
            ));
        }
    };
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar(
        "SELECT app_create_upload($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
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
            .map_err(AppError::from_db_function)?;
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
    State(state): State<AppState>,
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
    params(("upload_id" = String, Path, format = Uuid), ("number" = i32, Path)),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "files")]
pub async fn put_part(
    State(state): State<AppState>,
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
        return Err(AppError::BadRequest(format!(
            "part number must be 1..={total}"
        )));
    }
    let expected = if i64::from(number) == total {
        row.size - row.chunk_bytes * (total - 1)
    } else {
        row.chunk_bytes
    };
    if i64::try_from(body.len()).unwrap_or(i64::MAX) != expected {
        return Err(AppError::BadRequest(format!(
            "part {number} must be {expected} bytes"
        )));
    }
    let claimed = headers
        .get(PART_SHA256_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| AppError::BadRequest(format!("missing {PART_SHA256_HEADER}")))?;
    let actual = sha256_hex(&body);
    if claimed != actual {
        return Err(AppError::BadRequest(
            "the part does not match its SHA-256; send it again".into(),
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
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 마치기(같은 요청을 다시 보내도 한 번만). 모든 조각이 있어야 한다. 서버가 내용 전체를 다시 읽어 크기를
/// 확인하고 SHA-256을 기록한다. 맞지 않으면 업로드를 닫고 409.
#[utoipa::path(post, path = "/api/uploads/{upload_id}/complete", operation_id = "complete_upload",
    params(("upload_id" = String, Path, format = Uuid)),
    responses((status = 200, body = UploadView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "files")]
pub async fn complete_upload(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<UploadId>,
) -> Result<Json<UploadView>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let row: UploadRow = sqlx::query_as(&format!(
        "SELECT {UPLOAD_COLUMNS} FROM app_begin_complete($1)"
    ))
    .bind(id)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    let parts: Vec<(i32, String, i64)> = sqlx::query_as(
        "SELECT number, etag, size FROM upload_parts WHERE upload_id = $1 ORDER BY number",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    if row.state != "done" {
        // 연결이 끊겨도(프록시 시간 제한 등) 합치기·확인·기록은 끝까지 한다. 다시 요청하면 결과를 본다.
        let worker = state.clone();
        // 따로 도는 작업에도 요청 id를 붙인다(실패 줄을 사용자가 본 요청 id로 찾게, D-83).
        let request_id = crate::logging::current_request_id().unwrap_or_default();
        tokio::spawn(crate::logging::REQUEST_ID.scope(request_id, async move {
            finish(&worker, user, id, row, parts).await
        }))
        .await
        .map_err(|e| AppError::Config(format!("upload completion task: {e}")))??;
    }
    let mut tx = state.db.begin_as(user).await?;
    let row = my_upload(&mut tx, id).await?;
    let out = view(&mut tx, row).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// 저장소에서 조각을 합치고 내용을 확인한 뒤 파일·버전을 기록한다.
async fn finish(
    state: &AppState,
    user: UserId,
    id: UploadId,
    row: UploadRow,
    parts: Vec<(i32, String, i64)>,
) -> Result<(), AppError> {
    let storage_upload = row.storage_upload_id.clone().unwrap_or_default();
    let parts: Vec<Part> = parts
        .into_iter()
        .map(|(number, etag, size)| Part {
            number: u32::try_from(number).unwrap_or_default(),
            etag,
            size: u64::try_from(size).unwrap_or_default(),
        })
        .collect();
    // 앞선 시도가 저장소에서는 마쳤을 수 있다(업로드가 이미 없음). 그러면 내용 확인으로 넘어간다. 다른 이유로
    // 합치지 못하면(조각 표시가 어긋남 등) 처음부터 다시 받는다.
    match state
        .storage
        .complete(&row.storage_key, &storage_upload, &parts)
        .await
    {
        Ok(()) | Err(AppError::NotFound) => {}
        Err(err) => {
            tracing::warn!(event = "upload.assemble_failed", upload = %id.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "storage could not assemble the upload");
            let mut tx = state.db.begin_as(user).await?;
            sqlx::query("SELECT app_reset_upload($1)")
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
            tx.commit().await?;
            return Err(AppError::Conflict(
                "the storage could not assemble the parts; send them again".into(),
            ));
        }
    }
    let checked = state.storage.digest(&row.storage_key).await;
    let failure = match &checked {
        Ok((_, size)) if i64::try_from(*size).ok() == Some(row.size) => None,
        Ok((_, size)) => Some(format!("stored {size} bytes, expected {}", row.size)),
        Err(AppError::NotFound) => Some("the stored content is missing".to_owned()),
        Err(_) => None,
    };
    if let Some(reason) = failure {
        fail(state, user, id, &reason).await?;
        return Err(AppError::Conflict(format!("the upload failed: {reason}")));
    }
    let (sha256, _) = checked?;
    let mut tx = state.db.begin_as(user).await?;
    let finished = sqlx::query("SELECT app_finish_upload($1, $2, $3)")
        .bind(id)
        .bind(&sha256)
        .bind(i64::try_from(state.config.files.max_storage_bytes).unwrap_or(i64::MAX))
        .execute(tx.conn())
        .await;
    match finished {
        Ok(_) => tx.commit().await,
        Err(err) => {
            drop(tx);
            // 대상을 볼 수 없게 됐거나 쓸 수 없거나 저장량이 모자라면 업로드를 닫는다(다시 해도 같다). 잠깐의
            // 충돌(직렬화·교착)은 닫지 않고 다시 요청하게 한다.
            let lasting = err
                .as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| matches!(c.as_ref(), "P0002" | "42501" | "55000"));
            let err = AppError::from_db_function(err);
            if lasting {
                fail(state, user, id, &err.to_string()).await?;
            }
            Err(err)
        }
    }
}

async fn fail(state: &AppState, user: UserId, id: UploadId, reason: &str) -> Result<(), AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_fail_upload($1, $2)")
        .bind(id)
        .bind(reason)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await
}

/// 업로드 그만두기(올린 사람). 받은 조각은 백그라운드 작업이 지운다.
#[utoipa::path(delete, path = "/api/uploads/{upload_id}", operation_id = "abort_upload",
    params(("upload_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "files")]
pub async fn abort_upload(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<UploadId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_abort_upload($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

type FileRow = (
    FileId,
    String,
    Option<DocumentId>,
    Option<ItemId>,
    i32,
    Option<i64>,
    Option<String>,
    OffsetDateTime,
    Option<String>,
    bool,
);

const FILE_SELECT: &str = "SELECT f.id, f.name, f.document_id, f.item_id, f.current_version, v.size, v.content_type,
                                  f.updated_at, app_file_access(f.id), app_file_can_download(f.id)
                           FROM files f
                           LEFT JOIN file_versions v ON v.file_id = f.id AND v.version = f.current_version";

fn file_view(row: FileRow) -> FileView {
    let (
        id,
        name,
        document_id,
        item_id,
        current_version,
        size,
        content_type,
        updated_at,
        access,
        can_download,
    ) = row;
    FileView {
        id,
        name,
        document_id,
        item_id,
        current_version,
        size: size.unwrap_or_default(),
        content_type: content_type.unwrap_or_else(|| "application/octet-stream".into()),
        updated_at,
        can_write: matches!(access.as_deref(), Some("owner" | "write")),
        can_download,
    }
}

/// 문서에 붙은 파일(문서를 볼 수 있는 사람).
#[utoipa::path(get, path = "/api/documents/{document_id}/files", operation_id = "document_files",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [FileView]), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn document_files(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(doc): Path<DocumentId>,
) -> Result<Json<Vec<FileView>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: Option<String> = sqlx::query_scalar("SELECT app_document_access($1)")
        .bind(doc)
        .fetch_one(tx.conn())
        .await?;
    if visible.is_none() {
        return Err(AppError::NotFound);
    }
    let rows: Vec<FileRow> = sqlx::query_as(&format!(
        "{FILE_SELECT} WHERE f.document_id = $1 AND f.deleted_at IS NULL ORDER BY f.name, f.id"
    ))
    .bind(doc)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows.into_iter().map(file_view).collect()))
}

/// 항목에 붙은 파일(항목을 볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/files", operation_id = "item_files",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [FileView]), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn item_files(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<FileView>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: Option<String> = sqlx::query_scalar("SELECT app_item_access($1)")
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
    if visible.is_none() {
        return Err(AppError::NotFound);
    }
    let rows: Vec<FileRow> = sqlx::query_as(&format!(
        "{FILE_SELECT} WHERE f.item_id = $1 AND f.deleted_at IS NULL ORDER BY f.name, f.id"
    ))
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows.into_iter().map(file_view).collect()))
}

async fn one_file(tx: &mut UserTx, id: FileId) -> Result<FileView, AppError> {
    let row: Option<FileRow> = sqlx::query_as(&format!("{FILE_SELECT} WHERE f.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    row.filter(|r| r.8.is_some())
        .map(file_view)
        .ok_or(AppError::NotFound)
}

/// 파일과 모든 버전.
#[utoipa::path(get, path = "/api/files/{file_id}", operation_id = "get_file",
    params(("file_id" = String, Path, format = Uuid)),
    responses((status = 200, body = FileDetail), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn get_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
) -> Result<Json<FileDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let file = one_file(&mut tx, id).await?;
    type Row = (
        i32,
        i64,
        String,
        String,
        String,
        UserId,
        Option<String>,
        OffsetDateTime,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT v.version, v.size, v.sha256, v.content_type, v.note, v.uploaded_by, u.display_name, v.created_at
         FROM file_versions v LEFT JOIN users u ON u.id = v.uploaded_by
         WHERE v.file_id = $1 ORDER BY v.version DESC",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let versions = rows
        .into_iter()
        .map(
            |(
                version,
                size,
                sha256,
                content_type,
                note,
                uploaded_by,
                uploaded_by_name,
                created_at,
            )| {
                VersionView {
                    version,
                    size,
                    sha256,
                    content_type,
                    note,
                    uploaded_by,
                    uploaded_by_name,
                    created_at,
                }
            },
        )
        .collect();
    Ok(Json(FileDetail { file, versions }))
}

/// 이름 바꾸기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/files/{file_id}", operation_id = "rename_file",
    params(("file_id" = String, Path, format = Uuid)), request_body = RenameFile,
    responses((status = 200, body = FileView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "files")]
pub async fn rename_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
    Json(input): Json<RenameFile>,
) -> Result<Json<FileView>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_rename_file($1, $2)")
        .bind(id)
        .bind(&input.name)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let file = one_file(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(file))
}

/// 휴지통으로(쓸 수 있는 사람). 모든 버전이 함께 가고 함께 돌아온다.
#[utoipa::path(delete, path = "/api/files/{file_id}", operation_id = "trash_file",
    params(("file_id" = String, Path, format = Uuid)),
    responses((status = 200, body = FileTrashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "files")]
pub async fn trash_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
) -> Result<Json<FileTrashed>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let batch_id: Uuid = sqlx::query_scalar("SELECT app_trash_file($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(FileTrashed { batch_id }))
}

/// 현재가 아닌 버전 하나를 영구 삭제(쓸 수 있는 사람, D-56). 저장소의 내용은 백그라운드 작업이 지운다.
#[utoipa::path(delete, path = "/api/files/{file_id}/versions/{version}", operation_id = "delete_file_version",
    params(("file_id" = String, Path, format = Uuid), ("version" = i32, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "files")]
pub async fn delete_version(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, version)): Path<(FileId, i32)>,
) -> Result<StatusCode, AppError> {
    // 열린 실시간 방의 상태(본문의 파일 참조 포함)를 먼저 저장해, 방금 실시간 본문에 넣은 이미지의 버전도
    // 참조 확인(0059)이 보게 한다. 주기적 저장(3초)을 기다리면 그 사이에 지울 수 있었다.
    state.collab.persist_all().await;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_file_version($1, $2)")
        .bind(id)
        .bind(version)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 브라우저 안에서 열어도 되는 형식(스크립트를 실행할 수 없는 것만). SVG·HTML은 늘 내려받기.
const INLINE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "application/pdf",
    "text/plain",
];

/// 저장된 형식 문자열을 안전한 `type/subtype`으로(매개변수·이상한 문자는 버림).
fn safe_type(raw: &str) -> String {
    let base = raw
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let ok = base.split_once('/').is_some_and(|(t, s)| {
        !t.is_empty()
            && !s.is_empty()
            && base
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/.+-_".contains(c))
    });
    if ok {
        base
    } else {
        "application/octet-stream".into()
    }
}

/// `Content-Disposition` 값. 이름은 RFC 5987로, ASCII 대체 이름도 함께.
fn disposition(inline: bool, name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!(
        "{}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}",
        if inline { "inline" } else { "attachment" }
    )
}

/// 내려받기(볼 수 있는 사람). 최신 또는 `version`. `inline=true`는 이미지·PDF·텍스트만 브라우저 안에서 연다.
#[utoipa::path(get, path = "/api/files/{file_id}/content", operation_id = "file_content",
    params(("file_id" = String, Path, format = Uuid), ContentQuery),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
              (status = 404, body = ErrorBody)), tag = "files")]
pub async fn content(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let file = one_file(&mut tx, id).await?;
    let version = query.version.unwrap_or(file.current_version);
    let found: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT storage_key, size, content_type FROM file_versions WHERE file_id = $1 AND version = $2",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    let (key, size, content_type) = found.ok_or(AppError::NotFound)?;
    let content_type = safe_type(&content_type);
    let viewable = INLINE_TYPES.contains(&content_type.as_str());
    // 내려받기가 허용되지 않으면 안전한 형식을 브라우저 안에서 보는 것만 된다.
    if !file.can_download && !(query.inline && viewable) {
        return Err(AppError::Forbidden);
    }
    let inline = query.inline && viewable;
    let stream = state.storage.get(&key).await?;
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    let shown = if content_type == "text/plain" {
        "text/plain; charset=utf-8".to_owned()
    } else {
        content_type
    };
    let set = |headers: &mut HeaderMap, name: header::HeaderName, value: String| {
        if let Ok(v) = HeaderValue::from_str(&value) {
            headers.insert(name, v);
        }
    };
    set(headers, header::CONTENT_TYPE, shown);
    set(headers, header::CONTENT_LENGTH, size.to_string());
    set(
        headers,
        header::CONTENT_DISPOSITION,
        disposition(inline, &file.name),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'",
        ),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_are_cleaned() {
        assert_eq!(safe_type("Image/PNG; charset=x"), "image/png");
        assert_eq!(safe_type("text/html\r\nX: y"), "application/octet-stream");
        assert_eq!(safe_type(""), "application/octet-stream");
        assert_eq!(
            safe_type("application/vnd.ms-excel"),
            "application/vnd.ms-excel"
        );
    }

    #[test]
    fn dispositions_quote_names_safely() {
        assert_eq!(
            disposition(false, "회의 \"요약\".pdf"),
            "attachment; filename=\"__ ____.pdf\"; filename*=UTF-8''%ED%9A%8C%EC%9D%98%20%22%EC%9A%94%EC%95%BD%22.pdf"
        );
        assert!(disposition(true, "a.png").starts_with("inline;"));
    }

    #[test]
    fn counts_parts() {
        assert_eq!(parts_total(1, 5), 1);
        assert_eq!(parts_total(5, 5), 1);
        assert_eq!(parts_total(6, 5), 2);
    }
}
