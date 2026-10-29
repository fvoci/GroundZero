//! 기록·학습의 선택 기대 사용자와 자원·칸 오류 계약.

use crate::{common, sql_support::World};
use axum::http::{Method, StatusCode};
use common::{new_request_id, send, send_raw};
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn every_records_command_compares_an_expected_user_if_sent() -> Result<(), serde_json::Error>
{
    let world = World::new().await;
    let app = common::app(&world.state);
    let item = world.item;
    let id = Uuid::now_v7();
    let foreign = Uuid::now_v7().to_string();
    let commands = [
        (Method::POST, format!("/api/items/{item}/records")),
        (Method::PUT, format!("/api/records/{id}")),
        (Method::DELETE, format!("/api/records/{id}")),
        (Method::PUT, format!("/api/items/{item}/course")),
        (Method::POST, format!("/api/items/{item}/episodes")),
        (Method::PUT, format!("/api/episodes/{id}")),
        (Method::DELETE, format!("/api/episodes/{id}")),
        (Method::PUT, format!("/api/episodes/{id}/progress")),
        (Method::POST, format!("/api/items/{item}/study/units")),
        (Method::PUT, format!("/api/study-units/{id}")),
        (Method::DELETE, format!("/api/study-units/{id}")),
        (Method::PUT, format!("/api/study-units/{id}/progress")),
        (Method::PUT, format!("/api/items/{item}/study/plan")),
        (Method::POST, "/api/zotero/collections".into()),
        (Method::POST, format!("/api/items/{item}/zotero/import")),
    ];
    for (method, uri) in commands {
        let (status, _, bytes) = send_raw(
            &app,
            method.clone(),
            &uri,
            Some(&world.owner),
            &[
                ("content-type", "application/json"),
                ("x-fvoci-expected-user", &foreign),
            ],
            b"{}".to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{method} {uri}");
        let body: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(body["error"], "account_changed", "{method} {uri}: {body}");
    }
    let uri = format!("/api/records/{id}");
    let (status, _, _) = send_raw(
        &app,
        Method::DELETE,
        &uri,
        Some(&world.owner),
        &[("x-fvoci-expected-user", "invalid")],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let own = world.owner.id.0.to_string();
    let (status, _, _) = send_raw(
        &app,
        Method::DELETE,
        &uri,
        Some(&world.owner),
        &[
            ("x-fvoci-expected-user", &own),
            ("x-fvoci-expected-user", &own),
        ],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = send_raw(
        &app,
        Method::DELETE,
        &uri,
        None,
        &[("x-fvoci-expected-user", "invalid")],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn course_limits_name_the_field_and_keep_batches_atomic_without_a_required_header()
-> Result<(), serde_json::Error> {
    let world = World::of_kind("course").await;
    let app = common::app(&world.state);
    let uri = format!("/api/items/{}/episodes", world.item);
    let rows = json!({"episodes":[
        {"request_id":new_request_id(),"title":"valid"},
        {"request_id":new_request_id(),"title":"가".repeat(301)}
    ]});
    let invalid = send(&app, Method::POST, &uri, Some(&world.owner), Some(rows)).await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST, "{}", invalid.json);
    assert_eq!(invalid.json["reason"], "too_long");
    assert_eq!(invalid.json["field"], "title");
    assert_eq!(invalid.json["limit"], 300);
    let empty = send(&app, Method::GET, &uri, Some(&world.owner), None).await;
    assert_eq!(empty.json, json!([]));
    let rows = json!({"episodes":[{"request_id":new_request_id(),"title":"가".repeat(300)}]});
    let (status, _, bytes) = send_raw(
        &app,
        Method::POST,
        &uri,
        Some(&world.owner),
        &[("content-type", "application/json")],
        serde_json::to_vec(&rows)?,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<Value>(&bytes)?, json!({"added":1}));
    let repeated = send(&app, Method::POST, &uri, Some(&world.owner), Some(rows)).await;
    assert_eq!(repeated.json, json!({"added":0}));
    let excessive =
        json!({"episodes":vec![json!({"request_id":new_request_id(),"title":"valid"});1001]});
    let invalid = send(
        &app,
        Method::POST,
        &uri,
        Some(&world.owner),
        Some(excessive),
    )
    .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid.json["field"], "episodes");
    assert_eq!(invalid.json["reason"], "too_many");
    assert_eq!(invalid.json["limit"], 1000);
    Ok(())
}

#[tokio::test]
async fn learning_resources_keep_private_progress_out_of_shared_reads() {
    for (kind, rows, progress, entry_key, id_key, fields) in [
        (
            "course",
            "episodes",
            "episode-progress",
            "episodes",
            "episode_id",
            json!({"last_position_seconds":25}),
        ),
        (
            "study",
            "study/units",
            "study/progress",
            "units",
            "unit_id",
            json!({"done_amount":3}),
        ),
    ] {
        let world = World::of_kind(kind).await;
        let app = common::app(&world.state);
        let writer = world.member("member").await;
        let uri = format!("/api/items/{}/{rows}", world.item);
        let mut body = json!({});
        body[entry_key] = json!([{"request_id":new_request_id(),"title":"shared"}]);
        let added = send(&app, Method::POST, &uri, Some(&world.owner), Some(body)).await;
        assert_eq!(added.json, json!({"added":1}));
        let shared = send(&app, Method::GET, &uri, Some(&writer), None).await;
        assert_eq!(shared.status, StatusCode::OK);
        assert!(shared.json[0]["mine"].is_null());
        let id = &shared.json[0]["id"];
        let unit = if kind == "course" {
            "episodes"
        } else {
            "study-units"
        };
        let mut input = fields;
        input["status"] = json!("in_progress");
        input["note"] = json!("private writer note");
        let id = id.as_str().unwrap_or_default();
        assert!(!id.is_empty());
        let saved = send(
            &app,
            Method::PUT,
            &format!("/api/{unit}/{id}/progress"),
            Some(&writer),
            Some(input),
        )
        .await;
        assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.json);
        let uri = format!("/api/items/{}/{progress}", world.item);
        let mine = send(&app, Method::GET, &uri, Some(&writer), None).await;
        assert_eq!(mine.status, StatusCode::OK);
        assert_eq!(mine.json[0][id_key], id);
        assert_eq!(mine.json[0]["note"], "private writer note");
        let owner = send(&app, Method::GET, &uri, Some(&world.owner), None).await;
        assert_eq!(owner.status, StatusCode::OK);
        assert_eq!(owner.json, json!([]));
        let shared = send(
            &app,
            Method::GET,
            &format!("/api/items/{}/{rows}", world.item),
            Some(&world.owner),
            None,
        )
        .await;
        assert!(shared.json[0]["mine"].is_null());
        assert!(!shared.json.to_string().contains("private writer note"));
    }
}
