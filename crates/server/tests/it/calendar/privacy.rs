//! 강의·스터디 개인 계획은 본인만 항목 접근권한 안에서 본다(D-80).

use super::calendar;
use crate::common;
use axum::http::{Method, StatusCode};
use common::{add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

#[tokio::test]
async fn course_and_study_plans_are_private_and_disappear_when_item_access_is_removed() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "개인 계획").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    for (kind, key) in [("course", "CPLAN"), ("study", "SPLAN")] {
        let module = send(
            &app,
            Method::POST,
            &format!("/api/workspaces/{ws}/modules"),
            Some(&admin),
            Some(json!({ "request_id": new_request_id(), "kind": kind, "key": key, "name": key })),
        )
        .await;
        assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
        let module_id = module.json["id"].as_str().unwrap();
        let item = send(
            &app,
            Method::POST,
            &format!("/api/modules/{module_id}/items"),
            Some(&admin),
            Some(json!({ "request_id": new_request_id(), "title": key })),
        )
        .await;
        assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
        let item_id = item.json["id"].as_str().unwrap();
        let visible = send(&app, Method::PUT, &format!("/api/items/{item_id}"), Some(&admin),
            Some(json!({ "base_version": item.json["version"], "title": key, "description": "", "visibility": "workspace" }))).await;
        assert_eq!(visible.status, StatusCode::OK, "{:?}", visible.json);
        let (resource_path, create_body) = if kind == "course" {
            (
                "episodes",
                json!({ "episodes": [{ "request_id": new_request_id(), "title": key, "duration_seconds": 60, "planned_on": "2026-11-05" }] }),
            )
        } else {
            (
                "study/units",
                json!({ "units": [{ "request_id": new_request_id(), "title": key, "unit": "pages", "amount": 10, "planned_on": "2026-11-05" }] }),
            )
        };
        let added = send(
            &app,
            Method::POST,
            &format!("/api/items/{item_id}/{resource_path}"),
            Some(&admin),
            Some(create_body),
        )
        .await;
        assert!(added.status.is_success(), "{:?}", added.json);
        let plan = send(
            &app,
            Method::GET,
            &format!("/api/items/{item_id}/{resource_path}"),
            Some(&writer),
            None,
        )
        .await;
        assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
        let units = plan
            .json
            .as_array()
            .expect("shared learning resource array");
        assert_eq!(units.len(), 1, "{:?}", plan.json);
        let unit_id = units[0]["id"].as_str().expect("learning resource id");
        let (path, body) = if kind == "course" {
            (
                format!("/api/episodes/{unit_id}/progress"),
                json!({ "status": "not_started", "planned_on": "2026-11-10" }),
            )
        } else {
            (
                format!("/api/items/{item_id}/study/plan"),
                json!({ "entries": [{ "unit_id": unit_id, "planned_on": "2026-11-10" }] }),
            )
        };
        let planned = send(&app, Method::PUT, &path, Some(&writer), Some(body)).await;
        assert!(planned.status.is_success(), "{:?}", planned.json);
        let mine = calendar(&app, &writer, &ws, "2026-11-01", "2026-11-30", "UTC").await;
        let theirs = calendar(&app, &admin, &ws, "2026-11-01", "2026-11-30", "UTC").await;
        for (result, date, personal) in
            [(&mine, "2026-11-10", true), (&theirs, "2026-11-05", false)]
        {
            assert_eq!(result.0, StatusCode::OK, "{:?}", result.1);
            let dates: Vec<&Value> = result.1["dates"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["item_id"] == item_id)
                .collect();
            assert_eq!(dates.len(), 1, "{dates:?}");
            assert_eq!(dates[0]["date"], date);
            assert_eq!(dates[0]["personal"], personal);
        }
        let hidden = send(&app, Method::PUT, &format!("/api/items/{item_id}"), Some(&admin),
            Some(json!({ "base_version": visible.json["version"], "title": key, "description": "", "visibility": "private" }))).await;
        assert_eq!(hidden.status, StatusCode::OK, "{:?}", hidden.json);
        let mine = calendar(&app, &writer, &ws, "2026-11-01", "2026-11-30", "UTC").await;
        assert_eq!(mine.0, StatusCode::OK, "{:?}", mine.1);
        assert!(
            mine.1["dates"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| d["item_id"] != item_id)
        );
        let hidden_scope = send(&app, Method::GET,
            &format!("/api/workspaces/{ws}/calendar?from=2026-11-01&to=2026-11-30&tz=UTC&item_id={item_id}"),
            Some(&writer), None).await;
        assert_eq!(hidden_scope.status, StatusCode::NOT_FOUND);
    }
}
