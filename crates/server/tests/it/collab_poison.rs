//! #172: 목록 오염 뒤 저장된 상태와 클라이언트의 미반영 변경을 합친다.

use super::*;
use crate::sql_support::{World, fixture};
use fvoci_server::collab::room::recovery::{Registry, poison};

async fn reopen_after_poison(registry: Registry) {
    let w = World::new().await;
    let target = w.doc().await;
    fixture(
        "UPDATE fvoci.workspaces SET realtime_enabled=true WHERE id=$1",
        &[w.ws],
    )
    .await;
    let addr = serve(common::app(&w.state)).await;
    let id = target.to_string();
    let mut client = connect(addr, Some(&w.owner), &id, true).await.unwrap();
    client.paragraph("서버에 저장한 편집").await;
    assert!(client.until(acked).await);
    // 서버에 보내지 않은 로컬 편집도 재연결 때 원래 CRDT와 합쳐야 한다.
    let fragment = client.doc.get_or_insert_xml_fragment(FRAGMENT);
    {
        let mut txn = client.doc.transact_mut();
        let p = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(&mut txn, XmlTextPrelim::new("확인받지 못한 로컬 편집"));
    }
    poison(&w.state.collab, registry);
    // 세 보조 목록도 방 목록과 같은 잠금 구간에서 읽도록 실제 join 경로를 통과한다.
    let mut probe = connect(addr, Some(&w.owner), &id, true).await.unwrap();
    assert_eq!(
        client.closed().await,
        Some(4503),
        "poisoned {registry:?} must disconnect the old room"
    );
    assert!(
        probe
            .until(|c| c.text().contains("서버에 저장한 편집"))
            .await
    );
    let local = client.doc;
    let rejoined = connect_with(addr, Some(&w.owner), &id, true, local)
        .await
        .unwrap();
    assert!(
        probe
            .until(|c| c.text().contains("확인받지 못한 로컬 편집"))
            .await
    );
    w.state.collab.persist_all().await;
    assert!(w.state.collab.close_workspace(w.ws, w.owner.id).await);
    let mut reopened = connect(addr, Some(&w.owner), &id, true).await.unwrap();
    assert!(
        reopened
            .until(|c| c.text().contains("확인받지 못한 로컬 편집"))
            .await
    );
    assert!(reopened.text().contains("서버에 저장한 편집"));
    probe.close().await;
    rejoined.close().await;
    reopened.close().await;
}

#[tokio::test]
async fn poisoned_room_registry_reopens_and_merges_unacknowledged_edits() {
    reopen_after_poison(Registry::Rooms).await;
}

#[tokio::test]
async fn poisoned_closing_registry_reopens_and_merges_unacknowledged_edits() {
    reopen_after_poison(Registry::Closing).await;
}

#[tokio::test]
async fn poisoned_held_registry_reopens_and_merges_unacknowledged_edits() {
    reopen_after_poison(Registry::Held).await;
}
