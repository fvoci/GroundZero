//! 파일 경로와 업로드 본문·동시 처리 한도.

use crate::{FilesState, commands, completion, content, read, uploads};
use axum::Router;
use axum::extract::{DefaultBodyLimit, FromRef, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use fvoci_http::session::SessionState;
use std::sync::Arc;

async fn upload_slot(State(state): State<FilesState>, req: Request, next: Next) -> Response {
    let Ok(_permit) = Arc::clone(&state.upload_permits).acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    next.run(req).await
}

/// 파일 경로. 조립 상태에서 파일·세션 부분 상태를 꺼낸다.
pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    FilesState: FromRef<S>,
    SessionState: FromRef<S>,
{
    let files = FilesState::from_ref(state);
    let part_limit = usize::try_from(files.limits.chunk_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(64 * 1024);
    Router::new()
        .route("/uploads", post(uploads::create_upload))
        .route(
            "/uploads/{upload_id}",
            get(uploads::get_upload).delete(uploads::abort_upload),
        )
        .route(
            "/uploads/{upload_id}/parts/{number}",
            put(uploads::put_part)
                .layer(DefaultBodyLimit::max(part_limit))
                .layer(middleware::from_fn_with_state(files, upload_slot)),
        )
        .route(
            "/uploads/{upload_id}/complete",
            post(completion::complete_upload),
        )
        .route("/documents/{document_id}/files", get(read::document_files))
        .route("/items/{item_id}/files", get(read::item_files))
        .route(
            "/files/{file_id}",
            get(read::get_file)
                .put(commands::rename_file)
                .delete(commands::trash_file),
        )
        .route("/files/{file_id}/content", get(content::content))
        .route(
            "/files/{file_id}/versions/{version}",
            delete(commands::delete_version),
        )
}
