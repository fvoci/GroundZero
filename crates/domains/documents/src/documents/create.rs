//! 위키·개인 노트 생성과 템플릿 초기 본문.

use crate::state::DocumentsState;
use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use fvoci_core::error::AppError;
use fvoci_core::error::ErrorBody;
use fvoci_core::ids::DocumentId;
use fvoci_core::ids::ItemId;
use fvoci_core::ids::WorkspaceId;
use fvoci_core::validate::validate_title;
use fvoci_db::UserTx;
use fvoci_db::is_unique_violation;
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;
use fvoci_http::session::ExpectedUser;
use serde_json::Value;

use super::{
    CreateDocument, Document, DocumentSummary, EXPECTED_USER_DOC, SCHEMA_VERSION, SUMMARY_COLUMNS,
    SummaryRow, TREE_ORDER, empty_body, load_with_access, max_depth, to_document, to_summaries,
};

/// 템플릿 본문을 받을 새 문서의 자리.
#[derive(Debug, Clone, Copy)]
pub(crate) enum NewDocumentPlace {
    PersonalNote,
    Wiki(WorkspaceId),
    Item(ItemId),
}

/// 새 문서의 본문: 템플릿을 주면 그 본문(읽을 수 있는 템플릿이어야 함, 아니면 404), 아니면 빈 문서.
/// 공간에 만들 때 공간 문서 템플릿은 같은 조직 것만 쓰고(409) 원본에 복제 감사를 남긴다(D-114·D-115). 같은
/// `request_id`로 다시 보낸 생성에는 감사를 다시 남기지 않는다.
pub(crate) async fn initial_body(
    tx: &mut UserTx,
    input: &CreateDocument,
    place: NewDocumentPlace,
) -> Result<Value, AppError> {
    let Some(template) = input.template_id else {
        return Ok(empty_body());
    };
    let (workspace, item) = match place {
        NewDocumentPlace::PersonalNote => {
            // 문서 RLS가 읽을 수 없는 템플릿을 거른다.
            let body: Option<Value> = sqlx::query_scalar(
                "SELECT body FROM documents WHERE id = $1 AND is_template AND deleted_at IS NULL",
            )
            .bind(template)
            .fetch_optional(tx.conn())
            .await?;
            return body.ok_or(AppError::NotFound);
        }
        NewDocumentPlace::Wiki(workspace) => (Some(workspace), None),
        NewDocumentPlace::Item(item) => (None, Some(item)),
    };
    sqlx::query_scalar("SELECT app_template_body($1, $2, $3, $4, $5)")
        .bind(template)
        .bind(workspace)
        .bind(item)
        .bind(input.request_id)
        .bind(new_ids(1))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)
}

/// 위키 문서 목록. 볼 수 있는 문서만(비공개 문서는 작성자·관리자·공유 대상에게만) 나온다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/wiki/documents",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [DocumentSummary]), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list_wiki(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Workspace, ws).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents
         WHERE workspace_id = $1 AND placement = 'wiki' ORDER BY {TREE_ORDER}"
    ))
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 위키 문서 생성. 번호 할당·문서·리비전 1·감사 기록은 정의자 함수 `app_create_wiki_document`가
/// 한 번에 만든다. 같은 사용자의 같은 `request_id`는 같은 문서를 돌려준다.
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/wiki/documents",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = CreateDocument,
    responses((status = 201, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "다른 조직 공간의 템플릿")), tag = "documents")]
pub async fn create_wiki(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let grade: String = sqlx::query_scalar("SELECT app_access('workspace', $1)")
            .bind(ws)
            .fetch_one(tx.conn())
            .await?;
        // 볼 수 없는 워크스페이스는 404(존재를 드러내지 않음), 읽기만 되면 403(D-113).
        match grade.as_str() {
            "owner" | "write" => {}
            "none" => return Err(AppError::NotFound),
            _ => return Err(AppError::Forbidden),
        }
        let body = initial_body(&mut tx, &input, NewDocumentPlace::Wiki(ws)).await?;
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_wiki_document($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(ws)
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
        // 같은 request_id가 동시에 들어와 다른 쪽이 먼저 만들었다. 다시 부르면 그 문서를 돌려받는다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 내 개인 노트 목록. 워크스페이스 관리자에게도 보이지 않는다 (D-22, D-49).
#[utoipa::path(get, path = "/api/me/notes",
    responses((status = 200, body = [DocumentSummary]), (status = 401, body = ErrorBody)), tag = "documents")]
pub async fn my_notes(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents
         WHERE owner_user_id = app_user_id() ORDER BY {TREE_ORDER}"
    ))
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 개인 노트 생성.
///
/// 기대 사용자 머리가 필요하다(초안을 내 노트로 보존하는 길이 이 요청으로 시작한다, `ExpectedUser`).
#[utoipa::path(post, path = "/api/me/notes", request_body = CreateDocument,
    params(("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_USER_DOC)),
    responses((status = 201, body = Document), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "`account_changed`: 기대 사용자가 세션의 사용자와 다름(아무것도 만들지 않음)")),
    tag = "documents")]
pub async fn create_note(
    State(state): State<DocumentsState>,
    ExpectedUser(user): ExpectedUser,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let body = initial_body(&mut tx, &input, NewDocumentPlace::PersonalNote).await?;
        let id: DocumentId =
            // 템플릿을 주면 SQL이 그 조직을 노트에 기억한다(#175=1).
            sqlx::query_scalar("SELECT app_create_personal_note($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(body)
                .bind(input.parent_id)
                .bind(max_depth(&state.limits))
                .bind(new_ids(3))
                .bind(input.template_id)
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
