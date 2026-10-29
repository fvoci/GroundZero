//! #233: 실제 실시간 체크포인트가 조직 멤버 제거 뒤에 줄 서고 철회된 쓰기를 저장하지 않는다.

use super::*;
use crate::sql_support::*;

#[tokio::test]
async fn live_personal_note_checkpoint_waits_for_organization_removal() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let app = common::app(&w.state);
    let addr = serve(app.clone()).await;
    let mut client = connect(addr, Some(&editor), &note.to_string(), true)
        .await
        .unwrap();
    client.paragraph("revoked checkpoint").await;
    assert!(client.until(acked).await);
    let mut removal = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        removal.conn(),
        "SELECT fvoci.app_remove_organization_member_ids($1,$2)",
        &[w.org, editor.id.0],
    )
    .await
    .unwrap();
    let holder = pid(removal.conn()).await;
    // 마지막 연결 종료가 실제 write_snapshot → lock_for_save를 실행한다.
    client.close().await;
    until_waiting_on_lock("app_lock_document_workspace", holder).await;
    on(
        removal.conn(),
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[w.org, editor.id.0],
    )
    .await
    .unwrap();
    removal.commit().await.unwrap();
    until_no_room(&w.state, &note.to_string()).await;
    let current = get_doc(&app, &w.owner, &note.to_string()).await;
    assert_eq!(current["revision"], 1);
    assert_eq!(current["body"], json!({"type":"doc","content":[]}));
    let reopened = connect(addr, Some(&editor), &note.to_string(), true).await;
    assert!(matches!(reopened, Err(StatusCode::NOT_FOUND)));
}
