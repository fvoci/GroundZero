#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 댓글 목록의 고치기·해결 표시는 서버의 쓰기 등급 판단과 같다(D-113·D-20). 고치기와 해결은 SQL이 쓰기 등급을 요구한다.

use crate::common;
use crate::sql_support;

use axum::http::Method;
use sql_support::*;
use uuid::Uuid;

/// SR1-6: 읽기 전용으로 강등된 작성자에게는 자기 댓글이라도 고치기·해결 표시가 꺼진다.
#[tokio::test]
async fn demoted_author_sees_no_edit_or_resolve() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let author = w.member("member").await;
    let task = w.task().await;
    rows(&w.state, &author, COMMENT, &[Uuid::now_v7(), task]).await;
    let uri = format!("/api/tasks/{task}/comments");
    let list = common::send(&app, Method::GET, &uri, Some(&author), None)
        .await
        .json;
    assert_eq!(list[0]["can_edit"], true, "{list:?}");
    assert_eq!(list[0]["can_resolve"], true, "{list:?}");

    fixture(
        "UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws, author.id.0],
    )
    .await;
    let list = common::send(&app, Method::GET, &uri, Some(&author), None)
        .await
        .json;
    assert_eq!(list[0]["can_edit"], false, "{list:?}");
    assert_eq!(list[0]["can_resolve"], false, "{list:?}");
}
