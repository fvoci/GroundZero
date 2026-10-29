//! 저장 상태 불러오기, 확인과 리비전 체크포인트.

use super::{
    AppError, Attribution, CHECKPOINT_LIMIT, Change, CloseReason, Decode, Doc, Encode, FRAGMENT,
    LoadRow, MSG_PERSISTED, Map, Message, Outbound, ReadTxn, Room, SnapshotFailure, StateVector,
    Transact, Update, UserId, Uuid, Value, convert, documents, new_document,
};

impl Room {
    /// 방이 아직 문서를 불러오지 않았으면 불러온다(옮기기·복원하는 사람의 권한으로). 방을 연 연결의 참여 요청보다 이동·복원
    /// 명령이 먼저 닿으면(방 목록에 넣은 뒤 참여 요청을 보내기 전에 `Rooms::hold`가 그 방을 찾음) 저장 대기 상태를 이어받지
    /// 않은 채 복원·이동해 그 편집을 잃을 수 있다(#64 반박 검증).
    pub(super) async fn ensure_loaded(&mut self, user: UserId) -> Result<(), AppError> {
        if self.doc.is_some() {
            return Ok(());
        }
        self.doc = Some(self.load(user).await?);
        Ok(())
    }

    /// 들어오는 사람의 권한으로 저장 상태·편집자·귀속을 이어받거나 본문에서 방을 만든다(D-95).
    pub(super) async fn load(&mut self, user: UserId) -> Result<Doc, AppError> {
        let mut tx = self.db.begin_as(user).await?;
        let (revision, _title, body, state, editors, file_refs): LoadRow =
            sqlx::query_as("SELECT * FROM app_collab_load($1)")
                .bind(self.doc_id)
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        tx.commit().await?;
        let doc = new_document();
        let restored = state
            .and_then(|bytes| Update::decode_v1(&bytes).ok())
            .is_some_and(|update| doc.transact_mut().apply_update(update).is_ok());
        if !restored || lineage_of(&doc).is_none() {
            let doc = new_document();
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let map = meta(&doc);
            let mut txn = doc.transact_mut();
            convert::write_doc(&mut txn, &fragment, &body);
            map.insert(&mut txn, LINEAGE, Uuid::now_v7().to_string());
            drop(txn);
            self.dirty_state = true;
            self.lineage = lineage_of(&doc).unwrap_or_default();
            self.base_revision = revision;
            self.base_body = body;
            self.file_refs = Attribution::load(&doc, None)?;
            return Ok(doc);
        }
        self.lineage = lineage_of(&doc).unwrap_or_default();
        self.base_revision = revision;
        self.base_body = body;
        self.file_refs = Attribution::load(&doc, file_refs)?;
        let editors: Vec<UserId> = editors
            .unwrap_or_default()
            .into_iter()
            .map(UserId)
            .collect();
        self.editors.clone_from(&editors);
        self.edited_since = editors;
        // 저장된 상태에서 불러왔으니 이 상태는 이미 DB에 있다.
        self.persisted = Some(doc.transact().snapshot().encode_v1());
        // 이어받은 미리비전 편집도 다음 체크포인트에 남긴다. Yjs 형태가 본문과 같으면 남길 변경이 없다(#64).
        let live = {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            convert::read_doc(&doc.transact(), &fragment)
        };
        self.dirty_since_snapshot = live != live_form(&self.base_body);
        Ok(doc)
    }
    /// 편집 중 Yjs 상태 저장(서버 작업 문맥).
    pub(super) async fn persist(&mut self) {
        if !self.dirty_state {
            return;
        }
        let Some(doc) = &self.doc else {
            return;
        };
        // 저장하는 상태와 그 스냅숏(저장 확인에 담는다)을 같은 순간에서 만든다.
        let (state, snapshot) = {
            let txn = doc.transact();
            (
                txn.encode_state_as_update_v1(&StateVector::default()),
                txn.snapshot().encode_v1(),
            )
        };
        // 리비전이 되지 않은 본문의 첨부도 저장 확인 전에 붙잡는다(D-56).
        let body = self.dirty_since_snapshot.then(|| {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            convert::read_doc(&doc.transact(), &fragment)
        });
        let editors: Vec<Uuid> = self.edited_since.iter().map(|u| u.0).collect();
        let result = async {
            let mut tx = self.db.begin_anonymous().await?;
            // 실시간을 끄거나 옮기기 전까지 편집자와 첨부 귀속을 함께 이어받는다.
            let stored: bool =
                sqlx::query_scalar("SELECT app_system_collab_store($1, $2, $3, $4, $5, $6, $7)")
                    .bind(self.doc_id)
                    .bind(self.base_revision)
                    .bind(state)
                    .bind(self.dirty_since_snapshot)
                    .bind(body)
                    .bind(editors)
                    .bind(self.file_refs.json()?)
                    .fetch_one(tx.conn())
                    .await?;
            tx.commit().await?;
            Ok::<bool, AppError>(stored)
        }
        .await;
        match result {
            Ok(stored) => {
                self.dirty_state = false;
                // DB에 들어간 것만 확인으로 알린다(낡은 상태라 거절됐으면 알리지 않는다).
                if stored {
                    let message = Message::Custom(MSG_PERSISTED, snapshot.clone()).encode_v1();
                    self.persisted = Some(snapshot);
                    self.broadcast(&message, None);
                }
            }
            Err(err) => {
                tracing::warn!(event = "collab.store_failed", document = %self.doc_id.0, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "could not store live editing state");
            }
        }
    }

    /// 방 문서를 저장 API와 같은 길로 리비전에 남긴다. 작성자는 마지막으로 고친 사람, 그 사람이 쓸 수 없으면
    /// 최근에 고친 다른 사람, 그다음은 연결된 작성자 순. 작성자가 마지막 리비전 뒤로 고친 사람이 아니면(대신 저장) 고친
    /// 사람을 리비전에 따로 적는다(D-95, `write_snapshot`).
    pub(super) async fn snapshot(&mut self) -> bool {
        self.snapshot_with(None).await
    }

    /// `fallback`: 편집한 사람·연결된 작성자 모두 남길 수 없을 때의 작성자(실시간을 끄는 관리자, 옮기는 사람).
    pub(super) async fn snapshot_with(&mut self, fallback: Option<UserId>) -> bool {
        if !self.dirty_since_snapshot {
            return true;
        }
        let Some(doc) = &self.doc else {
            return true;
        };
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let body = convert::read_doc(&doc.transact(), &fragment);
        if documents::validate_body(&body, &self.limits).is_err() {
            self.snapshot_failure = Some(SnapshotFailure::InvalidBody);
            tracing::warn!(event = "collab.invalid_document", document = %self.doc_id.0, "live document failed checks; not saved as a revision");
            return false;
        }
        for author in self.snapshot_authors(fallback) {
            match self.write_snapshot(author, &body).await {
                Ok(Some(revision)) => {
                    self.base_revision = revision;
                    self.base_body = body;
                    self.dirty_since_snapshot = false;
                    self.edited_since.clear();
                    self.dirty_state = true;
                    // 리비전 번호와 상태가 어긋난 채로 멈추지 않도록 바로 저장한다.
                    self.persist().await;
                    return true;
                }
                Ok(None) => {
                    self.reopen_changed_document();
                    return true;
                }
                Err(AppError::Forbidden | AppError::NotFound) => {}
                Err(err) => {
                    tracing::warn!(event = "collab.save_failed", document = %self.doc_id.0, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "could not save the live document as a revision");
                    return false;
                }
            }
        }
        tracing::warn!(event = "collab.no_editor", document = %self.doc_id.0, "no editor can save the live document; keeping the live state");
        false
    }

    /// 최근 편집자 → 연결된 작성자 → 허용된 대리 저장자 순서를 유지한다(D-95).
    fn snapshot_authors(&self, fallback: Option<UserId>) -> Vec<UserId> {
        let mut authors = self.editors.clone();
        for c in self
            .conns
            .values()
            .filter(|c| c.peer.access.permission() == Some(true))
        {
            if !authors.contains(&c.peer.user) {
                authors.push(c.peer.user);
            }
        }
        if let Some(admin) = fallback.filter(|a| !authors.contains(a)) {
            authors.push(admin);
        }
        authors
    }

    fn reopen_changed_document(&mut self) {
        // 본문이 다른 길로 바뀌었다(복원). 덮어쓰지 않고 다시 열게 한다.
        self.snapshot_failure = Some(SnapshotFailure::ChangedOutside);
        tracing::warn!(event = "collab.changed_outside", document = %self.doc_id.0, "document changed outside live editing; reopening");
        for (_, c) in self.conns.drain() {
            let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
        }
        self.dirty_since_snapshot = false;
        self.edited_since.clear();
        self.dirty_state = false;
    }

    /// 옮기거나 복원하기 전 체크포인트: 리비전으로 남기지 않은 편집을 남긴다. 작성자는 편집한 사람, 그다음 연결된 작성자
    /// 순이다(`snapshot_with`, 연결된 작성자에는 연결돼 있는 복원하는 사람도 든다. 대신 저장이면 실제 편집자를 따로 적는다,
    /// D-95). 옮길 때는 그 모두가 남길 수 없으면 옮기는 사람(`user`)이 대신 작성자가 된다. 복원하는 사람은 그 자리에 두지
    /// 않는다(방 없는 복원은 연결이 없어 저장 대기 상태의 편집자만 남길 수 있다). 남겼으면 `None`, 남겼는지 확인되지 않으면
    /// 하지 않는 이유(409). 다시 해도 남길 수 없는 원인(본문 검사 실패)에는 "다시 하라"고 하지 않는다.
    pub(super) async fn checkpoint(
        &mut self,
        change: Change,
        user: UserId,
    ) -> Option<&'static str> {
        self.snapshot_failure = None;
        let moving = matches!(change, Change::Move);
        let stand_in = matches!(change, Change::Move | Change::Organization).then_some(user);
        let saved = tokio::time::timeout(CHECKPOINT_LIMIT, self.snapshot_with(stand_in)).await;
        match saved {
            Ok(true) if self.snapshot_failure != Some(SnapshotFailure::ChangedOutside) => None,
            Ok(true) if moving => Some(
                "the document changed outside live editing before its live edits were saved; nothing was moved",
            ),
            Ok(true) => Some(
                "the document changed outside live editing before its live edits were saved; nothing was restored",
            ),
            Ok(false) if self.snapshot_failure == Some(SnapshotFailure::InvalidBody) && moving => {
                Some(
                    "live edits do not pass the document checks, so they cannot be saved as a revision; nothing was moved",
                )
            }
            Ok(false) if self.snapshot_failure == Some(SnapshotFailure::InvalidBody) => Some(
                "live edits do not pass the document checks, so they cannot be saved as a revision; nothing was restored",
            ),
            Ok(false) if moving => Some(
                "live edits could not be saved as a revision first; nothing was moved, try again",
            ),
            Ok(false) => {
                Some("live edits could not be saved as a revision first; nothing was restored")
            }
            Err(_) if moving => {
                Some("saving live edits as a revision took too long; nothing was moved, try again")
            }
            Err(_) => Some(
                "saving live edits as a revision took too long; nothing was restored, try again",
            ),
        }
    }

    /// 새 리비전 번호, 또는 본문이 다른 길로 바뀌었으면 `None`. 작성자(`author`, 이 트랜잭션을 연 사람)가 마지막 리비전
    /// 뒤로 고친 사람에 들지 않으면 그 사람들을 리비전의 실제 편집자로 적는다(D-95: 옮긴 사람·관리자·연결된 다른 작성자가
    /// 대신 저장해도 내용을 쓴 것처럼 보이지 않게). 고친 사람을 모르면(이 기록 전에 저장된 대기 상태) 빈 목록이다.
    pub(super) async fn write_snapshot(
        &self,
        author: UserId,
        body: &Value,
    ) -> Result<Option<i32>, AppError> {
        let edited_by = (!self.edited_since.contains(&author)).then_some(&self.edited_since[..]);
        let mut tx = self.db.begin_as(author).await?;
        let (_, current) = documents::lock_for_save(&mut tx, self.doc_id).await?;
        if current.body != self.base_body {
            tx.commit().await?;
            return Ok(None);
        }
        if current.body == *body {
            tx.commit().await?;
            return Ok(Some(current.revision));
        }
        let title = current.title.clone();
        let row = documents::write_revision(
            &mut tx,
            self.doc_id,
            &title,
            body,
            Uuid::now_v7(),
            edited_by,
            Some(&self.file_refs.json()?),
        )
        .await?;
        tx.commit().await?;
        Ok(Some(row.revision))
    }
}

/// 계보 표시를 두는 Yjs 맵. 편집기(조각 `default`)는 이 맵을 보지 않는다.
const META: &str = "fvoci";
pub(super) const LINEAGE: &str = "lineage";

pub(super) fn meta(doc: &Doc) -> yrs::MapRef {
    doc.get_or_insert_map(META)
}

/// 본문을 Yjs 문서에 썼다가 다시 읽은 모양(null 속성이 빠지고 표시가 이름순이다). 저장된 상태가 본문과 같은 내용인지 볼 때
/// 쓴다.
pub(super) fn live_form(body: &Value) -> Value {
    let doc = Doc::new();
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    convert::write_doc(&mut doc.transact_mut(), &fragment, body);
    convert::read_doc(&doc.transact(), &fragment)
}

pub(super) fn lineage_of(doc: &Doc) -> Option<String> {
    let map = meta(doc);
    match map.get(&doc.transact(), LINEAGE) {
        Some(yrs::Out::Any(yrs::Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}
