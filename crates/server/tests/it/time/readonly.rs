#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
use crate::{
    common::{self, send},
    sql_support::World,
};
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn readonly_users_keep_personal_time_on_active_and_archived_items() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let task = w.task().await;
    let reader = w.member("readonly").await;
    for state in ["active", "archived"] {
        if state == "archived" {
            let r = send(
                &app,
                Method::PUT,
                &format!("/api/items/{}/archived", w.item),
                Some(&w.owner),
                Some(json!({"archived":true})),
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
        }
        // 공간 내용의 수정은 계속 금지된다.
        let content = send(
            &app,
            Method::PATCH,
            &format!("/api/tasks/{task}"),
            Some(&reader),
            Some(json!({"base_version":1,"title":"not allowed"})),
        )
        .await;
        assert!(
            matches!(content.status, StatusCode::FORBIDDEN | StatusCode::CONFLICT),
            "{:?}",
            content.json
        );
        let mut body = json!({"request_id":Uuid::now_v7(),"started_at":"2026-10-20T00:00:00Z",
            "ended_at":"2026-10-20T00:01:00Z","paused_seconds":0,"kind":"work","memo":state,
            "target":{"kind":"task","id":task}});
        let entry = send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(&reader),
            Some(body.clone()),
        )
        .await;
        assert_eq!(
            entry.status,
            StatusCode::CREATED,
            "{state}: {:?}",
            entry.json
        );
        let uri = format!("/api/time-entries/{}", entry.json["id"].as_str().unwrap());
        body["base_version"] = entry.json["version"].clone();
        body["memo"] = json!("my corrected memo");
        let edited = send(&app, Method::PUT, &uri, Some(&reader), Some(body)).await;
        assert_eq!(edited.status, StatusCode::OK, "{state}: {:?}", edited.json);
        let history = send(
            &app,
            Method::GET,
            &format!("{uri}/revisions"),
            Some(&reader),
            None,
        )
        .await;
        assert_eq!(history.json.as_array().unwrap().len(), 1);
        let timer = send(&app, Method::POST, "/api/me/timer", Some(&reader),
            Some(json!({"request_id":Uuid::now_v7(),"kind":"work","memo":state,"target":{"kind":"task","id":task}}))).await;
        assert_eq!(
            timer.status,
            StatusCode::CREATED,
            "{state}: {:?}",
            timer.json
        );
        let timer_uri = format!(
            "/api/time-entries/{}/timer",
            timer.json["id"].as_str().unwrap()
        );
        for action in ["pause", "resume", "stop"] {
            let r = send(
                &app,
                Method::POST,
                &timer_uri,
                Some(&reader),
                Some(json!({"action":action})),
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{state}/{action}: {:?}", r.json);
        }
        let removed = send(&app, Method::DELETE, &uri, Some(&reader), None).await;
        assert_eq!(
            removed.status,
            StatusCode::NO_CONTENT,
            "{state}: {:?}",
            removed.json
        );
    }
}
