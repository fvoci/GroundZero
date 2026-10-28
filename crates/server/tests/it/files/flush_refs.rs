//! 버전 삭제의 주입 경계에서도 CR183-4 편집자 귀속을 보존한다.
use super::{Client, connect, serve};
use crate::common::{self, TestUser};
use crate::sql_support::{World, attachment, fixture, made};
use axum::Router;
use axum::extract::FromRef;
use axum::http::{Method, StatusCode};
use fvoci_files::{FilesState, PersistWorkspace};
use fvoci_http::session::SessionState;
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;
use yrs::types::xml::{Xml, XmlElementPrelim, XmlFragment};

#[derive(Debug)]
struct Flush {
    rooms: fvoci_server::collab::Rooms,
    calls: AtomicUsize,
}
impl PersistWorkspace for Flush {
    fn persist_workspace(&self, workspace: Uuid) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.rooms.persist_workspace(workspace).await;
        })
    }
}
#[derive(Debug, Clone)]
struct State {
    files: FilesState,
    session: SessionState,
}
impl FromRef<State> for FilesState {
    fn from_ref(state: &State) -> Self {
        state.files.clone()
    }
}
impl FromRef<State> for SessionState {
    fn from_ref(state: &State) -> Self {
        state.session.clone()
    }
}

#[derive(Debug, Clone, Copy)]
enum Inserter {
    Visible,
    Hidden,
}

async fn deletion(insertion: Inserter) {
    let w = World::new().await;
    let other = w.member("member").await;
    let source = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[source],
    )
    .await;
    let file = attachment(&w, source).await;
    let upload = made(&w.state, &w.owner,
        "SELECT fvoci.app_create_upload($1,$2,NULL,NULL,'','','text/plain',10,10,3600,1000,100000,@ids) AS id", &[Uuid::now_v7(), file]).await;
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[upload],
    )
    .await;
    made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_finish_upload($1,repeat('b',64),100000,@ids) AS id",
        &[upload],
    )
    .await;
    let target = w.doc().await;
    fixture(
        "UPDATE fvoci.workspaces SET realtime_enabled=true WHERE id=$1",
        &[w.ws],
    )
    .await;
    let app = common::app(&w.state);
    let addr = serve(app).await;
    let (inserter, saver): (&TestUser, &TestUser) = match insertion {
        Inserter::Visible => (&w.owner, &other),
        Inserter::Hidden => (&other, &w.owner),
    };
    let id = target.to_string();
    let mut first = connect(addr, Some(inserter), &id, true).await.unwrap();
    let mut last = connect(addr, Some(saver), &id, true).await.unwrap();
    let url = format!("/api/files/{file}/content?version=1");
    first
        .edit(|txn, fragment| {
            fragment
                .push_back(txn, XmlElementPrelim::empty("image"))
                .insert_attribute(txn, "src", url);
        })
        .await;
    assert!(
        last.until(|c: &Client| c.text().contains(&file.to_string()))
            .await
    );
    last.paragraph("다른 편집자가 마지막으로 입력").await;
    assert!(
        first
            .until(|c| c.text().contains("다른 편집자가 마지막으로 입력"))
            .await
    );

    let flush = Arc::new(Flush {
        rooms: w.state.collab.clone(),
        calls: AtomicUsize::new(0),
    });
    let mut files = FilesState::from_ref(&w.state);
    files.collab = flush.clone();
    let state = State {
        files,
        session: SessionState::from_ref(&w.state),
    };
    let router = Router::new()
        .nest("/api", fvoci_files::router(&state))
        .with_state(state);
    let hidden = common::send(
        &router,
        Method::DELETE,
        &format!("/api/files/{file}/versions/1"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{}", hidden.json);
    assert_eq!(
        flush.calls.load(Ordering::SeqCst),
        0,
        "an invisible file must not flush any room"
    );
    let removed = common::send(
        &router,
        Method::DELETE,
        &format!("/api/files/{file}/versions/1"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(flush.calls.load(Ordering::SeqCst), 1);
    let expected = match insertion {
        Inserter::Visible => StatusCode::CONFLICT,
        Inserter::Hidden => StatusCode::NO_CONTENT,
    };
    assert_eq!(removed.status, expected, "{}", removed.json);
    let mut observer = common::fixture_conn().await;
    let pending: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT editor_id FROM fvoci.document_collab_file_refs WHERE document_id=$1 AND file_id=$2")
        .bind(target).bind(file).fetch_all(&mut observer).await.unwrap();
    let expected_editors = match insertion {
        Inserter::Visible => vec![inserter.id.0],
        Inserter::Hidden => vec![],
    };
    assert_eq!(pending, expected_editors);
    assert!(w.state.collab.close_workspace(w.ws, w.owner.id).await);
    let saved: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT editor_id FROM fvoci.document_file_refs WHERE document_id=$1 AND file_id=$2")
        .bind(target).bind(file).fetch_all(&mut observer).await.unwrap();
    assert_eq!(saved, expected_editors);
    let author: Uuid = sqlx::query_scalar("SELECT author FROM fvoci.document_revisions WHERE document_id=$1 ORDER BY revision DESC LIMIT 1")
        .bind(target).fetch_one(&mut observer).await.unwrap();
    assert_eq!(
        author, saver.id.0,
        "the revision author must differ from the inserting editor"
    );
    first.close().await;
    last.close().await;
}

#[tokio::test]
async fn deletion_preserves_a_readable_inserters_version_with_an_unreadable_last_editor() {
    deletion(Inserter::Visible).await;
}
#[tokio::test]
async fn deletion_does_not_borrow_the_last_editors_access_for_a_hidden_inserter() {
    deletion(Inserter::Hidden).await;
}
