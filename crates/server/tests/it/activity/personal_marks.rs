//! D-113 개정 2026-10-28 SR1-11: 읽기 전용도 본인의 고정·즐겨찾기를 남긴다.

use crate::common::{self, send};
use crate::sql_support::{World, fixture, rows};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn readonly_members_keep_private_pins_and_favorites_for_visible_targets() {
    let w = World::new().await;
    let reader = w.member("readonly").await;
    let task = w.task().await;
    let doc = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='workspace' WHERE id=$1",
        &[doc],
    )
    .await;
    let app = common::app(&w.state);
    for (kind, id) in [("item", w.item), ("task", task), ("document", doc)] {
        let uri = format!("/api/me/marks/{kind}/{id}");
        for mark in ["pin", "favorite"] {
            let r = send(
                &app,
                Method::PUT,
                &uri,
                Some(&reader),
                Some(json!({"mark":mark,"on":true})),
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{kind} {mark}: {:?}", r.json);
        }
        let mine = send(&app, Method::GET, &uri, Some(&reader), None).await;
        assert_eq!(mine.status, StatusCode::OK);
        assert_eq!(mine.json, json!({"pin":true,"favorite":true}));
        let owner = send(&app, Method::GET, &uri, Some(&w.owner), None).await;
        assert_eq!(owner.status, StatusCode::OK);
        assert_eq!(owner.json, json!({"pin":false,"favorite":false}));
        let visible = rows(
            &w.state,
            &w.owner,
            "SELECT mark FROM fvoci.user_marks WHERE target_id=$1",
            &[id],
        )
        .await;
        assert!(
            visible.is_empty(),
            "the owner must not see another user's marks"
        );
        let r = send(
            &app,
            Method::PUT,
            &uri,
            Some(&reader),
            Some(json!({"mark":"pin","on":false})),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.json, json!({"pin":false,"favorite":true}));
    }
    let home = send(
        &app,
        Method::GET,
        &format!("/api/me/home?organization_id={}", w.org),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(home.status, StatusCode::OK, "{:?}", home.json);
    assert_eq!(home.json["favorites"].as_array().map(Vec::len), Some(3));
    assert_eq!(home.json["pins"], json!([]));
}
