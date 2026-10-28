//! 개인 시간 기록은 본인에게도 달력 출처가 아니다(D-80).

use super::{Visibility, calendar, module_item};
use crate::common;
use axum::http::{Method, StatusCode};
use common::{add_member, create_workspace, login, new_request_id, send};
use serde_json::json;

#[tokio::test]
async fn personal_time_entries_are_excluded_for_their_owner_and_workspace_admin() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "시간 없는 달력").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "TIME", Visibility::Workspace).await;
    let before = calendar(&app, &writer, &ws, "2026-11-01", "2026-11-30", "UTC").await;
    assert_eq!(before.0, StatusCode::OK, "{:?}", before.1);
    for user in [&writer, &admin] {
        let entry = send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(user),
            Some(json!({
                "request_id": new_request_id(), "started_at": "2026-11-05T09:00:00Z",
                "ended_at": "2026-11-05T10:00:00Z", "paused_seconds": 0, "kind": "work",
                "memo": "달력에는 없는 개인 시간", "target": { "kind": "item", "id": item["id"] }
            })),
        )
        .await;
        assert_eq!(entry.status, StatusCode::CREATED, "{:?}", entry.json);
    }
    for user in [&writer, &admin] {
        for suffix in [
            String::new(),
            format!("&module_id={}", item["module_id"].as_str().unwrap()),
            format!("&item_id={}", item["id"].as_str().unwrap()),
        ] {
            let result = send(
                &app,
                Method::GET,
                &format!(
                    "/api/workspaces/{ws}/calendar?from=2026-11-01&to=2026-11-30&tz=UTC{suffix}"
                ),
                Some(user),
                None,
            )
            .await;
            assert_eq!(result.status, StatusCode::OK, "{:?}", result.json);
            assert_eq!(
                result.json, before.1,
                "time entries must not add any calendar source"
            );
        }
    }
}
