//! 조직 명령의 범위 막음과 체크포인트. 문서 이동·복원의 hold와 저장 경로를 공유한다.

use super::{
    AppError, Change, Command, Db, DocumentId, Handle, HashSet, HoldGuard, HoldKey, Limits, Rooms,
    UserId, Uuid, oneshot,
};
use fvoci_organizations::runtime::PreparedRooms;

type ScopedDocuments = Vec<(DocumentId, Option<Uuid>)>;

#[derive(Debug)]
pub(crate) struct Prepared {
    _guards: Vec<HoldGuard>,
}
impl PreparedRooms for Prepared {}

impl Prepared {
    pub(crate) fn include_document(
        &mut self,
        rooms: &Rooms,
        doc: DocumentId,
    ) -> Result<(), AppError> {
        if !self
            ._guards
            .iter()
            .any(|guard| guard.keys.contains(&HoldKey::Document(doc)))
        {
            let (guard, _) = rooms.hold(doc, &[], &[])?;
            self._guards.push(guard);
        }
        Ok(())
    }
}

impl Rooms {
    /// 문서의 현재 공간과 이동 목적지를 같은 목록 잠금 아래 확인한 뒤 기존 hold를 잡는다.
    pub(super) async fn hold_document(
        &self,
        db: &Db,
        user: UserId,
        doc: DocumentId,
        target: Option<Uuid>,
        target_item: Option<Uuid>,
    ) -> Result<(HoldGuard, Option<Handle>), AppError> {
        let mut tx = db.begin_as(user).await?;
        let source: Option<(Option<Uuid>, Option<Uuid>)> =
            sqlx::query_as("SELECT d.workspace_id, w.organization_id FROM documents d LEFT JOIN workspaces w ON w.id=d.workspace_id WHERE d.id=$1")
                .bind(doc)
                .fetch_optional(tx.conn())
                .await?;
        tx.commit().await?;
        let source = source.ok_or(AppError::NotFound)?;
        let target = if target.is_none()
            && let Some(item) = target_item
        {
            let mut tx = db.begin_as(user).await?;
            let ws = sqlx::query_scalar("SELECT m.workspace_id FROM items i JOIN modules m ON m.id=i.module_id WHERE i.id=$1")
                .bind(item).fetch_optional(tx.conn()).await?;
            tx.commit().await?;
            ws
        } else {
            target
        };
        let target_org: Option<Uuid> = if let Some(target) = target {
            let mut tx = db.begin_as(user).await?;
            let org = sqlx::query_scalar("SELECT organization_id FROM workspaces WHERE id=$1")
                .bind(target)
                .fetch_optional(tx.conn())
                .await?;
            tx.commit().await?;
            org
        } else {
            None
        };
        self.hold(
            doc,
            &[source.0, target].into_iter().flatten().collect::<Vec<_>>(),
            &[source.1, target_org]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
        )
    }

    /// 새 방과 문서 이동을 먼저 막는다. 준비 중 겹치는 명령은 기다리지 않고 409로 다시 확인하게 한다.
    pub(crate) fn hold_organization(
        &self,
        workspaces: &HashSet<Uuid>,
        pending: &[(DocumentId, Option<Uuid>)],
        organization: Option<Uuid>,
    ) -> Result<(Prepared, ScopedDocuments), AppError> {
        let rooms = self.lock();
        let mut docs = pending.to_vec();
        docs.extend(
            rooms
                .iter()
                .filter(|(_, h)| {
                    h.workspace.is_some_and(|ws| workspaces.contains(&ws))
                        || (organization.is_some() && h.organization == organization)
                })
                .map(|(id, h)| (*id, h.workspace)),
        );
        docs.sort_by_key(|(doc, _)| doc.0);
        docs.dedup_by_key(|(doc, _)| *doc);
        let keys: Vec<_> = workspaces
            .iter()
            .copied()
            .map(HoldKey::Workspace)
            .chain(docs.iter().map(|(doc, _)| HoldKey::Document(*doc)))
            .chain(organization.map(HoldKey::Organization))
            .collect();
        let mut held = self.held_set();
        if keys.iter().any(|key| {
            held.contains_key(key)
                || match key {
                    HoldKey::Workspace(ws) => held.contains_key(&HoldKey::DocumentWorkspace(*ws)),
                    HoldKey::Organization(org) => {
                        held.contains_key(&HoldKey::DocumentOrganization(*org))
                    }
                    _ => false,
                }
        }) {
            return Err(AppError::Conflict(
                "another command is preserving live edits; try again".into(),
            ));
        }
        let guards = keys
            .into_iter()
            .map(|key| {
                held.insert(key, 1);
                HoldGuard {
                    rooms: self.clone(),
                    keys: vec![key],
                    epoch: self.recovery.current(),
                }
            })
            .collect();
        Ok((Prepared { _guards: guards }, docs))
    }

    pub(crate) async fn prepare_organization_document(
        &self,
        db: &Db,
        limits: &Limits,
        doc: DocumentId,
        user: UserId,
    ) -> Result<(), AppError> {
        if let Some(handle) = self.live_room(doc) {
            let (done, answer) = oneshot::channel();
            handle
                .tx
                .try_send(Command::PrepareOrganization { user, done })
                .map_err(|_| AppError::Conflict("the live room is closing; try again".into()))?;
            answer.await.map_err(|_| {
                AppError::Conflict("the live room closed while preserving edits; try again".into())
            })??;
            return Ok(());
        }
        // 닫히던 방의 미리비전 상태도 기존 저장 경로에서 다시 확인한다.
        self.save_pending_before(db, limits, doc, user, Change::Organization)
            .await
    }
}
