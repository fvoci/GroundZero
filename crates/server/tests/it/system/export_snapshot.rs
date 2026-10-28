#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration setup and assertions fail on invalid test data"
)]
//! 공간 반출의 스냅샷 뒤 재인가(D-113, 시스템 브리프·PR4a 잠금 경계).
use crate::common::{self, create_doc, create_workspace, login, send_raw};
use axum::{
    Router,
    extract::FromRef,
    http::{Method, StatusCode},
};
use fvoci_core::ids::DocumentId;
use fvoci_http::session::SessionState;
use fvoci_system::{
    SystemState,
    state::{IoFuture, SnapshotDocuments},
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ExportState {
    system: SystemState,
    session: SessionState,
}
impl FromRef<ExportState> for SystemState {
    fn from_ref(state: &ExportState) -> Self {
        state.system.clone()
    }
}
impl FromRef<ExportState> for SessionState {
    fn from_ref(state: &ExportState) -> Self {
        state.session.clone()
    }
}

#[derive(Debug)]
struct DeleteDuringSnapshot {
    workspace: Uuid,
    document: DocumentId,
    calls: AtomicUsize,
}
impl SnapshotDocuments for DeleteDuringSnapshot {
    fn snapshot<'a>(&'a self, docs: &'a HashSet<DocumentId>) -> IoFuture<'a, ()> {
        Box::pin(async move {
            assert!(docs.contains(&self.document));
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut conn = common::fixture_conn().await;
            sqlx::query("UPDATE fvoci.workspaces SET deleted_at = now() WHERE id = $1")
                .bind(self.workspace)
                .execute(&mut conn)
                .await
                .unwrap();
        })
    }
}

#[tokio::test]
async fn workspace_export_reauthorizes_after_the_injected_snapshot_before_reading_or_auditing() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let outsider = login(&state, "outsider").await;
    let ws = create_workspace(&app, &owner, "export reauthorization").await;
    let doc = create_doc(&app, &owner, &ws, "private export").await;
    let hook = Arc::new(DeleteDuringSnapshot {
        workspace: Uuid::parse_str(&ws).unwrap(),
        document: DocumentId(Uuid::parse_str(doc["id"].as_str().unwrap()).unwrap()),
        calls: AtomicUsize::new(0),
    });
    let mut system = SystemState::from_ref(&state);
    let snapshots = Arc::clone(&hook);
    system.snapshots = snapshots;
    let test_state = ExportState {
        system,
        session: SessionState::from_ref(&state),
    };
    let app = Router::new()
        .nest("/api", fvoci_system::router(&test_state))
        .with_state(test_state);
    let uri = format!("/api/workspaces/{ws}/export");
    assert_eq!(
        send_raw(&app, Method::GET, &uri, Some(&outsider), &[], vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(hook.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        send_raw(&app, Method::GET, &uri, Some(&owner), &[], vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        state.export_permits.available_permits(),
        state.config.files.export_concurrency
    );
    let mut conn = common::fixture_conn().await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.audit_log WHERE workspace_id = $1 AND action = 'export'",
    )
    .bind(hook.workspace)
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
