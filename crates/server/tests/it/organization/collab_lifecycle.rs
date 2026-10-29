//! #235: 권한 제거의 보류 편집과 삭제·복원 전후의 방 수명(D-27·D-70·D-95·D-99).

mod contracts;
mod live;
mod privacy;
mod races;
mod timeouts;

use crate::common::{self, TestUser, send};
use axum::Router;
use axum::http::{Method, StatusCode};
use fvoci_core::ids::DocumentId;
use fvoci_server::http::AppState;
use live::Client;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio::task::JoinHandle;

const PENDING: &str = "제거 전에 서버가 확인한 미리비전 편집";
const AFTER_RESTORE: &str = "재연결 없이 삭제 전 방으로 보낸 편집";

struct Fixture {
    state: AppState,
    app: Router,
    addr: SocketAddr,
    server: JoinHandle<()>,
    owner: TestUser,
    writer: TestUser,
    workspace: String,
    document: String,
}

impl Fixture {
    async fn new() -> Self {
        // 주기 재확인이 명령 자체의 방 정리 결함을 가리지 않게 한다.
        Self::with_recheck("3600").await
    }

    async fn with_recheck(recheck: &str) -> Self {
        let state = fvoci_server::app::build_state(common::config_with(&[(
            "FVOCI_COLLAB_RECHECK_SECS",
            recheck,
        )]))
        .unwrap();
        let app = common::app(&state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let served = app.clone();
        let server = tokio::spawn(async move { axum::serve(listener, served).await.unwrap() });
        let owner = common::login(&state, "lifecycle owner").await;
        let workspace = common::create_workspace(&app, &owner, "방 수명 재현").await;
        let (writer, _) =
            common::add_member(&app, &state, &owner, &workspace, "sole-writer", "member").await;
        let doc = common::create_doc(&app, &owner, &workspace, "방 수명").await;
        let document = doc["id"].as_str().unwrap().to_owned();
        Self {
            state,
            app,
            addr,
            server,
            owner,
            writer,
            workspace,
            document,
        }
    }

    async fn current(&self) -> Value {
        let reply = send(
            &self.app,
            Method::GET,
            &format!("/api/documents/{}", self.document),
            Some(&self.owner),
            None,
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
        reply.json
    }

    async fn pending(&self) -> bool {
        let mut conn = common::fixture_conn().await;
        sqlx::query_scalar(
            "SELECT pending FROM fvoci.document_collab_states WHERE document_id = $1::uuid",
        )
        .bind(&self.document)
        .fetch_one(&mut conn)
        .await
        .unwrap()
    }

    async fn history(&self) -> Vec<Value> {
        let uri = format!("/api/documents/{}/revisions", self.document);
        let list = send(&self.app, Method::GET, &uri, Some(&self.owner), None).await;
        assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
        let mut history = Vec::new();
        for summary in list.json.as_array().unwrap() {
            let revision = summary["revision"].as_i64().unwrap();
            let reply = send(
                &self.app,
                Method::GET,
                &format!("{uri}/{revision}"),
                Some(&self.owner),
                None,
            )
            .await;
            assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
            history.push(reply.json);
        }
        history
    }

    async fn stop(self) {
        self.state.collab.shutdown().await;
        self.server.abort();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[derive(Debug)]
enum Removal {
    Workspace,
    Organization,
}

async fn sole_editor_removal(removal: Removal) {
    let f = Fixture::new().await;
    // 관리자 연결을 열지 않는다. 연결된 다른 작성자가 체크포인트를 대신 남기면 재현 조건이 사라진다.
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(
        writer.edit_and_wait(PENDING).await,
        "pending edit was rejected"
    );
    assert!(f.pending().await);
    assert_eq!(f.current().await["revision"], 1);
    let uri = match removal {
        Removal::Workspace => format!("/api/workspaces/{}/members/{}", f.workspace, f.writer.id.0),
        Removal::Organization => format!(
            "/api/organizations/{}/members/{}",
            common::organization_of_workspace(&f.workspace).await,
            f.writer.id.0
        ),
    };
    let removed = send(&f.app, Method::DELETE, &uri, Some(&f.owner), None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    let access = send(
        &f.app,
        Method::GET,
        &format!("/api/documents/{}", f.document),
        Some(&f.writer),
        None,
    )
    .await;
    assert!(
        access.status == StatusCode::NOT_FOUND || access.json["can_edit"] == false,
        "removed editor retained write access: {:?}",
        access.json
    );
    // 보통 판 충돌이면 최신 판을 다시 확인해 진행한다. 매번 새 request_id로 재송신 캐시도 배제한다.
    let mut attempts = Vec::new();
    for _ in 0..3 {
        let current = f.current().await;
        let reply = send(
            &f.app,
            Method::POST,
            &format!("/api/documents/{}/revisions/1/restore", f.document),
            Some(&f.owner),
            Some(json!({
                "request_id": common::new_request_id(), "base_revision": current["revision"]
            })),
        )
        .await;
        let restored = reply.status == StatusCode::OK;
        attempts.push((reply.status, reply.json));
        if restored {
            break;
        }
    }
    let current = f.current().await;
    let pending = f.pending().await;
    let history = f.history().await;
    writer.close().await;
    f.stop().await;
    assert!(
        attempts.iter().any(|(status, _)| *status == StatusCode::OK),
        "{removal:?}: restore never progressed; attempts={attempts:?}, revision={}, pending={pending}, history={history:?}",
        current["revision"]
    );
    assert!(
        history
            .iter()
            .any(|r| r["body"] == common::doc_body(PENDING)),
        "{removal:?}: removed editor's acknowledged edit was lost: {history:?}"
    );
}

#[tokio::test]
async fn workspace_removal_preserves_sole_editors_pending_edit_and_allows_restore() {
    sole_editor_removal(Removal::Workspace).await;
}

#[tokio::test]
async fn organization_removal_preserves_sole_editors_pending_edit_and_allows_restore() {
    sole_editor_removal(Removal::Organization).await;
}

#[tokio::test]
async fn workspace_delete_then_restore_requires_a_new_live_connection() {
    let f = Fixture::new().await;
    let mut old = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(old.edit_and_wait(PENDING).await);
    assert!(f.pending().await);
    let deleted = send(
        &f.app,
        Method::DELETE,
        &format!("/api/workspaces/{}", f.workspace),
        Some(&f.owner),
        Some(json!({ "name": "방 수명 재현" })),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);
    // 재확인 타이머나 재연결을 기다리지 않고 바로 복원한다.
    let restored = send(
        &f.app,
        Method::POST,
        &format!("/api/me/deleted-workspaces/{}/restore", f.workspace),
        Some(&f.owner),
        None,
    )
    .await;
    assert_eq!(
        restored.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        restored.json
    );
    let accepted = old.edit_and_wait(AFTER_RESTORE).await;
    let room_remained = f
        .state
        .collab
        .has_room(DocumentId(f.document.parse().unwrap()));
    let mut fresh = Client::connect(f.addr, &f.document, &f.owner).await;
    let observed = fresh.body();
    old.close().await;
    fresh.close().await;
    f.stop().await;
    assert!(
        !accepted && observed != common::doc_body(AFTER_RESTORE),
        "pre-delete connection still edits after restore: acknowledged={accepted}, room_remained={room_remained}, fresh_body={observed}"
    );
}
