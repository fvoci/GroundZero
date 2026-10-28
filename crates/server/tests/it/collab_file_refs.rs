//! CR183-4: 첨부를 넣은 사람과 합친 리비전의 작성자를 구분한다.

use super::*;
use crate::sql_support::{World, attachment, fixture, rows};

async fn merged_attachment(inserted_by_reader: bool, forged: bool) {
    let w = World::new().await;
    let other = w.member("member").await;
    let source = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[source],
    )
    .await;
    let file = attachment(&w, source).await;
    let target = w.doc().await;
    let app = common::app(&w.state);
    let addr = serve(app).await;
    fixture(
        "UPDATE fvoci.workspaces SET realtime_enabled=true WHERE id=$1",
        &[w.ws],
    )
    .await;
    let (inserter, saver) = if inserted_by_reader {
        (&w.owner, &other)
    } else {
        (&other, &w.owner)
    };
    let id = target.to_string();
    let mut first = connect(addr, Some(inserter), &id, true).await.unwrap();
    let mut last = connect(addr, Some(saver), &id, true).await.unwrap();
    let url = format!("/api/files/{file}/content?version=1");
    first
        .edit(|txn, fragment| {
            let image = fragment.push_back(txn, XmlElementPrelim::empty("image"));
            image.insert_attribute(txn, "src", url);
            if forged {
                image.insert_attribute(txn, "inserted_by", w.owner.id.0.to_string());
            }
        })
        .await;
    assert!(last.until(|c| c.text().contains(&file.to_string())).await);
    last.paragraph("다른 편집자의 마지막 변경").await;
    assert!(
        first
            .until(|c| c.text().contains("다른 편집자의 마지막 변경"))
            .await
    );
    w.state.collab.persist_all().await;
    let mut observer = common::fixture_conn().await;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_collab_file_refs WHERE document_id=$1 AND file_id=$2",
    )
    .bind(target)
    .bind(file)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    assert!(w.state.collab.close_workspace(w.ws, w.owner.id).await);
    let revision = rows(&w.state, &w.owner,
        "SELECT author FROM fvoci.document_revisions WHERE document_id=$1 ORDER BY revision DESC LIMIT 1", &[target]).await;
    assert_eq!(revision[0]["author"], saver.id.0.to_string());
    let saved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_file_refs WHERE document_id=$1 AND file_id=$2",
    )
    .bind(target)
    .bind(file)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    let retained: bool = sqlx::query_scalar("SELECT fvoci.app_version_referenced($1,1,(SELECT id FROM fvoci.file_versions WHERE file_id=$1 AND version=1))")
        .bind(file).fetch_one(&mut observer).await.unwrap();
    first.close().await;
    last.close().await;
    assert_eq!(
        (pending, saved, retained),
        if inserted_by_reader {
            (1, 1, true)
        } else {
            (0, 0, false)
        },
        "pending/revision retention must use the inserting editor's placement grade"
    );
}

#[tokio::test]
async fn reader_inserts_attachment_and_unreadable_author_preserves_it() {
    merged_attachment(true, false).await;
}

#[tokio::test]
async fn unreadable_editor_inserts_attachment_and_readable_author_cannot_lend_access() {
    merged_attachment(false, false).await;
}

#[tokio::test]
async fn unreadable_editor_cannot_forge_the_inserting_users_identity() {
    merged_attachment(false, true).await;
}
