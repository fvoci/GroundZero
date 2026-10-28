//! 항목 아래 문서 조회와 생성. 항목·모듈 처리기에 기대지 않는다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, ItemId};
use fvoci_core::validate::validate_title;
use fvoci_db::{is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;

use crate::documents::{
    CreateDocument, Document, DocumentSummary, NewDocumentPlace, SCHEMA_VERSION, SUMMARY_COLUMNS,
    SummaryRow, TREE_ORDER, initial_body, load_with_access, max_depth, to_document, to_summaries,
};
use crate::state::DocumentsState;

/// 항목 아래 문서 목록. 볼 수 있는 문서만 나온다.
#[utoipa::path(get, path = "/api/items/{item_id}/documents",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [DocumentSummary]), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn list_item_documents(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Item, id).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents WHERE item_id = $1 ORDER BY {TREE_ORDER}"
    ))
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 항목 아래 문서 생성(항목 쓰기 권한). 모듈 번호열에서 번호를 받는다.
#[utoipa::path(post, path = "/api/items/{item_id}/documents",
    params(("item_id" = String, Path, format = Uuid)), request_body = CreateDocument,
    responses((status = 201, body = Document), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn create_item_document(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let body = initial_body(&mut tx, &input, NewDocumentPlace::Item(item)).await?;
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_module_document($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(item)
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(body)
                .bind(input.parent_id)
                .bind(max_depth(&state.limits))
                .bind(new_ids(4))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let (row, access) = load_with_access(&mut tx, id).await?;
        tx.commit().await?;
        Ok(to_document(row, access))
    };
    match run().await {
        Ok(doc) => Ok((StatusCode::CREATED, Json(doc))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}
