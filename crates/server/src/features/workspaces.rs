//! 실시간 공동 편집의 공간 설정(조립 상태의 협업 방에 의존).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, WorkspaceId};
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;

/// 실시간 공동 편집 켜고 끄기 요청(D-27, D-70).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRealtime {
    pub enabled: bool,
}

/// 워크스페이스 문서의 실시간 공동 편집을 켜거나 끈다(관리자). 끄면 열려 있는 편집 방은 리비전을 남기고 닫힌다.
/// 리비전을 남기지 못한 방이 있으면 끄지 않고 409.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/realtime",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = SetRealtime,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)),
    tag = "workspaces")]
pub async fn set_realtime(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<SetRealtime>,
) -> Result<StatusCode, AppError> {
    // 끌 때: 새 방이 열리지 않게 막고, 열린 방의 편집을 모두 리비전으로 남긴 뒤에만 끈다. 실시간이 켜진 동안에는 저장
    // API의 본문 변경이 막혀(409) 그 사이에 끼어들 수 없고, 끈 뒤에는 남은 방이 없다. 리비전을 남기지 못한 방이
    // 있으면 끄지 않는다(편집을 버리지 않는다). 막음은 이 요청이 끝날 때 풀린다.
    let _closing = if input.enabled {
        None
    } else {
        let mut tx = state.db.begin_as(user).await?;
        let role: Option<String> = sqlx::query_scalar("SELECT app_member_role($1)")
            .bind(ws)
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        if role.as_deref() == Some("admin") {
            let guard = state.collab.begin_closing(ws.0);
            let mut saved = state.collab.close_workspace(ws.0, user).await;
            // 방 없이 남은 실시간 상태(방이 리비전을 남기지 못하고 닫힌 것)도 끄기 전에 리비전으로 남긴다.
            if saved {
                let mut tx = state.db.begin_as(user).await?;
                let pending: Vec<Uuid> = sqlx::query_scalar("SELECT app_collab_pending($1)")
                    .bind(ws)
                    .fetch_all(tx.conn())
                    .await
                    .map_err(AppError::from_db_function)?;
                tx.commit().await?;
                for doc in pending {
                    saved &= state
                        .collab
                        .save_pending(&state.db, &state.config.limits, DocumentId(doc), user)
                        .await;
                }
            }
            if !saved {
                return Err(AppError::Conflict(
                    "some live documents could not be saved as revisions; realtime stays on, try again"
                        .into(),
                ));
            }
            Some(guard)
        } else {
            None
        }
    };
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_workspace_realtime($1, $2, $3)")
        .bind(ws)
        .bind(input.enabled)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
