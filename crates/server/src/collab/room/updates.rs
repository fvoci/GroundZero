//! 수신 메시지 해석과 검증 후 CRDT 변경 반영.

use super::{
    AppError, CloseReason, Decode, DecoderV1, Doc, EditAuthorization, Encode, FRAGMENT, HashMap,
    Instant, Limits, MAX_MESSAGES_PER_FRAME, Message, MessageReader, Pending, ReadTxn, Room,
    StateVector, SyncMessage, Transact, Update, UserId, Verdict, convert, documents, lineage_of,
};

impl Room {
    pub(super) fn incoming(&mut self, conn: u64, data: &[u8]) {
        self.incoming_with_authorization(conn, data, EditAuthorization::Current);
    }

    pub(super) fn incoming_with_authorization(
        &mut self,
        conn: u64,
        data: &[u8],
        authorization: EditAuthorization,
    ) {
        let Some(connection) = self.conns.get(&conn) else {
            return;
        };
        let permission = match &authorization {
            EditAuthorization::Current => connection.peer.access.permission(),
            EditAuthorization::BeforeMove { .. } => authorization
                .allows(&connection.peer.access)
                .then_some(true),
        };
        let Some(can_write) = permission else {
            if matches!(authorization, EditAuthorization::BeforeMove { .. }) {
                return;
            }
            let reason = connection.peer.access.reason().unwrap_or(CloseReason::Busy);
            self.close(conn, reason);
            return;
        };
        let (user, can_write, out) = (connection.peer.user, can_write, connection.peer.out.clone());
        let mut decoder = DecoderV1::from(data);
        let messages: Vec<Message> = MessageReader::new(&mut decoder)
            .take(MAX_MESSAGES_PER_FRAME + 1)
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        if (messages.is_empty() && !data.is_empty()) || messages.len() > MAX_MESSAGES_PER_FRAME {
            self.close(conn, CloseReason::Invalid);
            return;
        }
        let count = u32::try_from(messages.len()).unwrap_or(u32::MAX);
        if !self.within_rate(conn, count, data.len()) {
            self.close(conn, CloseReason::Busy);
            return;
        }
        for message in messages {
            match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let Some(doc) = &self.doc else {
                        continue;
                    };
                    let update = doc.transact().encode_state_as_update_v1(&sv);
                    self.send(
                        &out,
                        Message::Sync(SyncMessage::SyncStep2(update)).encode_v1(),
                        conn,
                    );
                }
                Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                    let empty = Update::decode_v1(&update).map_or(true, |u| u.is_empty());
                    if empty {
                        continue;
                    }
                    if !can_write {
                        // 읽기 연결의 변경은 받지 않는다(편집기가 스스로 맞춘 것일 수 있어 닫지는 않는다).
                        tracing::debug!(event = "collab.read_only_change", document = %self.doc_id.0, "ignored a change from a read-only connection");
                        continue;
                    }
                    self.pending.push(Pending {
                        conn,
                        user,
                        update,
                        authorization: authorization.clone(),
                    });
                }
                Message::Awareness(update) => self.awareness_update(conn, update),
                Message::AwarenessQuery => {
                    if let Some(update) = self.presence_update() {
                        self.send(&out, Message::Awareness(update).encode_v1(), conn);
                    }
                }
                Message::Auth(_) | Message::Custom(..) => {}
            }
        }
    }
    /// 연결별 수신 순서로 합치고, 검증 전후의 권한 확인을 각각 거쳐 반영한다.
    pub(super) async fn flush(&mut self) {
        for (conn, edits) in ordered_edits(std::mem::take(&mut self.pending)) {
            let Some(first) = edits.first() else { continue };
            if !self.edits_authorized(conn, &edits) {
                continue;
            }
            let Ok(merged) = yrs::merge_updates_v1(edits.iter().map(|edit| edit.update.as_slice()))
            else {
                self.close(conn, CloseReason::Invalid);
                continue;
            };
            let Some(doc) = &self.doc else { return };
            let state = doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            let verdict = self.validate_update(state, merged.clone()).await;
            if !self.accept_verdict(conn, verdict) {
                continue;
            }
            if !self.recheck_edit_session(conn, &first.authorization).await {
                continue;
            }
            // 본문 검사 동안 기한이 지나거나 권한이 철회됐을 수 있다. 검증 전 검사와 합치지 않는다.
            if !self.edits_authorized(conn, &edits) {
                continue;
            }
            if self.doc.is_none() {
                return;
            }
            self.apply_update(conn, first.user, merged);
        }
    }

    fn edits_authorized(&self, conn: u64, edits: &[Pending]) -> bool {
        self.conns.get(&conn).is_some_and(|c| {
            edits
                .iter()
                .all(|edit| edit.authorization.allows(&c.peer.access))
        })
    }

    async fn validate_update(&self, state: Vec<u8>, candidate: Vec<u8>) -> Verdict {
        let limits = self.limits;
        let lineage = self.lineage.clone();
        let max_state = self.max_state_bytes();
        tokio::task::spawn_blocking(move || {
            check_update(&state, &candidate, &lineage, &limits, max_state)
        })
        .await
        .unwrap_or(Verdict::Invalid("check task failed".into()))
    }

    fn accept_verdict(&mut self, conn: u64, verdict: Verdict) -> bool {
        match verdict {
            Verdict::Ok => true,
            Verdict::Reload => {
                self.close(conn, CloseReason::Reload);
                false
            }
            Verdict::Invalid(reason) => {
                // 이유는 우리가 붙인 고정 분류다(문서 내용은 담지 않는다, D-83).
                tracing::warn!(
                    event = "collab.edit_rejected",
                    document = %self.doc_id.0,
                    reason,
                    "rejected a live edit that fails document checks"
                );
                self.close(conn, CloseReason::Invalid);
                false
            }
        }
    }

    async fn recheck_edit_session(&mut self, conn: u64, authorization: &EditAuthorization) -> bool {
        // 새 위치 거부와 달리 검사 중 세션이 끝났거나 DB로 확인하지 못하면 대리 저장도 받지 않는다.
        if matches!(authorization, EditAuthorization::BeforeMove { .. }) {
            let Some(peer) = self
                .conns
                .get(&conn)
                .map(|connection| connection.peer.clone())
            else {
                return false;
            };
            if let Err(reason) = authorization.check_session(&self.db, &peer).await {
                self.close(conn, reason);
                return false;
            }
        }
        true
    }

    fn apply_update(&mut self, conn: u64, user: UserId, merged: Vec<u8>) {
        let Some(doc) = &self.doc else { return };
        // 다시 붙을 때 이미 반영된 Yjs 지움 기록이 와도 조용한 시간을 다시 세지 않는다.
        let before = doc.transact().state_vector();
        let applied = Update::decode_v1(&merged).ok().and_then(|u| {
            let mut txn = doc.transact_mut();
            txn.apply_update(u).ok()?;
            Some(!txn.delete_set().is_empty())
        });
        let Some(deleted) = applied else {
            self.close(conn, CloseReason::Invalid);
            return;
        };
        if !deleted && doc.transact().state_vector() == before {
            return;
        }
        self.file_refs.observe(doc, Some(user));
        self.dirty_state = true;
        self.dirty_since_snapshot = true;
        self.last_change = Instant::now();
        for list in [&mut self.editors, &mut self.edited_since] {
            list.retain(|u| *u != user);
            list.insert(0, user);
        }
        let message = Message::Sync(SyncMessage::Update(merged)).encode_v1();
        self.broadcast(&message, Some(conn));
    }
}

/// 방 상태에 변경을 넣은 임시 문서가 계보·모양·크기·본문 검사를 통과하는지(블로킹 작업에서 돈다).
fn check_update(
    state: &[u8],
    update: &[u8],
    lineage: &str,
    limits: &Limits,
    max_state: usize,
) -> Verdict {
    // 변경 혼자에 다른 계보 표시가 있으면 옛 상태로 다시 붙은 클라이언트다.
    let probe = Doc::new();
    let Ok(alone) = Update::decode_v1(update) else {
        return Verdict::Invalid("update does not decode".into());
    };
    if probe.transact_mut().apply_update(alone).is_err() {
        return Verdict::Invalid("update does not apply".into());
    }
    if lineage_of(&probe).is_some_and(|l| l != lineage) {
        return Verdict::Reload;
    }
    let scratch = Doc::new();
    let fragment = scratch.get_or_insert_xml_fragment(FRAGMENT);
    let applied = [state, update].iter().all(|bytes| {
        Update::decode_v1(bytes)
            .ok()
            .is_some_and(|u| scratch.transact_mut().apply_update(u).is_ok())
    });
    if !applied {
        return Verdict::Invalid("update does not apply to the room state".into());
    }
    let txn = scratch.transact();
    // 앞선 변경이 빠져 적용하지 못하고 남은 것이 있으면 이 클라이언트 상태는 서버와 어긋났다. 새로 열게 한다.
    if txn.has_missing_updates() {
        return Verdict::Reload;
    }
    if txn.encode_state_as_update_v1(&StateVector::default()).len() > max_state {
        return Verdict::Invalid("state too large".into());
    }
    if let Err(reason) = convert::check_shape(&txn) {
        return Verdict::Invalid(reason.into());
    }
    let body = convert::read_doc(&txn, &fragment);
    match documents::validate_body(&body, limits) {
        Ok(()) => Verdict::Ok,
        // 본문 검사의 설명은 우리가 쓴 고정 문구와 개수뿐이다.
        Err(AppError::BadRequest(reason)) => Verdict::Invalid(reason),
        Err(other) => Verdict::Invalid(other.code().into()),
    }
}

/// 연결이 처음 나타난 순서와 각 연결 안의 편집 순서를 보존한다.
fn ordered_edits(pending: Vec<Pending>) -> Vec<(u64, Vec<Pending>)> {
    let mut order = Vec::new();
    let mut by_conn: HashMap<u64, Vec<Pending>> = HashMap::new();
    for item in pending {
        if !order.contains(&item.conn) {
            order.push(item.conn);
        }
        by_conn.entry(item.conn).or_default().push(item);
    }
    order
        .into_iter()
        .filter_map(|conn| by_conn.remove(&conn).map(|edits| (conn, edits)))
        .collect()
}
