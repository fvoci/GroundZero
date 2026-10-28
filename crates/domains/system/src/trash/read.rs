use super::model::{RetentionSettings, TrashEntry, TrashExpiry, TrashKind, TrashScope};
use crate::SystemState;
use axum::{
    Json,
    extract::{Path, Query, State},
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
                    expiry: if expires_at > now {
                        TrashExpiry::Active
                    } else if kept.contains(&id) {
                        TrashExpiry::KeptForReferences
                    } else {
                        TrashExpiry::Expired
                    },
                    can_purge,
                })
            },
        )
        .collect()
}

/// 휴지통 묶음 모음. 공간은 관리자·지운 사람의 범위, 개인은 본인 범위로 읽는다.
#[utoipa::path(get, path = "/api/trash-batches", operation_id = "list_trash_batches",
    params(TrashScope),
    responses((status = 200, body = Vec<TrashEntry>), (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn list(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Query(scope): Query<TrashScope>,
) -> Result<Json<Vec<TrashEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    if let Some(ws) = scope.workspace_id {
        fvoci_db::access::require_visible(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
    }
    let entries = entries(&mut tx, scope.workspace_id).await?;
    tx.commit().await?;
    Ok(Json(entries))
}

/// 공간 보존 설정. 공간을 볼 수 있는 사람에게 기존과 같은 설정만 공개한다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/retention", operation_id = "workspace_retention",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = RetentionSettings), (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn retention(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<RetentionSettings>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
    let (retention_days, version_retention_days) = sqlx::query_as(
        "SELECT trash_retention_days, version_retention_days FROM workspaces WHERE id = $1",
    )
    .bind(ws)
    .fetch_one(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(RetentionSettings {
        retention_days,
        version_retention_days,
    }))
}
