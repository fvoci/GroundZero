//! @ 링크·사용자 언급·역방향 링크·알림 (D-18, D-27, D-39).
//!
//! 본문의 참조 노드는 종류와 내부 id만 가진다. 표시 이름은 여기서 권한을 확인해 만든다.
//! 볼 수 없는 대상과 없는 대상은 같은 모양(`visible: false`)으로 돌려준다(존재 여부를 드러내지 않는다).
//! 링크·언급 기록과 알림 생성은 저장 트랜잭션 안의 DB 함수 `app_record_document_references`가 한다
//! (`crates/db/migrations/202610280003_documents.sql`). 새 UUIDv7은 앱이 공급하며(D-88), 언급은 접근 권한을 주지 않는다.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::documents::access;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, ItemId, WorkspaceId};
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;

/// 이 대상을 가리키는 문서(내가 볼 수 있는 것만).
#[derive(Debug, Serialize, ToSchema)]
pub struct Backlink {
    pub id: DocumentId,
    pub reference: Option<String>,
    pub title: String,
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
}

type BacklinkRow = (
    DocumentId,
    Option<String>,
    String,
    Option<WorkspaceId>,
    Option<ItemId>,
);

async fn backlinks_to(
    tx: &mut UserTx,
    kind: &str,
    target: Uuid,
) -> Result<Vec<Backlink>, AppError> {
    // document_links와 documents 모두 RLS로 출발 문서를 볼 수 있는 것만 남는다.
    let rows: Vec<BacklinkRow> = sqlx::query_as(
        "SELECT d.id, app_document_reference(d.id), d.title, d.workspace_id,
                CASE WHEN app_access('item', d.item_id) <> 'none' THEN d.item_id END
         FROM document_links l JOIN documents d ON d.id = l.document_id
         WHERE l.target_kind = $1 AND l.target_id = $2
         ORDER BY d.updated_at DESC",
    )
    .bind(kind)
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, reference, title, workspace_id, item_id)| Backlink {
            id,
            reference,
            title,
            workspace_id,
            item_id,
        })
        .collect())
}

/// 이 문서를 가리키는 문서. 대상 문서를 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/documents/{document_id}/backlinks",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn document_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let links = backlinks_to(&mut tx, "document", id.0).await?;
    tx.commit().await?;
    Ok(Json(links))
}

/// 이 업무를 가리키는 문서. 업무를 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/tasks/{task_id}/backlinks",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn task_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_access('task', $1) <> 'none'")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let links = backlinks_to(&mut tx, "task", id).await?;
    tx.commit().await?;
    Ok(Json(links))
}

/// 이 항목을 가리키는 문서. 항목을 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/backlinks",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn item_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let links = backlinks_to(&mut tx, "item", id.0).await?;
    tx.commit().await?;
    Ok(Json(links))
}

/// 공개 보기용: 참조 노드를 자리표시 글자 노드로 바꾼다(내부 id와 대상 존재를 익명 방문자에게 숨긴다).
/// 저장된 본문은 serde_json의 중첩 한도 안에서 읽혔으므로 재귀 깊이가 제한된다.
pub fn strip_references(node: &mut serde_json::Value) {
    use serde_json::Value;
    match node {
        Value::Array(items) => items.iter_mut().for_each(strip_references),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("reference") {
                *node = serde_json::json!({ "type": "text", "text": "[참조]" });
                return;
            }
            map.values_mut().for_each(strip_references);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_bodies_drop_reference_ids() {
        let id = "0198f1f2-0000-7000-8000-000000000001";
        let mut body = serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
            { "type": "text", "text": "참고 " },
            { "type": "reference", "attrs": { "kind": "item", "id": id } }
        ] }] });
        strip_references(&mut body);
        assert!(!body.to_string().contains(id));
        assert_eq!(body["content"][0]["content"][1]["text"], "[참조]");
    }
}
