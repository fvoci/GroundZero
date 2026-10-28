//! 마치기 점유·내용 무결성 확인·파일 버전 기록.
use crate::{
    FilesState,
    model::UploadView,
    uploads::{UPLOAD_COLUMNS, UploadRow, my_upload, view},
};
use axum::Json;
use axum::extract::{Path, State};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{UploadId, UserId};
use fvoci_db::{UserTx, new_ids};
use fvoci_http::session::CurrentUser;
use fvoci_storage::{Assembled, Part};
use time::OffsetDateTime;
/// 마치기(같은 요청을 다시 보내도 한 번만). 모든 조각이 있어야 한다. 서버가 내용 전체를 다시 읽어 크기를
/// 확인하고 SHA-256을 기록한다. 맞지 않으면 업로드를 닫고 409.
#[utoipa::path(post, path = "/api/uploads/{upload_id}/complete", operation_id = "complete_upload",
    params(("upload_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "보내면 세션 사용자와 비교한다(D-96). 생략할 수 있다.")),
    responses((status = 200, body = UploadView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "files")]
pub async fn complete_upload(
    State(state): State<FilesState>,
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
        let claim = row.state_changed_at;
        // 따로 도는 작업에도 요청 id를 붙인다(실패 줄을 사용자가 본 요청 id로 찾게, D-83).
        let request_id = fvoci_core::logging::current_request_id().unwrap_or_default();
        let finished = tokio::spawn(fvoci_core::logging::REQUEST_ID.scope(
            request_id,
            async move {
                let result = finish(&worker, user, id, row, parts, claim).await;
                if result.is_err() {
                    // 마무리 작업이 멈췄다. 일시적 실패(저장소 읽기 오류, 연결 풀 대기 초과, 교착 등)로 'completing'에
                    // 남았으면 점유를 바로 풀어, 다시 요청이 5분 동안 "마치는 중"(409)을 받지 않게 한다(감사 S11). 연결이
                    // 끊겨 이 요청의 핸들러가 사라져도 풀리게 작업 안에서 푼다. 닫거나 되돌린 업로드, 다른 시도가 넘겨받은
                    // 점유는 그대로다.
                    release_claim(&worker, user, id, claim).await;
                }
                result
            },
        ))
        .await;
        match finished {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(e) => {
                // 작업이 패닉 등으로 끝나 스스로 풀지 못했다.
                release_claim(&state, user, id, claim).await;
                return Err(AppError::Config(format!("upload completion task: {e}")));
            }
        }
    }
    let mut tx = state.db.begin_as(user).await?;
    let row = my_upload(&mut tx, id).await?;
    let out = view(&mut tx, row).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// 저장소에서 조각을 합치고 내용을 확인한 뒤 파일·버전을 기록한다.
async fn finish(
    state: &FilesState,
    user: UserId,
    id: UploadId,
    row: UploadRow,
    parts: Vec<(i32, String, i64)>,
    claim: OffsetDateTime,
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
    // 조각을 잇는다. 로컬은 이 시도만의 준비본에 잇고, 최종 키로는 아래에서 점유를 쥔 채 게시한다(점유를 잃은 늦은
    // 시도가 다른 시도가 마치고 기록한 내용을 덮지 않게, #65). 앞선 시도가 저장소에서 이미 게시했으면(업로드가 이미
    // 없음) 최종 키의 내용을 확인한다. 다른 이유로 잇지 못하면(조각 표시가 어긋남 등) 처음부터 다시 받는다.
    let assembled = match state
        .storage
        .assemble(&row.storage_key, &storage_upload, &parts)
        .await
    {
        Ok(assembled) => assembled,
        Err(AppError::NotFound) => Assembled::Published,
        Err(err) => {
            tracing::warn!(event = "upload.assemble_failed", upload = %id.0, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "storage could not assemble the upload");
            let mut tx = state.db.begin_as(user).await?;
            hold_claim(&mut tx, id, claim).await?;
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
    };
    let result = record(state, user, id, &row, claim, &storage_upload, &assembled).await;
    // 게시하지 않은 준비본(점유를 잃었거나 확인·기록에 실패함)은 버린다.
    assembled.discard().await;
    result
}

/// 이은 내용을 확인하고, 점유를 쥔 채 최종 키로 게시한 뒤 파일·버전을 기록한다.
async fn record(
    state: &FilesState,
    user: UserId,
    id: UploadId,
    row: &UploadRow,
    claim: OffsetDateTime,
    storage_upload: &str,
    assembled: &Assembled,
) -> Result<(), AppError> {
    let checked = state
        .storage
        .digest_assembled(&row.storage_key, assembled)
        .await;
    let failure = match &checked {
        Ok((_, size)) if i64::try_from(*size).ok() == Some(row.size) => None,
        Ok((_, size)) => Some(format!("stored {size} bytes, expected {}", row.size)),
        Err(AppError::NotFound) => Some("the stored content is missing".to_owned()),
        Err(_) => None,
    };
    if let Some(reason) = failure {
        fail(state, user, id, claim, &reason).await?;
        return Err(AppError::Conflict(format!("the upload failed: {reason}")));
    }
    let (sha256, _) = checked?;
    let mut tx = state.db.begin_as(user).await?;
    hold_claim(&mut tx, id, claim).await?;
    // 점유 확인은 이미 마친 업로드에도 참이다. 다른 시도가 넘겨받아 마쳤으면 이 시도의 내용은 게시하지 않는다(기록된
    // 내용을 덮는다).
    if my_upload(&mut tx, id).await?.state == "done" {
        return Ok(());
    }
    // 행을 잠근 채 게시하므로 이 트랜잭션이 끝날 때까지 다른 시도는 넘겨받지 못한다. 게시한 뒤 기록하지 못하면 최종
    // 키에는 기록되지 않은 이 내용이 남는다. 잠깐의 충돌이면 점유를 풀고, 다음 시도가 (조각 폴더가 이미 없으니) 최종
    // 키의 내용을 확인해 기록한다. 오래 갈 실패면 업로드를 닫고 최종 키를 지울 목록에 올린다(app_fail_upload).
    state
        .storage
        .publish(&row.storage_key, storage_upload, assembled)
        .await?;
    let finished = sqlx::query("SELECT app_finish_upload($1, $2, $3, $4)")
        .bind(id)
        .bind(&sha256)
        .bind(i64::try_from(state.limits.max_storage_bytes).unwrap_or(i64::MAX))
        .bind(new_ids(3))
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
                fail(state, user, id, claim, &err.to_string()).await?;
            }
            Err(err)
        }
    }
}

/// 멈춘 마무리의 점유를 푼다(이 시도의 점유일 때만, 0088). 실패해도 원래 오류를 돌려주는 게 우선이라 기록만
/// 남긴다(5분 뒤 넘겨받기는 그대로 된다).
async fn release_claim(state: &FilesState, user: UserId, id: UploadId, claim: OffsetDateTime) {
    let released = async {
        let mut tx = state.db.begin_as(user).await?;
        sqlx::query("SELECT app_release_upload_claim($1, $2)")
            .bind(id)
            .bind(claim)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        tx.commit().await
    }
    .await;
    if let Err(err) = released {
        tracing::warn!(event = "upload.release_failed", upload = %id.0, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "could not release the completion claim");
    }
}

/// 이 시도가 아직 마무리를 맡고 있는지 행을 잠근 채 확인한다(0088). 5분 뒤 다른 요청이 넘겨받았으면 늦게 돌아온 이
/// 시도는 업로드를 되돌리거나(받은 조각 삭제) 기록·닫지 않는다.
async fn hold_claim(tx: &mut UserTx, id: UploadId, claim: OffsetDateTime) -> Result<(), AppError> {
    let held: bool = sqlx::query_scalar("SELECT app_upload_claim_held($1, $2)")
        .bind(id)
        .bind(claim)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    if held {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "another request took over completing this upload".into(),
        ))
    }
}

async fn fail(
    state: &FilesState,
    user: UserId,
    id: UploadId,
    claim: OffsetDateTime,
    reason: &str,
) -> Result<(), AppError> {
    let mut tx = state.db.begin_as(user).await?;
    hold_claim(&mut tx, id, claim).await?;
    sqlx::query("SELECT app_fail_upload($1, $2)")
        .bind(id)
        .bind(reason)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await
}
