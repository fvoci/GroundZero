//! D-120·D-19와 집계 읽기의 HTTP 계약.
use crate::common::{self, send};
use crate::sql_support::{World, fixture, made};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

async fn home(w: &World, user: &common::TestUser) -> Value {
    let response = send(
        &common::app(&w.state),
        Method::GET,
        &format!("/api/me/home?organization_id={}&tz=UTC", w.org),
        Some(user),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
    response.json
}

#[tokio::test]
async fn active_items_require_active_status_and_current_organization_access() {
    let w = World::new().await;
    let me = w.member("member").await;
    let inactive = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_item($1,$2,'inactive','',@ids) AS id",
        &[w.module, Uuid::now_v7()],
    )
    .await;
    fixture(
        "UPDATE fvoci.items SET visibility='workspace', status='done' WHERE id=$1",
        &[inactive],
    )
    .await;
    let private = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_item($1,$2,'private','',@ids) AS id",
        &[w.module, Uuid::now_v7()],
    )
    .await;
    let other = World::new().await;
    common::join_organization(other.org, &me, "member").await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[other.ws, me.id.0],
    )
    .await;
    let value = home(&w, &me).await;
    let ids: Vec<_> = value["active_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].clone())
        .collect();
    assert_eq!(
        ids,
        [json!(w.item)],
        "inactive {inactive}, private {private}, other {}",
        other.item
    );
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    assert_eq!(home(&w, &me).await["active_items"], json!([]));
}
