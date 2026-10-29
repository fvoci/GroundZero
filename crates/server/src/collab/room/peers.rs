//! 참여자 표시, 전송과 세션·접근 등급 재확인.

use super::{
    AccessCheck, AppError, Arc, AwarenessUpdate, AwarenessUpdateEntry, ClientID, CloseReason, Db,
    DocumentId, Duration, Encode, HashMap, Instant, MAX_BYTES_PER_SECOND, MAX_MESSAGES_PER_SECOND,
    MAX_PRESENCE_BYTES, MAX_PRESENCE_IDS, MSG_PERSISTED, Message, Outbound, Peer, PeerAccess,
    ReadTxn, Room, Snapshot, SyncMessage, Transact, UserId, mpsc,
};

impl Room {
    /// 들어온 연결에 처음 보낼 메시지들. 메시지마다 프레임 하나다(y-websocket은 한 프레임에서 첫 메시지만 읽는다).
    pub(super) fn start_messages(&self) -> Vec<Vec<u8>> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        let sv = doc.transact().state_vector();
        let mut out = vec![Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1()];
        if let Some(update) = self.presence_update() {
            out.push(Message::Awareness(update).encode_v1());
        }
        // 이미 저장된 상태도 알린다: 다시 붙은 클라이언트의 편집이 그 안에 있으면 새 저장 없이 확인된다.
        if let Some(snapshot) = &self.persisted {
            out.push(Message::Custom(MSG_PERSISTED, snapshot.clone()).encode_v1());
        }
        out
    }

    /// 지금 있는 참여자 표시 전부(없으면 `None`).
    pub(super) fn presence_update(&self) -> Option<AwarenessUpdate> {
        let clients: HashMap<ClientID, AwarenessUpdateEntry> = self
            .presence
            .iter()
            .map(|(id, e)| (*id, e.clone()))
            .collect();
        (!clients.is_empty()).then_some(AwarenessUpdate { clients })
    }

    /// 1초 창 안의 메시지·바이트가 한도 안이면 참.
    pub(super) fn within_rate(&mut self, conn: u64, messages: u32, bytes: usize) -> bool {
        let Some(c) = self.conns.get_mut(&conn) else {
            return false;
        };
        if c.window.elapsed() >= Duration::from_secs(1) {
            c.window = Instant::now();
            c.messages = 0;
            c.bytes = 0;
        }
        c.messages = c.messages.saturating_add(messages);
        c.bytes = c.bytes.saturating_add(bytes);
        c.messages <= MAX_MESSAGES_PER_SECOND && c.bytes <= MAX_BYTES_PER_SECOND
    }
    pub(super) fn awareness_update(&mut self, conn: u64, update: AwarenessUpdate) {
        // 이 연결이 처음 알린 id만, 더 새로운(시계가 큰) 항목만 받는다. 표시 내용(JSON 글)은 해석하지 않고 그대로 전한다.
        let mut fresh = HashMap::new();
        for (id, entry) in update.clients {
            let owner = self.presence_owner.get(&id).copied();
            if owner.is_some_and(|o| o != conn) {
                continue;
            }
            let owned_count = self.conns.get(&conn).map_or(0, |c| c.clients.len());
            if owner.is_none() && owned_count >= MAX_PRESENCE_IDS {
                continue;
            }
            let newer = self
                .presence
                .get(&id)
                .is_none_or(|old| entry.clock > old.clock);
            if !newer || entry.json.len() > MAX_PRESENCE_BYTES {
                continue;
            }
            if &*entry.json == "null" {
                // 그 참여자가 떠났다.
                self.presence.remove(&id);
            } else {
                self.presence.insert(id, entry.clone());
            }
            self.presence_owner.insert(id, conn);
            if let Some(c) = self.conns.get_mut(&conn) {
                c.clients.insert(id);
            }
            fresh.insert(id, entry);
        }
        if fresh.is_empty() {
            return;
        }
        let encoded = Message::Awareness(AwarenessUpdate { clients: fresh }).encode_v1();
        // 보낸 연결에도 돌려준다. y-websocket은 30초 동안 아무것도 받지 못하면 끊고 다시 붙으므로, 혼자 편집할 때는
        // 15초마다 오는 자기 표시 갱신이 연결을 살려 둔다(참조 서버와 같다).
        self.broadcast(&encoded, None);
    }
    /// 닫힌 연결(웹소켓이 끝났는데 나감 알림이 대기열에 못 들어간 것)을 걸러 낸다.
    pub(super) fn prune(&mut self) {
        let closed: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.peer.out.is_closed() || c.peer.access.permission().is_none())
            .map(|(id, _)| *id)
            .collect();
        for id in closed {
            self.leave(id);
        }
    }

    pub(super) fn leave(&mut self, conn: u64) {
        let Some(connection) = self.conns.remove(&conn) else {
            return;
        };
        // 나간 연결의 참여자 표시를 지운다(시계를 올린 "null" 항목을 보내면 브라우저도 지운다).
        let mut gone = HashMap::new();
        for id in connection.clients {
            self.presence_owner.remove(&id);
            if let Some(old) = self.presence.remove(&id) {
                gone.insert(
                    id,
                    AwarenessUpdateEntry {
                        clock: old.clock.saturating_add(1),
                        json: Arc::from("null"),
                    },
                );
            }
        }
        if !gone.is_empty() {
            self.broadcast(
                &Message::Awareness(AwarenessUpdate { clients: gone }).encode_v1(),
                None,
            );
        }
    }

    pub(super) fn close(&mut self, conn: u64, reason: CloseReason) {
        if let Some(c) = self.conns.get(&conn) {
            c.peer.access.close(reason);
            let _ignored = c.peer.out.try_send(Outbound::Close(reason));
        }
        self.leave(conn);
    }

    pub(super) fn send(&mut self, out: &mpsc::Sender<Outbound>, bytes: Vec<u8>, conn: u64) {
        if !self
            .conns
            .get(&conn)
            .is_some_and(|c| c.peer.access.permission().is_some())
        {
            return;
        }
        if out.try_send(Outbound::Binary(bytes)).is_err() {
            self.close(conn, CloseReason::Busy);
        }
    }

    pub(super) fn broadcast(&mut self, bytes: &[u8], except: Option<u64>) {
        let slow: Vec<u64> = self
            .conns
            .iter()
            .filter(|(id, c)| Some(**id) != except && c.peer.access.permission().is_some())
            .filter(|(_, c)| {
                c.peer
                    .out
                    .try_send(Outbound::Binary(bytes.to_vec()))
                    .is_err()
            })
            .map(|(id, _)| *id)
            .collect();
        for id in slow {
            self.close(id, CloseReason::Busy);
        }
    }
    /// 독립 감시가 닫은 연결을 정리한다. 남은 편집은 기존 방 종료 경로에서 보존한다.
    pub(super) async fn recheck(&mut self) {
        self.recheck_peers().await;
    }

    /// 이동 직전 등 명시적인 재확인도 유지한다. peer별로 병렬 확인하며 기존 기한보다 오래 기다리지 않는다.
    pub(super) async fn recheck_peers(&mut self) {
        self.refresh_peers().await;
        self.remove_closed_peers();
    }

    pub(super) async fn refresh_peers(&self) {
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        let checks = peers.iter().map(|(_, peer)| async {
            let started = tokio::time::Instant::now();
            if let Ok(observed) = peer
                .access
                .until_closed(observe_access(&self.db, self.doc_id, peer))
                .await
            {
                peer.access.observe(started, observed);
            }
        });
        futures_util::future::join_all(checks).await;
    }

    /// 주기 처리는 독립 감시의 결과만 소비하고 DB 검사를 중복해서 시작하지 않는다.
    pub(super) fn remove_closed_peers(&mut self) {
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        for (conn, peer) in peers {
            if let Some(reason) = peer.access.reason() {
                self.close(conn, reason);
            }
        }
    }
}

/// 그 연결의 세션이 아직 그 사람의 것인지. DB 오류로 알 수 없으면 `None`.
pub(super) async fn session_alive(db: &Db, peer: &Peer) -> Option<bool> {
    let session = async {
        let mut tx = db.begin_anonymous().await?;
        let user: Option<UserId> = sqlx::query_scalar("SELECT app_session_user($1)")
            .bind(&peer.session)
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        Ok::<Option<UserId>, AppError>(user)
    }
    .await;
    match session {
        Ok(user) => Some(user == Some(peer.user)),
        Err(err) => {
            // R2 A: 알 수 없으면 연결을 닫는다. 닫힌 까닭(DB 오류)을 기한 초과와 구분해 로그에 남긴다.
            tracing::warn!(
                event = "collab.session_check_failed",
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(&err),
                "could not confirm the live editing session"
            );
            None
        }
    }
}

/// 세션이 살아 있고 문서를 실시간으로 열 수 있는지.
pub async fn peer_access(db: &Db, doc: DocumentId, peer: &Peer) -> PeerAccess {
    observe_access(db, doc, peer).await.access
}

pub(in crate::collab) async fn observe_access(
    db: &Db,
    doc: DocumentId,
    peer: &Peer,
) -> AccessCheck {
    match session_alive(db, peer).await {
        Some(true) => {}
        Some(false) => return PeerAccess::Denied.into(),
        None => return PeerAccess::Unknown.into(),
    }
    let access = async {
        let mut tx = db.begin_anonymous().await?;
        // begin_as의 set_config SELECT보다 먼저 격리를 정한다. 예외로 끝나는 권한 판정도
        // 그 판정과 같은 snapshot을 보존하며 mover 신원이나 문서 위치를 추가로 읽지 않는다.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(tx.conn())
            .await?;
        let snapshot = Arc::new(Snapshot::read(&mut tx).await?);
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(peer.user.0.to_string())
            .execute(tx.conn())
            .await?;
        let access: Result<String, AppError> = sqlx::query_scalar("SELECT app_collab_access($1)")
            .bind(doc)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function);
        if access.is_ok() {
            tx.commit().await?;
        }
        Ok::<_, AppError>((access, snapshot))
    }
    .await;
    let (access, snapshot) = match access {
        Ok(observed) => observed,
        Err(err) => {
            tracing::warn!(
                event = "collab.access_check_failed",
                document = %doc.0,
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(&err),
                "could not confirm live editing access"
            );
            return PeerAccess::Unknown.into();
        }
    };
    let access = match access {
        Ok(a) => PeerAccess::Allowed {
            can_write: matches!(a.as_str(), "owner" | "write"),
        },
        Err(AppError::NotFound | AppError::Forbidden) => PeerAccess::Denied,
        Err(err) => {
            tracing::warn!(
                event = "collab.access_check_failed",
                document = %doc.0,
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(&err),
                "could not confirm live editing access"
            );
            PeerAccess::Unknown
        }
    };
    AccessCheck {
        access,
        snapshot: Some(snapshot),
    }
}
