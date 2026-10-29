//! 복제·이동 미리보기와 원자적 배치 명령.

use crate::state::{CommittedMove, DocumentsState, MoveRequest};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, ItemId, UserId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_db::{Db, UserTx, needed_ids, new_ids};
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};

use super::{
    CopyDocument, Document, LOCK_LIMIT, MoveAccessChange, MoveConflict, MoveDocument, MoveOutcome,
    MovePreview, MoveReply, MoveTarget, access, load, load_with_access, to_document,
};

/// 복제. 새 문서(리비전 1)이며 이후 원본과 동기화하지 않는다 (D-23).
/// 소유 권한자는 항상, 그 밖의 사람은 원본이 복제를 허용할 때만 할 수 있다.
#[utoipa::path(post, path = "/api/documents/{document_id}/copy",
    params(("document_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC)), request_body = CopyDocument,
    responses((status = 201, body = Document), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 400, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
pub async fn copy(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CopyDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    let mut tx = state.db.begin_as(user).await?;
    let new_id: DocumentId = sqlx::query_scalar("SELECT app_copy_document($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(input.request_id)
        .bind(input.target_workspace_id)
        .bind(&title)
        .bind(new_ids(4))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 사본의 링크도 기록한다. 언급 알림은 원작성자가 한 것이 아니므로 만들지 않는다(새 행 없음).
    sqlx::query("SELECT app_record_document_references($1, false, '{}')")
        .bind(new_id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (row, access) = load_with_access(&mut tx, new_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(to_document(row, access))))
}

async fn preview(
    tx: &mut UserTx,
    id: DocumentId,
    target: Option<WorkspaceId>,
    item: Option<ItemId>,
) -> Result<MovePreview, AppError> {
    let changes: Vec<MoveAccessChange> =
        sqlx::query_as("SELECT change, user_id, display_name, from_grade, to_grade FROM app_move_preview($1, $2, $3)")
            .bind(id)
            .bind(target)
            .bind(item)
            .fetch_all(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let keeps_number: bool = sqlx::query_scalar("SELECT app_move_keeps_number($1, $2, $3)")
        .bind(id)
        .bind(target)
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 공개 링크는 이동해도 유지된다. 비공개 항목으로 옮길 때 알 수 있게 개수를 보여 준다.
    // 하위 문서는 원래 자리에 남는다(이동하는 문서의 원래 부모에 붙음). 볼 수 있는 것만 센다.
    let children_left_behind: i64 =
        sqlx::query_scalar("SELECT count(*) FROM documents WHERE parent_id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    let active_public_links: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_links
         WHERE document_id = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(MovePreview {
        changes,
        reference_changes: !keeps_number,
        active_public_links,
        children_left_behind,
        history_moves_with_document: true,
    })
}

/// 이동 미리보기(소유 권한자).
#[utoipa::path(post, path = "/api/documents/{document_id}/move-plan",
    params(("document_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC)), request_body = MoveTarget,
    responses((status = 200, body = MovePreview), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
              (status = 400, body = ErrorBody)), tag = "documents")]
pub async fn move_preview(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<MoveTarget>,
) -> Result<Json<MovePreview>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let p = preview(&mut tx, id, input.target_workspace_id, input.target_item_id).await?;
    tx.commit().await?;
    Ok(Json(p))
}

/// 이동(소유 권한자). 확인한 미리보기가 없거나 지금 계산한 것과 다르면 409와 새 미리보기를 돌려준다. 실시간 편집 중인
/// 방의 편집이나 방 없이 남은 저장 대기 상태를 먼저 리비전으로 남기지 못하면 옮기지 않고 409(오류 본문)다. 편집은 그대로
/// 남고 다시 하면 된다. 한도를 낮춰 그 내용이 본문 검사를 통과하지 못하는 경우처럼 다시 해도 남길 수 없을 때도 같은 409로
/// 옮기지 않는다(문구에 "try again"이 없다). 다른 변경이 문서나 그 자리를 쥐어 잠금 하나를
/// 한도(`LOCK_LIMIT`)보다 오래 기다리면 옮기지 않고 409다(편집은 방에 남는다, 다시 하면 된다).
#[utoipa::path(put, path = "/api/documents/{document_id}/placement",
    params(("document_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC)), request_body = MoveDocument,
    responses((status = 200, body = MoveOutcome), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = MoveConflict),
              (status = 400, body = ErrorBody)), tag = "documents")]
pub async fn move_document(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<MoveDocument>,
) -> Result<Response, AppError> {
    // 실시간으로 편집 중인 문서는 본문(마지막 리비전)이 뒤처져 있을 수 있다. 열린 방이 있으면 그 방이 옮긴다(감사 S6,
    // #71): 방의 편집을 리비전으로 남기고(남기지 못하면 옮기지 않고 409, 편집은 방에 남는다) → 아래 이동 트랜잭션 →
    // 옮겼으면 그사이 받은 편집까지 리비전으로 남긴 뒤 연결을 닫는다(4409, 새 자리의 권한·설정으로 다시 연다). 방이 이
    // 명령을 처리하는 동안 들어온 편집은 대기열에서 기다리므로, 확인을 받은 편집이 옮긴 곳에서 리비전이 되지 못하고
    // 사라지는 일이 없다. 방이 없으면 옮기는 동안 방이 새로 열리지 않게 막고, 방 없이 남은 실시간 상태(앞선 방이 리비전을
    // 남기지 못하고 닫힘)를 먼저 리비전으로 남긴 뒤(그 상태를 고친 사람 가운데 아직 쓸 수 있는 사람, 없으면 옮기는 사람이
    // 대신 작성자이고 실제 편집자를 따로 적는다, D-95. 남기지 못하면 409) 옮긴다. 방에는 이
    // 이동을 할 수 있는 요청(문서 소유 권한·대상 쓰기 권한, 미리보기와 같은 확인)만 보낸다: 거절될 요청이 다른 사람의 열린
    // 편집을 리비전으로 남기게 하거나 방을 기다리게 하지 않는다(리뷰). 거절이면 그 오류를 바로 돌려준다. 이동 트랜잭션이
    // 잠근 뒤 다시 확인하는 것이 그대로 최종이다. 방을 기다리는 동안 DB 연결을 쥐지 않는다.
    {
        let mut tx = state.db.begin_as(user).await?;
        preview(&mut tx, id, input.target_workspace_id, input.target_item_id).await?;
        tx.commit().await?;
    }
    let request = MoveRequest {
        doc: id,
        user,
        input,
    };
    match state
        .runtime
        .move_document(&state.db, &state.limits, request)
        .await?
    {
        MoveReply::Moved { outcome, .. } => Ok(Json(outcome).into_response()),
        MoveReply::Changed(current) => Ok((StatusCode::CONFLICT, Json(current)).into_response()),
    }
}

/// 이 트랜잭션에만 잠금 대기 한도(`LOCK_LIMIT`)를 건다(세션 풀러의 연결에 남지 않는다).
pub(crate) async fn limit_lock_wait(tx: &mut UserTx) -> Result<(), AppError> {
    sqlx::query(&format!("SET LOCAL lock_timeout = '{LOCK_LIMIT}'"))
        .execute(tx.conn())
        .await?;
    Ok(())
}

/// 잠금 대기가 한도를 넘은 것(55P03)은 409(`refused`: 하지 않았다, 다시 하면 된다). 그 밖은 `from_db_function`.
pub(crate) fn lock_limit_error(err: sqlx::Error, refused: &'static str) -> AppError {
    if err
        .as_database_error()
        .is_some_and(|db| db.code().as_deref() == Some("55P03"))
    {
        return AppError::Conflict(refused.into());
    }
    AppError::from_db_function(err)
}

fn move_lock_error(err: sqlx::Error) -> AppError {
    lock_limit_error(
        err,
        "another change is holding the document or its places; nothing was moved, try again",
    )
}

/// 이동 트랜잭션(잠금 → 미리보기 다시 계산·확인 → 옮김). 열린 방이 있으면 방이 자기 명령 안에서 부르고, 없으면 요청
/// 처리기가 방이 새로 열리지 않게 막은 채 부른다(`Rooms::move_document`). 잠금은 `LOCK_LIMIT`까지만 기다린다.
pub async fn move_in_db(
    db: &Db,
    user: UserId,
    id: DocumentId,
    input: &MoveDocument,
) -> Result<MoveReply, AppError> {
    let mut tx = db.begin_as(user).await?;
    limit_lock_wait(&mut tx).await?;
    // 잠금 순서(자리 → 문서 행 → 항목 → 번호): 원래 자리와 새 자리를 먼저 잠근다.
    sqlx::query("SELECT app_lock_move($1, $2, $3)")
        .bind(id)
        .bind(input.target_workspace_id)
        .bind(input.target_item_id)
        .execute(tx.conn())
        .await
        .map_err(move_lock_error)?;
    // 소유 권한자만 잠글 수 있다(RLS 갱신 정책). 다른 사람에게는 행이 없고, 아래 함수가 권한 오류를 낸다.
    sqlx::query("SELECT 1 FROM documents WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(move_lock_error)?;
    // 원래 항목과 대상 항목(과 그 모듈)도 잠가, 확인한 접근 변화가 이동 시점과 같게 한다.
    sqlx::query(
        "SELECT app_lock_item((SELECT item_id FROM documents WHERE id = $1)), app_lock_item($2)",
    )
    .bind(id)
    .bind(input.target_item_id)
    .execute(tx.conn())
    .await
    .map_err(move_lock_error)?;
    // 잠근 뒤 미리보기를 다시 계산해 사용자가 확인한 것과 같을 때만 옮긴다.
    let current = preview(&mut tx, id, input.target_workspace_id, input.target_item_id).await?;
    let confirmed = input.acknowledged.as_ref().is_some_and(|ack| {
        let mut a = ack.changes.clone();
        let mut b = current.changes.clone();
        a.sort_by_key(|change| change.user_id.0);
        b.sort_by_key(|change| change.user_id.0);
        a == b
    });
    if !confirmed {
        tx.commit().await?;
        return Ok(MoveReply::Changed(current));
    }
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_move_document_ids($1)").bind(id),
    )
    .await?;
    let (workspace_id, reference): (WorkspaceId, String) =
        sqlx::query_as("SELECT workspace_id, reference FROM app_move_document($1, $2, $3, $4)")
            .bind(id)
            .bind(input.target_workspace_id)
            .bind(input.target_item_id)
            .bind(ids)
            .fetch_one(tx.conn())
            .await
            .map_err(move_lock_error)?;
    let document = visible_document(&mut tx, id).await?;
    let xid: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
        .fetch_one(tx.conn())
        .await?;
    let xid = xid
        .parse::<u64>()
        .map_err(|err| AppError::Database(sqlx::Error::Decode(Box::new(err))))?;
    tx.commit().await?;
    Ok(MoveReply::Moved {
        outcome: Box::new(MoveOutcome {
            workspace_id,
            reference,
            document,
        }),
        committed_at: std::time::Instant::now(),
        commit: CommittedMove::new(xid),
    })
}

/// 이 사람이 볼 수 있으면 그 문서(볼 수 없으면 `None`).
async fn visible_document(tx: &mut UserTx, id: DocumentId) -> Result<Option<Document>, AppError> {
    Ok(match access(tx, id).await? {
        Some(a) => load(tx, id).await?.map(|row| to_document(row, a)),
        None => None,
    })
}

/// 옮긴 뒤 방이 리비전을 더 남겼을 때 이동의 답에 넣을 지금 문서(옮긴 사람이 볼 수 있으면).
pub async fn document_for(
    db: &Db,
    user: UserId,
    id: DocumentId,
) -> Result<Option<Document>, AppError> {
    let mut tx = db.begin_as(user).await?;
    let document = visible_document(&mut tx, id).await?;
    tx.commit().await?;
    Ok(document)
}
