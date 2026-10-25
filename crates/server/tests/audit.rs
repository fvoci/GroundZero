#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 감사 기록 보기 (D-57): 워크스페이스 기록은 관리자만, 개인 영역 기록은 본인만.

mod common;

use axum::http::{Method, StatusCode};
use common::{add_member, create_doc, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

fn actions(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}:{}",
                e["action"].as_str().unwrap(),
                e["target_type"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test]
async fn workspace_audit_is_for_admins_and_personal_audit_is_private() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "감사").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &writer, &ws, "기록될 문서").await;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);

    let uri = format!("/api/workspaces/{ws}/audit");
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&writer), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let log = send(&app, Method::GET, &uri, Some(&admin), None).await;
    assert_eq!(log.status, StatusCode::OK, "{:?}", log.json);
    let seen = actions(&log.json);
    assert!(seen.contains(&"invite:invitation".to_owned()), "{seen:?}");
    assert!(seen.contains(&"join:member".to_owned()), "{seen:?}");
    assert!(seen.contains(&"delete:document".to_owned()), "{seen:?}");
    // 최신 먼저, 누가 했는지 이름과 함께.
    assert_eq!(log.json[0]["action"], "delete");
    assert_eq!(log.json[0]["actor_name"], "member");
    // 쪽 넘기기: 마지막 것보다 앞선 기록만.
    let last = log.json.as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let older = send(
        &app,
        Method::GET,
        &format!("{uri}?before={last}"),
        Some(&admin),
        None,
    )
    .await;
    assert!(
        older
            .json
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"].as_str().unwrap() < last.as_str())
    );

    // 개인 노트의 기록은 본인만 본다(관리자의 개인 기록에도, 워크스페이스 기록에도 없다).
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "개인 노트" })),
    )
    .await
    .json;
    send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", note["id"].as_str().unwrap()),
        Some(&writer),
        None,
    )
    .await;
    let mine = send(&app, Method::GET, "/api/me/audit", Some(&writer), None).await;
    assert!(
        actions(&mine.json).contains(&"delete:document".to_owned()),
        "{:?}",
        mine.json
    );
    assert!(
        mine.json
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["target_id"] != doc["id"]),
        "workspace actions are not in the personal log"
    );
    let theirs = send(&app, Method::GET, "/api/me/audit", Some(&admin), None).await;
    assert!(
        theirs
            .json
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["target_id"] != note["id"])
    );
    let admin_log = send(&app, Method::GET, &uri, Some(&admin), None).await;
    assert!(
        admin_log
            .json
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["target_id"] != note["id"])
    );
}
