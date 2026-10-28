use super::model::{Trash, TrashEntry, TrashKind};
use crate::SystemState;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::{UserId, WorkspaceId},
};
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;
use time::OffsetDateTime;
use uuid::Uuid;

type EntryRow = (
    Uuid,
    String,
    Uuid,
    String,
    i32,
    UserId,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
    bool,
);

async fn entries(
    tx: &mut UserTx,
    workspace: Option<WorkspaceId>,
) -> Result<Vec<TrashEntry>, AppError> {
    let rows: Vec<EntryRow> = sqlx::query_as("SELECT * FROM app_trash_list($1)")
        .bind(workspace)
        .fetch_all(tx.conn())
        .await?;
    let now = OffsetDateTime::now_utc();
    let expired: Vec<Uuid> = rows.iter().filter(|r| r.8 <= now).map(|r| r.0).collect();
    let kept: Vec<Uuid> = if expired.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT * FROM app_trash_kept_for_references($1)")
            .bind(&expired)
            .fetch_all(tx.conn())
            .await?
    };
    rows.into_iter()
        .map(
            |(
                id,
                kind,
                root_id,
                title,
                member_count,
                deleted_by,
                name,
                deleted_at,
                expires_at,
                can_purge,
            )| {
                Ok(TrashEntry {
                    id,
                    kind: TrashKind::parse(&kind)
                        .ok_or_else(|| AppError::Config(format!("unknown trash kind {kind}")))?,
                    root_id,
                    title,
                    member_count,
                    deleted_by,
                    deleted_by_name: name,
                    deleted_at,
                    expires_at,
                    expired: expires_at <= now,
                    kept_for_references: kept.contains(&id),
                    can_purge,
                })
            },
        )
        .collect()
}

/// 워크스페이스 휴지통(관리자는 전부, 작성자는 자기가 지운 것만).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/trash", operation_id = "workspace_trash",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trash), (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn workspace_trash(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Trash>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
    let (retention_days, version_retention_days): (i32, Option<i32>) = sqlx::query_as(
        "SELECT trash_retention_days, version_retention_days FROM workspaces WHERE id = $1",
    )
    .bind(ws)
    .fetch_one(tx.conn())
    .await?;
    let entries = entries(&mut tx, Some(ws)).await?;
    tx.commit().await?;
    Ok(Json(Trash {
        retention_days,
        version_retention_days,
        entries,
    }))
}

/// 내 개인 노트 휴지통(보존 30일).
#[utoipa::path(get, path = "/api/me/trash", operation_id = "my_trash",
    responses((status = 200, body = Trash)), tag = "trash")]
pub async fn my_trash(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Trash>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let entries = entries(&mut tx, None).await?;
    tx.commit().await?;
    Ok(Json(Trash {
        retention_days: 30,
        version_retention_days: None,
        entries,
    }))
}
