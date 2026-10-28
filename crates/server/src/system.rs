//! 시스템 도메인이 받는 구현 경계. 저장소와 공동 편집은 조립 crate에 남는다.

use crate::{collab::Rooms, http::AppState, storage::Storage};
use axum::extract::FromRef;
use fvoci_core::{error::AppError, ids::DocumentId};
use fvoci_system::{
    SystemState,
    state::{ByteStream, IoFuture, ObjectStore, SnapshotDocuments},
};
use std::{collections::HashSet, sync::Arc};

impl FromRef<AppState> for SystemState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            storage: Arc::new(state.storage.clone()),
            snapshots: Arc::new(state.collab.clone()),
            plain_text: crate::features::search_text::plain_text,
            export_permits: Arc::clone(&state.export_permits),
        }
    }
}

impl ObjectStore for Storage {
    fn get<'a>(&'a self, key: &'a str) -> IoFuture<'a, Result<ByteStream, AppError>> {
        Box::pin(self.get(key))
    }
    fn delete<'a>(&'a self, key: &'a str) -> IoFuture<'a, Result<(), AppError>> {
        Box::pin(self.delete(key))
    }
    fn abort<'a>(&'a self, key: &'a str, upload: &'a str) -> IoFuture<'a, Result<(), AppError>> {
        Box::pin(self.abort(key, upload))
    }
}

impl SnapshotDocuments for Rooms {
    fn snapshot<'a>(&'a self, docs: &'a HashSet<DocumentId>) -> IoFuture<'a, ()> {
        Box::pin(self.snapshot_documents(docs))
    }
}
