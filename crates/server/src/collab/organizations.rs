//! 조직 도메인이 선언한 실행 훅에 방 보존을 연결한다.

use super::Rooms;
use crate::config::Limits;
use fvoci_core::{
    error::AppError,
    ids::{DocumentId, UserId},
};
use fvoci_db::Db;
use fvoci_organizations::runtime::{OrganizationRuntime, PrepareReply, PreparedRooms, RoomChange};
use std::{collections::HashSet, time::Duration};
use uuid::Uuid;

#[derive(Debug)]
pub struct OrganizationCollab {
    pub rooms: Rooms,
    pub limits: Limits,
    /// 기존 권한 재확인 주기를 준비 전체의 시한으로 재사용한다.
    pub prepare_timeout: Duration,
}

impl OrganizationRuntime for OrganizationCollab {
    fn prepare<'a>(&'a self, db: &'a Db, user: UserId, change: RoomChange) -> PrepareReply<'a> {
        let (rooms, limits, db) = (self.rooms.clone(), self.limits, db.clone());
        let timeout = self.prepare_timeout;
        Box::pin(async move {
            // HTTP 요청 취소로 중간에 guard가 풀리지 않는다. 준비 작업이 끝나면 결과와 함께 해제한다.
            tokio::spawn(async move {
                // 시한은 spawn 안에 둔다. 넘으면 준비 future와 그 트랜잭션·guard를 함께 버린다.
                tokio::time::timeout(timeout, prepare(&rooms, &db, &limits, user, change))
                    .await
                    .map_err(|_| {
                        AppError::Conflict(
                            "live editing preservation took too long; the command was not applied, try again"
                                .into(),
                        )
                    })?
            })
                .await
                .map_err(|_| {
                    AppError::Conflict(
                        "live editing preservation was interrupted; try again".into(),
                    )
                })?
        })
    }
}

async fn pending(
    db: &Db,
    user: UserId,
    change: RoomChange,
) -> Result<Vec<(DocumentId, Option<Uuid>)>, AppError> {
    let mut tx = db.begin_as(user).await?;
    let docs = sqlx::query_as(
        "SELECT document_id, workspace_id FROM app_collab_command_pending($1,$2,$3)",
    )
    .bind(change.action)
    .bind(change.scope)
    .bind(change.member)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(docs)
}

async fn prepare(
    rooms: &Rooms,
    db: &Db,
    limits: &Limits,
    user: UserId,
    change: RoomChange,
) -> Result<Box<dyn PreparedRooms>, AppError> {
    // SQL이 해당 명령의 권한을 검사한 뒤에만 방에 부작용을 낸다.
    let docs = pending(db, user, change).await?;
    let mut workspaces: HashSet<Uuid> = docs.iter().filter_map(|(_, ws)| *ws).collect();
    if change.action == "organization_remove" {
        let mut tx = db.begin_as(user).await?;
        let visible: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM workspaces WHERE organization_id=$1")
                .bind(change.scope)
                .fetch_all(tx.conn())
                .await?;
        tx.commit().await?;
        workspaces.extend(visible);
    } else {
        workspaces.insert(change.scope);
    }
    let (mut guard, docs) = rooms.hold_organization(
        &workspaces,
        &docs,
        (change.action == "organization_remove").then_some(change.scope),
    )?;
    for (doc, workspace) in docs {
        rooms
            .prepare_organization_document(db, limits, doc, user)
            .await
            .map_err(|_| blocked(doc, workspace))?;
    }
    // 이미 닫히는 중이던 방의 마지막 저장까지 포함해 한 번 더 확인한다.
    for (doc, workspace) in pending(db, user, change).await? {
        guard.include_document(rooms, doc)?;
        rooms
            .prepare_organization_document(db, limits, doc, user)
            .await
            .map_err(|_| blocked(doc, workspace))?;
    }
    Ok(Box::new(guard))
}

fn blocked(doc: DocumentId, workspace: Option<Uuid>) -> AppError {
    let Some(workspace) = workspace else {
        return AppError::Conflict(format!(
            "document {} has pending edits; nothing was changed. Ask the document owner or an editor to save the edits and retry",
            doc.0
        ));
    };
    AppError::Conflict(format!(
        "document {} in workspace {} has pending edits; nothing was changed. Ask an editor to save the edits, or join as admin and retry. Restore trashed content first if needed",
        doc.0, workspace
    ))
}
