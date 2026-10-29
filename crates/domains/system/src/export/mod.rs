//! ZIP 반출 질의와 감사 트랜잭션. 스냅샷 전후의 트랜잭션 경계를 유지한다.

mod queries;
mod zip;

use crate::SystemState;
use axum::{
    extract::{Path, State},
    response::Response,
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::{DocumentId, WorkspaceId},
};
use fvoci_http::session::CurrentUser;
use queries::{PERSONAL_JSON, WORKSPACE_JSON};
use serde_json::Value;
use std::sync::Arc;
use zip::{entries_from, safe_name, stream_zip};

/// 반출 자리. 모자라면 409(잠시 뒤 다시).
fn export_permit(state: &SystemState) -> Result<tokio::sync::OwnedSemaphorePermit, AppError> {
    Arc::clone(&state.export_permits)
        .try_acquire_owned()
        .map_err(|_| AppError::Conflict("other exports are running; try again shortly".into()))
}

/// 워크스페이스 반출(관리자). 지금 볼 수 있는 모든 내용과 파일 버전을 ZIP으로.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/export", operation_id = "export_workspace",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "export")]
pub async fn workspace(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_owner(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
    let permit = export_permit(&state)?;
    // 실시간으로 편집 중인 문서는 본문(마지막 리비전)이 뒤처져 있을 수 있다. 읽기 전에 열린 방을 리비전으로 남긴다.
    let docs: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    // 방을 기다리는 동안 DB 연결을 쥐지 않는다(방이 리비전을 남길 때 연결이 필요하다). 다시 열고 권한도 다시 본다.
    tx.commit().await?;
    state
        .snapshots
        .snapshot(&docs.into_iter().map(DocumentId).collect())
        .await;
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_owner(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
    let data: Value = sqlx::query_scalar(WORKSPACE_JSON)
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    let name = data["workspace"]["name"]
        .as_str()
        .unwrap_or("workspace")
        .to_owned();
    sqlx::query("SELECT app_audit_workspace_export($1, $2)")
        .bind(ws)
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    let entries = entries_from(data, "documents", state.plain_text)?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(stream_zip(
        Arc::clone(&state.storage),
        entries,
        &format!("fvoci-{}-{date}.zip", safe_name(&name)),
        permit,
    ))
}

/// 내 개인 자료 반출: 개인 노트·첨부·내 시간 기록·강의 진도·고정·즐겨찾기.
#[utoipa::path(get, path = "/api/me/export", operation_id = "export_mine",
    responses((status = 200, content_type = "application/zip", body = Vec<u8>), (status = 409, body = ErrorBody)),
    tag = "export")]
pub async fn mine(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, AppError> {
    let permit = export_permit(&state)?;
    let mut tx = state.db.begin_as(user).await?;
    let data: Value = sqlx::query_scalar(PERSONAL_JSON)
        .bind(user)
        .fetch_one(tx.conn())
        .await?;
    sqlx::query("SELECT app_audit_personal_export($1)")
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    let entries = entries_from(data, "notes", state.plain_text)?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(stream_zip(
        Arc::clone(&state.storage),
        entries,
        &format!("fvoci-mine-{date}.zip"),
        permit,
    ))
}
