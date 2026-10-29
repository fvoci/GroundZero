//! 기존 화면 검증을 작은 학습 자원 읽기에서 조합한다.

use crate::common::{TestUser, send};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};

pub(super) enum LearningKind {
    Course,
    Study,
}

pub(super) async fn view(app: &Router, user: &TestUser, item: &str, kind: LearningKind) -> Value {
    let (metadata, rows, progress, key, progress_id, empty) = match kind {
        LearningKind::Course => (
            "course",
            "episodes",
            "episode-progress",
            "episodes",
            "episode_id",
            json!({"status":"not_started","planned_on":null,"completed_on":null,"watch_count":0,"last_position_seconds":null,"note":""}),
        ),
        LearningKind::Study => (
            "study",
            "study/units",
            "study/progress",
            "units",
            "unit_id",
            json!({"status":"not_started","done_amount":0,"planned_on":null,"completed_on":null,"note":""}),
        ),
    };
    let mut info = send(
        app,
        Method::GET,
        &format!("/api/items/{item}/{metadata}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(info.status, StatusCode::OK, "{}", info.json);
    let rows = send(
        app,
        Method::GET,
        &format!("/api/items/{item}/{rows}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(rows.status, StatusCode::OK, "{}", rows.json);
    let progress = send(
        app,
        Method::GET,
        &format!("/api/items/{item}/{progress}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(progress.status, StatusCode::OK, "{}", progress.json);
    assert!(rows.json.is_array(), "{}", rows.json);
    assert!(progress.json.is_array(), "{}", progress.json);
    let mut rows = rows.json.as_array().cloned().unwrap_or_default();
    for row in &mut rows {
        let mut mine = progress
            .json
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry[progress_id] == row["id"])
            .cloned()
            .unwrap_or_else(|| empty.clone());
        if let Some(object) = mine.as_object_mut() {
            object.remove(progress_id);
        }
        row["mine"] = mine;
    }
    let can_write = info.json["access"] == "write";
    if let Some(object) = info.json.as_object_mut() {
        object.remove("access");
    }
    info.json["can_write"] = json!(can_write);
    info.json[key] = json!(rows);
    info.json
}
