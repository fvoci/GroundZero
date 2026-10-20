#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 저장한 보기 (D-31): 사람마다 따로(본인만 읽음), 같은 이름이면 덮어쓰기, 항목을 볼 수 있어야 한다.

mod common;

use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::json;

#[tokio::test]
async fn saved_views_are_personal_and_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;
    let m = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "VIEW" }))).await;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    let item_id = item.json["id"].as_str().unwrap().to_owned();
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    let uri = format!("/api/items/{item_id}/views");

    let config =
        json!({ "view": "list", "filter": { "assignee": "me", "status": "todo" }, "sort": "due" });
    let saved = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "name": "내 할 일", "config": config })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::CREATED, "{:?}", saved.json);
    assert_eq!(saved.json["config"]["sort"], "due");
    // 같은 이름이면 덮어쓴다.
    let again = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(),
        "name": "내 할 일", "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(again.json["id"], saved.json["id"]);
    let mine = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(mine.json.as_array().unwrap().len(), 1);
    assert_eq!(mine.json[0]["config"], json!({ "view": "board" }));
    // 다른 사람에게는 보이지 않고 지울 수도 없다.
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&admin), None).await.json,
        json!([])
    );
    let view_uri = format!("/api/views/{}", saved.json["id"].as_str().unwrap());
    assert_eq!(
        send(&app, Method::DELETE, &view_uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            Some(&writer),
            Some(json!({ "request_id": new_request_id(), "name": " ", "config": {} }))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    // 항목을 볼 수 없게 되면 보기도 보이지 않는다.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(
        send(&app, Method::DELETE, &view_uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

/// 내 시간대(D-50): 본인만 보고, 모르는 이름은 400, 비우면 지운다.
#[tokio::test]
async fn time_zone_is_a_personal_setting() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "tz-me").await;
    let other = login(&state, "tz-other").await;
    let set = |zone: &str| {
        let app = app.clone();
        let me = me.clone();
        let body = json!({ "time_zone": zone });
        async move {
            send(&app, Method::PUT, "/api/me/settings", Some(&me), Some(body))
                .await
                .status
        }
    };
    assert_eq!(set("Asia/Seoul").await, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&me), None)
            .await
            .json["time_zone"],
        "Asia/Seoul"
    );
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&other), None)
            .await
            .json["time_zone"],
        ""
    );
    assert_eq!(set("Mars/Olympus").await, StatusCode::BAD_REQUEST);
    assert_eq!(set("").await, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&me), None)
            .await
            .json["time_zone"],
        ""
    );
}

/// 표시 설정(D-81): 언어·주 시작·시각 표시를 따로 저장하고, 보내지 않은 칸은 그대로 둔다. 본인만 보고 모르는 값은 400.
#[tokio::test]
async fn display_settings_are_personal_and_kept_independently() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "display-me").await;
    let other = login(&state, "display-other").await;
    let put = |body: serde_json::Value| {
        let app = app.clone();
        let me = me.clone();
        async move {
            send(&app, Method::PUT, "/api/me/settings", Some(&me), Some(body))
                .await
                .status
        }
    };
    let fields = |user: TestUser| {
        let app = app.clone();
        async move {
            let v = send(&app, Method::GET, "/api/me", Some(&user), None)
                .await
                .json;
            [
                v["time_zone"].as_str().unwrap().to_owned(),
                v["locale"].as_str().unwrap().to_owned(),
                v["week_start"].as_str().unwrap().to_owned(),
                v["hour_cycle"].as_str().unwrap().to_owned(),
            ]
        }
    };
    // 처음에는 아무것도 고르지 않았다(화면이 브라우저·로케일로 제안).
    assert_eq!(fields(me.clone()).await, ["", "", "", ""]);
    assert_eq!(
        put(json!({ "time_zone": "Asia/Seoul", "locale": "en", "week_start": "sun", "hour_cycle": "h12" })).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(fields(me.clone()).await, ["Asia/Seoul", "en", "sun", "h12"]);
    // 한 칸만 바꾸면 나머지는 그대로다(예전 화면은 시간대만 보낸다).
    assert_eq!(
        put(json!({ "time_zone": "America/New_York" })).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        put(json!({ "week_start": "mon" })).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        fields(me.clone()).await,
        ["America/New_York", "en", "mon", "h12"]
    );
    for bad in [
        json!({ "locale": "fr" }),
        json!({ "week_start": "sat" }),
        json!({ "hour_cycle": "h24" }),
        json!({ "time_zone": "Mars/Olympus" }),
    ] {
        assert_eq!(put(bad).await, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        fields(me.clone()).await,
        ["America/New_York", "en", "mon", "h12"]
    );
    assert_eq!(fields(other.clone()).await, ["", "", "", ""]);
    // 빈 값은 "고르지 않음"으로 되돌린다.
    assert_eq!(put(json!({ "locale": "" })).await, StatusCode::NO_CONTENT);
    assert_eq!(fields(me.clone()).await[1], "");
}

/// 초기 관리자(D-51): 운영자가 지정한 OIDC subject가 로그인하면 첫 워크스페이스의 소유자가 된다. 한 번만, 다른 사람은
/// 아무것도 얻지 않는다(첫 로그인 사용자 자동 관리자 없음, X-10).
#[tokio::test]
async fn the_designated_initial_admin_gets_the_first_workspace_once() {
    use fvoci_server::features::accounts::{
        SESSION_COOKIE, VerifiedIdentity, claim_initial_admin, start_session,
    };
    let subject = format!("operator-{}", uuid::Uuid::now_v7());
    // 초기 관리자는 DB마다 한 번만 정해진다. 다른 실행·시험이 먼저 정했을 수 있는 공유 DB가 아니라 새 DB를 쓴다(공유 DB를
    // 다시 쓰면 이미 정해져 실패했다).
    let fresh = {
        use sqlx::Connection as _;
        let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
        let mut admin = sqlx::PgConnection::connect(&admin_url).await.unwrap();
        common::fresh_database(&mut admin, "initial_admin").await
    };
    let config = common::config_with(&[
        ("DATABASE_URL", &fresh.app_url),
        ("FVOCI_OIDC_ISSUER", "https://test-issuer.invalid"),
        ("FVOCI_OIDC_CLIENT_ID", "fvoci-test"),
        ("FVOCI_OIDC_CLIENT_SECRET", "test-only-secret"),
        ("FVOCI_INITIAL_ADMIN_SUBJECT", &subject),
        ("FVOCI_INITIAL_WORKSPACE", "우리 팀"),
    ]);
    let state = fvoci_server::app::build_state(config).unwrap();
    let app = common::app(&state);
    let identity = |subject: &str| VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: subject.to_owned(),
        email: None,
        email_verified: false,
        display_name: "운영자".to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    // 다른 사람이 먼저 로그인해도 관리자가 되지 않는다.
    let stranger = identity("someone-else");
    let (stranger_id, _) = start_session(&state.db, state.config.session_ttl, &stranger, None)
        .await
        .unwrap();
    assert_eq!(
        claim_initial_admin(&state.db, &state.config, &stranger, stranger_id)
            .await
            .unwrap(),
        None
    );

    let operator = identity(&subject);
    let (user, token) = start_session(&state.db, state.config.session_ttl, &operator, None)
        .await
        .unwrap();
    let ws = claim_initial_admin(&state.db, &state.config, &operator, user)
        .await
        .unwrap();
    assert!(ws.is_some());
    assert_eq!(
        claim_initial_admin(&state.db, &state.config, &operator, user)
            .await
            .unwrap(),
        None
    );
    let me = common::TestUser {
        id: user,
        cookie: format!("{SESSION_COOKIE}={token}"),
    };
    let list = send(&app, Method::GET, "/api/workspaces", Some(&me), None).await;
    let first = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["name"] == "우리 팀")
        .unwrap()
        .clone();
    assert_eq!(first["role"], "admin");
}
