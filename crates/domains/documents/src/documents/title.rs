//! 제목만 바꾸는 문서 명령. 본문과 리비전의 조건부 저장 순서를 유지한다(D-62·D-77).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::DocumentId;
use fvoci_core::validate::validate_title;
use fvoci_http::cache;
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{Document, document_reply, representation, stale_reply, to_document, write_revision};
use crate::document_locks;
use crate::state::DocumentsState;

/// 제목 명령. 본문은 보내지 않으며 현재 본문을 그대로 유지한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchDocumentTitle {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub base_revision: i32,
    /// 글자 수 상한은 서버의 limits.max_title_chars와 400 응답의 limit에 있다.
    pub title: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DocumentTitleConflict {
    Stale(Box<StaleWrite<Document>>),
    State(ErrorBody),
}

/// 권한·잠금 → 멱등 재시도 → If-Match → 기준 판 → 같은 제목 확인 → 리비전 저장 순서다.
#[utoipa::path(patch, path = "/api/documents/{document_id}", operation_id = "patch_document_title",
    params(("document_id" = String, Path, format = Uuid),
           ("If-Match" = Option<String>, Header, description = "현재 문서의 강한 ETag. 다르면 412"),
           ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC)),
    request_body = PatchDocumentTitle,
    responses((status = 200, body = Document, headers(("ETag" = String))),
              (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = DocumentTitleConflict),
              (status = 412, body = StaleWrite<Document>)), tag = "documents")]
pub async fn patch_title(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    headers: HeaderMap,
    Json(input): Json<PatchDocumentTitle>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    let sent = json!({"title": title.as_str()});
    let mut tx = state.db.begin_as(user).await?;
    let (access, current, _) = document_locks::for_save(&mut tx, id).await?;
    let replay: Option<(i32, Option<i32>)> = sqlx::query_as(
        "SELECT revision, restored_from FROM document_revisions
         WHERE document_id = $1 AND request_id = $2 AND author = app_user_id()",
    )
    .bind(id)
    .bind(input.request_id)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((_, Some(_))) = replay {
        return Err(AppError::invalid(
            "request_id",
            InvalidReason::InvalidFormat,
            None,
            "request_id was already used for another change",
        ));
    }
    if let Some((revision, None)) = replay {
        tx.commit().await?;
        let doc = to_document(current, access);
        return if doc.revision == revision {
            document_reply(user, &doc)
        } else {
            stale_reply(StatusCode::CONFLICT, "conflict", user, doc, &sent)
        };
    }
    if headers.contains_key(header::IF_MATCH) {
        let now = to_document(current.clone(), access);
        let (_, etag) = representation(user, &now)?;
        if cache::if_match(&headers, &etag) == Some(false) {
            tx.commit().await?;
            return stale_reply(
                StatusCode::PRECONDITION_FAILED,
                "precondition_failed",
                user,
                now,
                &sent,
            );
        }
    }
    if current.revision != input.base_revision {
        tx.commit().await?;
        return stale_reply(
            StatusCode::CONFLICT,
            "conflict",
            user,
            to_document(current, access),
            &sent,
        );
    }
    if current.title == title {
        tx.commit().await?;
        return document_reply(user, &to_document(current, access));
    }
    let row = write_revision(
        &mut tx,
        id,
        &title,
        &current.body,
        input.request_id,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    document_reply(user, &to_document(row, access))
}
