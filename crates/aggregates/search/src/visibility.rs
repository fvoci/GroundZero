//! 현재 사용자와 조직으로 검색 후보를 최종 확인한다.

use fvoci_core::error::AppError;
use fvoci_core::ids::{OrganizationId, WorkspaceId};
use fvoci_db::UserTx;
use std::collections::HashSet;
use uuid::Uuid;

fn quote(values: impl IntoIterator<Item = String>) -> String {
    // 값은 모두 UUID 문자열이라 따옴표 밖으로 나갈 수 없다.
    let parts: Vec<String> = values.into_iter().map(|v| format!("\"{v}\"")).collect();
    format!("[{}]", parts.join(", "))
}

pub(super) async fn scope_filter(
    tx: &mut UserTx,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
    organization: OrganizationId,
) -> Result<String, AppError> {
    let kind_filter = format!("kind IN {}", quote(kinds.iter().map(|k| (*k).to_owned())));
    if let Some(ws) = workspace {
        fvoci_db::access::require_visible(tx, fvoci_db::access::Kind::Workspace, ws).await?;
        return Ok(format!("({kind_filter}) AND workspace_id = \"{}\"", ws.0));
    }
    let workspaces: Vec<Uuid> = sqlx::query_scalar("SELECT app_my_workspace_ids($1)")
        .bind(organization)
        .fetch_all(tx.conn())
        .await?;
    let mut scope = vec![format!("owner_user_id = \"{}\"", tx.user().0)];
    if !workspaces.is_empty() {
        scope.push(format!(
            "workspace_id IN {}",
            quote(workspaces.iter().map(Uuid::to_string))
        ));
    }
    // 공유 표는 수신자에게 직접 열리지 않는다. 요청한 종류의 공유만 읽으며, 두 종류를 함께 찾을 때는
    // 공유 문서 집합을 한 번 읽어 첨부에도 쓴다. 공유 변경 때문에 재색인을 기다릴 필요는 없다.
    let shared_query = match (kinds.contains(&"document"), kinds.contains(&"file")) {
        (true, true) => Some(
            "WITH shared AS MATERIALIZED (SELECT id FROM app_shared_with_me($1))
             SELECT 'document'::text, id FROM shared
             UNION ALL
             SELECT 'file', f.id FROM files f JOIN shared s ON s.id = f.document_id",
        ),
        (true, false) => Some("SELECT 'document'::text, id FROM app_shared_with_me($1)"),
        (false, true) => Some(
            "SELECT 'file'::text, f.id FROM files f
             JOIN app_shared_with_me($1) s ON s.id = f.document_id",
        ),
        (false, false) => None,
    };
    if let Some(query) = shared_query {
        let shared: Vec<(String, Uuid)> = sqlx::query_as(query)
            .bind(organization)
            .fetch_all(tx.conn())
            .await?;
        for kind in ["document", "file"] {
            let ids: Vec<String> = shared
                .iter()
                .filter(|(k, _)| k == kind)
                .map(|(_, id)| id.to_string())
                .collect();
            if !ids.is_empty() {
                scope.push(format!("(kind = \"{kind}\" AND id IN {})", quote(ids)));
            }
        }
    }
    Ok(format!("({kind_filter}) AND ({})", scope.join(" OR ")))
}

pub(super) async fn visible(
    tx: &mut UserTx,
    candidates: &[(String, Uuid)],
    organization: Option<OrganizationId>,
) -> Result<HashSet<(String, Uuid)>, AppError> {
    let kinds: Vec<&str> = candidates.iter().map(|(k, _)| k.as_str()).collect();
    let ids: Vec<Uuid> = candidates.iter().map(|(_, id)| *id).collect();
    let shared_document = match (kinds.contains(&"document"), kinds.contains(&"file")) {
        (true, true) => Some("coalesce(d.id, fd.id)"),
        (true, false) => Some("d.id"),
        (false, true) => Some("fd.id"),
        (false, false) => None,
    };
    let shared_filter = shared_document
        .map(|id| format!(" OR {id} IN (SELECT id FROM app_shared_with_me($3))"))
        .unwrap_or_default();
    let query = format!(
        "SELECT v.kind, v.id FROM app_search_visible($1, $2) v
             LEFT JOIN documents d ON v.kind = 'document' AND d.id = v.id
             LEFT JOIN tasks t ON v.kind = 'task' AND t.id = v.id
             LEFT JOIN items i ON v.kind = 'item' AND i.id = v.id
             LEFT JOIN events e ON v.kind = 'event' AND e.id = v.id
             LEFT JOIN files f ON v.kind = 'file' AND f.id = v.id
             LEFT JOIN documents fd ON fd.id = f.document_id
             LEFT JOIN item_records r ON v.kind = 'record' AND r.id = v.id
             WHERE coalesce(d.workspace_id,t.workspace_id,i.workspace_id,e.workspace_id,f.workspace_id,r.workspace_id)
                       IN (SELECT app_my_workspace_ids($3))
                OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id())
                OR (f.workspace_id IS NULL AND fd.owner_user_id = app_user_id()){shared_filter}"
    );
    let rows: Vec<(String, Uuid)> = sqlx::query_as(&query)
        .bind(&kinds)
        .bind(&ids)
        .bind(organization)
        .fetch_all(tx.conn())
        .await?;
    Ok(rows.into_iter().collect())
}

/// 이 항목들 가운데 그 사람이 볼 수 있는 것(호출자 권한, 항목 RLS).
pub(super) async fn visible_items(
    tx: &mut UserTx,
    ids: &[Uuid],
) -> Result<HashSet<Uuid>, AppError> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM items WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(tx.conn())
        .await?;
    Ok(rows.into_iter().collect())
}

/// 파일이 붙은 문서(파일 결과에서 문서로 열 때).
pub(super) async fn file_document(
    tx: &mut UserTx,
    kind: &str,
    id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    if kind != "file" {
        return Ok(None);
    }
    Ok(
        sqlx::query_scalar("SELECT document_id FROM files WHERE id = $1")
            .bind(id)
            .fetch_optional(tx.conn())
            .await?
            .flatten(),
    )
}

/// 번호 표시. 호출자 권한으로 읽어 키를 볼 수 없는 사람(멤버가 아닌 공유 대상)에게는 없다.
pub(super) async fn reference(
    tx: &mut UserTx,
    kind: &str,
    id: Uuid,
) -> Result<Option<String>, AppError> {
    let sql = match kind {
        "document" => "SELECT app_document_reference($1)",
        "task" => "SELECT app_task_reference($1)",
        "item" => "SELECT app_item_reference($1)",
        _ => return Ok(None),
    };
    Ok(sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(tx.conn())
        .await?
        .flatten())
}

pub(super) fn parse_uuid(raw: Option<&str>) -> Option<Uuid> {
    raw.and_then(|r| Uuid::parse_str(r).ok())
}
