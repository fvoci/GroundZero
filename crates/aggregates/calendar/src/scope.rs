//! 공간·모듈·항목의 달력 범위(D-80).

use crate::calendar::CalendarRange;
use fvoci_core::error::AppError;
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_db::UserTx;

/// 달력 범위(D-80): 항목이면 그 항목 하나, 모듈이면 그 모듈의 볼 수 있는 항목들. 범위가 없으면 `None`(워크스페이스 전체).
/// 그 워크스페이스에 없거나 볼 수 없으면 404.
pub(super) async fn calendar_scope(
    tx: &mut UserTx,
    ws: WorkspaceId,
    range: &CalendarRange,
) -> Result<Option<std::collections::HashSet<ItemId>>, AppError> {
    if let Some(item) = range.item_id {
        let found: Option<ItemId> =
            sqlx::query_scalar("SELECT id FROM items WHERE id = $1 AND workspace_id = $2")
                .bind(item)
                .bind(ws)
                .fetch_optional(tx.conn())
                .await?;
        let item = found.ok_or(AppError::NotFound)?;
        if let Some(module) = range.module_id {
            let inside: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM items WHERE id = $1 AND module_id = $2)",
            )
            .bind(item)
            .bind(module)
            .fetch_one(tx.conn())
            .await?;
            if !inside {
                return Err(AppError::NotFound);
            }
        }
        return Ok(Some(std::iter::once(item).collect()));
    }
    if let Some(module) = range.module_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM modules WHERE id = $1 AND workspace_id = $2)",
        )
        .bind(module)
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
        if !exists {
            return Err(AppError::NotFound);
        }
        let items: Vec<ItemId> = sqlx::query_scalar("SELECT id FROM items WHERE module_id = $1")
            .bind(module)
            .fetch_all(tx.conn())
            .await?;
        return Ok(Some(items.into_iter().collect()));
    }
    Ok(None)
}
