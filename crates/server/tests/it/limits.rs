#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
use crate::sql_support;

use sql_support::*;
use uuid::Uuid;

const UPLOAD: &str = "SELECT fvoci.app_create_upload($1,NULL,'document',$2,'file.txt','','text/plain',20,20,3600,1000,10000,@ids) AS id";

#[tokio::test]
async fn file_size_and_open_upload_limits_use_the_smallest_scope() {
    let w = World::new().await;
    let doc = w.doc().await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_workspace_limits($1,'{\"max_file_bytes\":10}',@ids)",
        &[w.ws],
    )
    .await;
    denied(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc], "22023").await;
    rows(&w.state,&w.owner,"SELECT fvoci.app_set_workspace_limits($1,'{\"max_file_bytes\":100,\"max_concurrent_uploads\":1}',@ids)",&[w.ws]).await;
    let open = made(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc]).await;
    denied(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc], "55000").await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_abort_upload($1)",
        &[open],
    )
    .await;
    made(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc]).await;
    // 개인 노트는 같은 소유자의 조직·공간 한도가 아닌 서버 기본값만 받는다.
    let note = w.note().await;
    rows(&w.state,&w.owner,"SELECT fvoci.app_set_organization_settings($1,'{\"max_file_bytes\":1,\"max_storage_bytes\":0}',@ids)",&[w.org]).await;
    let personal = made(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), note]).await;
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[personal],
    )
    .await;
    made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_finish_upload($1,repeat('a',64),10000,@ids) AS id",
        &[personal],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        &UPLOAD.replace("3600,1000,", "3600,10,"),
        &[Uuid::now_v7(), note],
        "22023",
    )
    .await;
}

#[tokio::test]
async fn organization_storage_is_shared_across_workspaces_and_rechecked_at_finish() {
    let w = World::new().await;
    let doc = w.doc().await;
    let ws2 = workspace(&w.state, &w.owner, w.org).await;
    let doc2 = made(&w.state,&w.owner,"SELECT fvoci.app_create_wiki_document($1,$2,'second',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",&[ws2,Uuid::now_v7()]).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_settings($1,'{\"max_storage_bytes\":30}',@ids)",
        &[w.org],
    )
    .await;
    let upload = made(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc]).await;
    denied(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc2], "55000").await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_settings($1,'{\"max_storage_bytes\":10}',@ids)",
        &[w.org],
    )
    .await;
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[upload],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_finish_upload($1,repeat('a',64),10000,@ids)",
        &[upload],
        "55000",
    )
    .await;
    let unchanged = one(
        &w.state,
        &w.owner,
        "SELECT state,version_id FROM fvoci.uploads WHERE id=$1",
        &[upload],
    )
    .await;
    assert_eq!(unchanged["state"], "completing");
    assert!(unchanged["version_id"].is_null());
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_settings($1,'{\"max_storage_bytes\":30}',@ids)",
        &[w.org],
    )
    .await;
    made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_finish_upload($1,repeat('a',64),10000,@ids) AS id",
        &[upload],
    )
    .await;
    denied(&w.state, &w.owner, UPLOAD, &[Uuid::now_v7(), doc2], "55000").await;
}
