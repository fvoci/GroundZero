//! 문자열 경계의 실제 API·DB 검증. 브라우저의 사전 검증과 별개로 서버가 최종 판단한다.

mod common;

use axum::http::{Method, StatusCode};
use common::{create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

#[tokio::test]
async fn module_names_preserve_unicode_and_keys_remain_ascii()
-> Result<(), Box<dyn std::error::Error>> {
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "unicode-module").await;
    let ws = create_workspace(&app, &user, "문자 경계").await;
    let uri = format!("/api/workspaces/{ws}/modules");
    let names = [
        "한글".to_owned(),
        "ㄱㅏ".to_owned(),
        "가".to_owned(),
        "誤".to_owned(),
        "Latin 123 !?".to_owned(),
        "e\u{301}".to_owned(),
        "👩‍👩‍👧‍👦".to_owned(),
        "😀".repeat(200),
    ];
    for (index, name) in names.iter().enumerate() {
        let key = format!("U{index}");
        let created = send(
            &app,
            Method::POST,
            &uri,
            Some(&user),
            Some(json!({"request_id": new_request_id(), "kind": "project", "name": name, "key": key})),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
        let id = created.json["id"].as_str().ok_or("missing module id")?;
        let stored = send(
            &app,
            Method::GET,
            &format!("/api/modules/{id}"),
            Some(&user),
            None,
        )
        .await;
        assert_eq!(stored.status, StatusCode::OK);
        assert_eq!(stored.json["name"], *name);
        assert_eq!(stored.json["key"], key);
    }
    for key in [
        "ß",
        "ıd",
        "ſa",
        "ﬀ",
        "ＡＢ",
        "한글",
        "\u{feff}DEV",
        "",
        "A",
        "1A",
        "A B",
        "wiki",
    ] {
        let result = send(
            &app,
            Method::POST,
            &uri,
            Some(&user),
            Some(json!({"request_id": new_request_id(), "kind": "project", "name": "이름", "key": key})),
        )
        .await;
        assert_eq!(
            result.status,
            StatusCode::BAD_REQUEST,
            "key {key:?}: {:?}",
            result.json
        );
    }
    let ascii = send(
        &app,
        Method::POST,
        &uri,
        Some(&user),
        Some(json!({"request_id": new_request_id(), "kind": "project", "name": "공백", "key": "\u{85}dev\u{85}"})),
    )
    .await;
    assert_eq!(ascii.status, StatusCode::CREATED, "{:?}", ascii.json);
    assert_eq!(ascii.json["key"], "DEV");
    for name in [
        String::new(),
        "   ".to_owned(),
        "a\u{7}b".to_owned(),
        "😀".repeat(201),
    ] {
        let result = send(
            &app,
            Method::POST,
            &uri,
            Some(&user),
            Some(json!({"request_id": new_request_id(), "kind": "project", "name": name, "key": "BAD"})),
        )
        .await;
        assert_eq!(result.status, StatusCode::BAD_REQUEST, "{:?}", result.json);
    }
    Ok(())
}

#[tokio::test]
async fn episode_api_counts_scalars_and_rejected_batches_leave_no_rows()
-> Result<(), Box<dyn std::error::Error>> {
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "unicode-course").await;
    let ws = create_workspace(&app, &user, "회차 문자 경계").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&user),
        Some(
            json!({"request_id": new_request_id(), "kind": "course", "name": "강의", "key": "LEC"}),
        ),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let module_id = module.json["id"].as_str().ok_or("missing module id")?;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{module_id}/items"),
        Some(&user),
        Some(json!({"request_id": new_request_id(), "title": "한글 가 誤 😀"})),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    let item_id = item.json["id"].as_str().ok_or("missing item id")?;
    let uri = format!("/api/items/{item_id}/episodes");
    let titles = [
        "한".repeat(300),
        "誤".repeat(300),
        "😀".repeat(300),
        "가".repeat(150),
        "e\u{301}".repeat(150),
    ];
    let episodes: Vec<Value> = titles
        .iter()
        .map(|title| json!({"request_id": new_request_id(), "title": title}))
        .collect();
    let accepted = send(
        &app,
        Method::POST,
        &uri,
        Some(&user),
        Some(json!({"episodes": episodes})),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.json);
    assert_eq!(accepted.json["added"], titles.len());
    for title in [
        "😀".repeat(301),
        "한".repeat(301),
        String::new(),
        "   ".to_owned(),
    ] {
        let rejected = send(
            &app,
            Method::POST,
            &uri,
            Some(&user),
            Some(json!({"episodes": [{"request_id": new_request_id(), "title": title}]})),
        )
        .await;
        assert_eq!(
            rejected.status,
            StatusCode::BAD_REQUEST,
            "{:?}",
            rejected.json
        );
    }
    let stored = send(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/course"),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(stored.status, StatusCode::OK, "{:?}", stored.json);
    let rows = stored.json["episodes"]
        .as_array()
        .ok_or("missing episodes")?;
    assert_eq!(rows.len(), titles.len());
    for title in titles {
        assert!(rows.iter().any(|row| row["title"] == title));
    }
    Ok(())
}
