//! 문서 행 읽기와 리비전·참조 저장.

use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_core::ids::ItemId;
use fvoci_core::ids::UserId;
use fvoci_core::ids::WorkspaceId;
use fvoci_db::UserTx;
use fvoci_db::needed_ids;
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Access, Document, DocumentSummary, SCHEMA_VERSION, access};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DocRow {
    id: DocumentId,
    workspace_id: Option<WorkspaceId>,
    item_id: Option<ItemId>,
    parent_id: Option<DocumentId>,
    placement: String,
    reference: Option<String>,
    pub title: String,
    pub body: Value,
    schema_version: i32,
    pub revision: i32,
    updated_at: OffsetDateTime,
    visibility: String,
    allow_copy: bool,
    can_manage: bool,
    can_delete: bool,
    is_template: bool,
    pub realtime: bool,
    can_export: bool,
}

/// 참조 문자열은 저장하지 않고 현재 키로 만든다(키가 바뀌어도 번호·id는 그대로, D-18).
/// 항목 id는 그 항목을 볼 수 있는 사람에게만 준다(공유로만 문서를 보는 사람에게 비공개 항목을 드러내지 않는다).
const DOC_COLUMNS: &str = "id, workspace_id, \
    CASE WHEN app_access('item', item_id) <> 'none' THEN item_id END AS item_id, \
    CASE WHEN app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id, placement, \
    app_document_reference(id) AS reference, title, body, schema_version, revision, updated_at, \
    visibility, allow_copy, app_document_can_manage(id) AS can_manage, \
    coalesce(app_access('document', id) = 'owner' \
      OR (app_access('document', id) = 'write' AND created_by = app_user_id()), false) AS can_delete, \
    is_template, app_document_realtime(id) AS realtime, app_document_can_export(id) AS can_export";

pub(crate) fn to_document(row: DocRow, access: Access) -> Document {
    Document {
        id: row.id,
        workspace_id: row.workspace_id,
        item_id: row.item_id,
        parent_id: row.parent_id,
        reference: row.reference,
        placement: row.placement,
        title: row.title,
        body: row.body,
        schema_version: row.schema_version,
        revision: row.revision,
        updated_at: row.updated_at,
        access,
        can_edit: access.can_write(),
        visibility: row.visibility,
        allow_copy: row.allow_copy,
        can_manage: row.can_manage,
        can_delete: row.can_delete,
        is_template: row.is_template,
        realtime: row.realtime,
        can_export: row.can_export,
    }
}

pub(super) async fn load(tx: &mut UserTx, id: DocumentId) -> Result<Option<DocRow>, AppError> {
    let row = sqlx::query_as(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row)
}

/// 행을 잠그고 읽는다. 같은 문서의 저장은 직렬화되어 재시도 판단과 리비전 비교가 최신 행 기준이 된다.
pub async fn load_for_update(tx: &mut UserTx, id: DocumentId) -> Result<Option<DocRow>, AppError> {
    let row = sqlx::query_as(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row)
}

/// 문서와 권한을 함께 읽는다. 볼 수 없으면 404.
pub(crate) async fn load_with_access(
    tx: &mut UserTx,
    id: DocumentId,
) -> Result<(DocRow, Access), AppError> {
    let access = access(tx, id).await?.ok_or(AppError::NotFound)?;
    let row = load(tx, id).await?.ok_or(AppError::NotFound)?;
    Ok((row, access))
}

/// 목록 조회 열. `SummaryRow`와 순서가 같다.
/// 목록 조회 열. `SummaryRow`와 순서가 같다. 부모는 볼 수 있을 때만 준다(아니면 그 문서는 최상위로 보인다).
pub(crate) const SUMMARY_COLUMNS: &str = "id, app_document_reference(id) AS reference, title, revision, \
    updated_at, CASE WHEN app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id";

/// 트리 순서. 같은 부모 아래에서 위치, 같으면 번호·만든 순서.
pub(crate) const TREE_ORDER: &str = "position, number NULLS LAST, created_at, id";

pub(crate) type SummaryRow = (
    DocumentId,
    Option<String>,
    String,
    i32,
    OffsetDateTime,
    Option<DocumentId>,
);

pub(crate) fn to_summaries(rows: Vec<SummaryRow>) -> Vec<DocumentSummary> {
    rows.into_iter()
        .map(
            |(id, reference, title, revision, updated_at, parent_id)| DocumentSummary {
                id,
                reference,
                title,
                revision,
                updated_at,
                parent_id,
            },
        )
        .collect()
}

/// 저장의 끝부분: 문서 행을 새 리비전으로 바꾸고 리비전·링크·언급을 기록한다. 저장 API와 실시간 편집의
/// 리비전(collab)이 같은 길을 쓴다. 호출하는 쪽이 문서 행을 잠그고 쓰기 권한을 확인한다. `edited_by`: 실시간 방이 고친
/// 사람 대신 남기는 리비전의 실제 편집자(D-95). 작성자는 늘 이 트랜잭션의 사용자다(`document_revisions_insert`).
pub async fn write_revision(
    tx: &mut UserTx,
    id: DocumentId,
    title: &str,
    body: &Value,
    request_id: Uuid,
    edited_by: Option<&[UserId]>,
    file_ref_editors: Option<&Value>,
) -> Result<DocRow, AppError> {
    let row: DocRow = sqlx::query_as(&format!(
        "UPDATE documents
         SET title = $2, body = $3, schema_version = $4, revision = revision + 1,
             updated_by = app_user_id(), updated_at = now()
         WHERE id = $1
         RETURNING {DOC_COLUMNS}"
    ))
    .bind(id)
    .bind(title)
    .bind(body)
    .bind(SCHEMA_VERSION)
    .fetch_optional(tx.conn())
    .await?
    // 잠금 뒤 권한이 바뀌어 갱신 정책이 행을 거른 경우(예: 그사이 항목 보관): 500이 아니라 403.
    .ok_or(AppError::Forbidden)?;
    let edited_by: Option<Vec<Uuid>> = edited_by.map(|users| users.iter().map(|u| u.0).collect());
    sqlx::query(
        "INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id,
                                         edited_by, file_ref_editors)
         VALUES ($1, $2, $3, $4, $5, app_user_id(), $6, $7, $8)",
    )
    .bind(id)
    .bind(row.revision)
    .bind(title)
    .bind(SCHEMA_VERSION)
    .bind(body)
    .bind(request_id)
    .bind(edited_by)
    .bind(file_ref_editors)
    .execute(tx.conn())
    .await?;
    // 제목이 바뀌었으면 감사 기록을 남긴다(이전·새 제목은 함수가 두 리비전에서 읽는다, 본문은 남기지 않는다).
    sqlx::query("SELECT app_audit_document_rename($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(row.revision)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 링크·언급은 방금 저장한 본문에서 뽑아 같은 트랜잭션에 기록한다.
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_record_document_references_ids($1, true)").bind(id),
    )
    .await?;
    sqlx::query("SELECT app_record_document_references($1, true, $2)")
        .bind(id)
        .bind(ids)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    Ok(row)
}
