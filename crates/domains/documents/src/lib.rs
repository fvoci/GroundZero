//! 문서 도메인의 자원·명령과 라우터·OpenAPI 조각. 공동 편집 runtime은 server가 연결한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

mod document_locks;
pub mod documents;
pub mod items;
pub mod model;
pub mod public_links;
pub mod references;
pub mod revisions;
pub mod state;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_extractor_with_state;
use axum::routing::{delete, get, patch, post, put};
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
use utoipa::OpenApi;

pub use state::{DocumentLimits, DocumentsState};

#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        documents::list_wiki,
        documents::create_wiki,
        documents::get,
        documents::patch_title,
        documents::my_notes,
        documents::create_note,
        documents::set_settings,
        documents::list_shares,
        documents::create_share,
        documents::delete_share,
        documents::copy,
        documents::move_preview,
        documents::move_document,
        documents::set_parent,
        documents::shared_with_me,
        documents::templates,
        documents::set_template,
        references::task_backlinks,
        revisions::list,
        revisions::get,
        revisions::export,
        revisions::restore,
        references::document_backlinks,
        references::item_backlinks,
        public_links::list,
        public_links::create,
        public_links::revoke,
        public_links::view,
        items::list_item_documents,
        items::create_item_document,
    ),
    components(schemas(
        documents::DocumentSummary,
        documents::Document,
        documents::CreateDocument,
        documents::PatchDocumentTitle,
        documents::DocumentTitleConflict,
        documents::Access,
        documents::SharedDocument,
        documents::DocumentSettings,
        documents::Share,
        documents::CreateShare,
        documents::CopyDocument,
        documents::MoveTarget,
        documents::MoveDocument,
        documents::MovePreview,
        documents::MoveAccessChange,
        documents::MoveConflict,
        documents::AcknowledgedPreview,
        documents::MoveOutcome,
        documents::SetParent,
        documents::DocumentTemplate,
        documents::SetDocumentTemplate,
        revisions::RevisionSummary,
        revisions::Revision,
        revisions::ExportFormat,
        revisions::ExportRequest,
        revisions::RestoreRevision,
        revisions::RestoreConflict,
        references::Backlink,
        public_links::PublicLink,
        public_links::CreatePublicLink,
        public_links::CreatedPublicLink,
        public_links::PublicDocument,
    ))
)]
pub struct DocumentsApi;

pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    DocumentsState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/document-templates", get(documents::templates))
        .route(
            "/documents/{document_id}/template",
            put(documents::set_template),
        )
        .route(
            "/workspaces/{workspace_id}/wiki/documents",
            get(documents::list_wiki).post(documents::create_wiki),
        )
        .route(
            "/me/notes",
            get(documents::my_notes).post(documents::create_note),
        )
        .route(
            "/documents/{document_id}/settings",
            patch(documents::set_settings),
        )
        .route(
            "/documents/{document_id}/shares",
            get(documents::list_shares).post(documents::create_share),
        )
        .route(
            "/document-shares/{share_id}",
            delete(documents::delete_share),
        )
        .route("/documents/{document_id}/copy", post(documents::copy))
        .route(
            "/documents/{document_id}/move-plan",
            post(documents::move_preview),
        )
        .route(
            "/documents/{document_id}/placement",
            put(documents::move_document),
        )
        .route(
            "/documents/{document_id}/public-links",
            get(public_links::list).post(public_links::create),
        )
        .route("/public-links/{link_id}", delete(public_links::revoke))
        .route(
            "/documents/{document_id}/parent",
            put(documents::set_parent),
        )
        .route(
            "/tasks/{task_id}/backlinks",
            get(references::task_backlinks),
        )
        .route("/documents/{document_id}/revisions", get(revisions::list))
        .route(
            "/documents/{document_id}/revisions/{revision}",
            get(revisions::get),
        )
        .route(
            "/documents/{document_id}/revisions/{revision}/restore",
            post(revisions::restore),
        )
        .route(
            "/documents/{document_id}/revisions/{revision}/export",
            post(revisions::export),
        )
        .route("/me/shared", get(documents::shared_with_me))
        .route(
            "/documents/{document_id}/backlinks",
            get(references::document_backlinks),
        )
        .route(
            "/items/{item_id}/backlinks",
            get(references::item_backlinks),
        )
        .route(
            "/items/{item_id}/documents",
            get(items::list_item_documents).post(items::create_item_document),
        )
        .route("/public/{token}", get(public_links::view))
        .route(
            "/documents/{document_id}",
            get(documents::get).patch(documents::patch_title),
        )
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr4c_contract_uses_collab_for_bodies_and_revisions_for_title_conflicts() {
        let doc = serde_json::to_value(DocumentsApi::openapi()).unwrap_or_default();
        let resource = &doc["paths"]["/api/documents/{document_id}"];
        assert!(resource.get("put").is_none());
        assert!(resource["patch"]["responses"].get("412").is_none());
        assert!(
            resource["patch"]["responses"]["200"]
                .get("headers")
                .is_none()
        );
        assert!(
            resource["patch"]["parameters"]
                .as_array()
                .is_some_and(|parameters| { parameters.iter().all(|p| p["name"] != "If-Match") })
        );
        let schemas = &doc["components"]["schemas"];
        assert!(schemas["Document"]["properties"].get("realtime").is_none());
        assert!(schemas["StaleWrite_Document"]["properties"].is_object());
        assert!(
            schemas["StaleWrite_Document"]["properties"]
                .get("etag")
                .is_none()
        );
        for removed in ["SaveDocument", "SaveConflict", "ConflictBody"] {
            assert!(schemas.get(removed).is_none(), "{removed}");
        }
        assert!(resource["get"]["responses"]["200"]["headers"]["ETag"].is_object());
        assert!(resource["get"]["responses"]["304"].is_object());
    }
}
