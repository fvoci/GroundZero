#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 저장한 보기 (D-31): 사람마다 따로(본인만 읽음), 항목을 볼 수 있어야 한다. 기존 보기 고치기는 읽은 판을 확인한다(D-103).

use crate::common;

use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

#[tokio::test]
async fn saved_views_are_personal_and_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    let uri = format!("/api/items/{item_id}/views");

    let config =
        json!({ "view": "list", "filter": { "assignee": "me", "status": "할 일" }, "sort": "due" });
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
    // 같은 이름은 새로 만들지 않는다(기존 보기는 판을 붙인 고치기로만 바꾼다, D-103).
    let again = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(),
        "name": "내 할 일", "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{:?}", again.json);
    assert_eq!(again.json["error"], "exists");
    assert_eq!(saved.json["version"], 1);
    assert_eq!(again.json["current"]["id"], saved.json["id"]);
    assert_eq!(again.json["current"]["config"]["sort"], "due");
    let view_uri = format!("/api/views/{}", saved.json["id"].as_str().unwrap());
    let changed = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "base_version": 1,
        "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);
    assert_eq!(changed.json["id"], saved.json["id"]);
    assert_eq!(changed.json["version"], 2);
    let mine = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(mine.json.as_array().unwrap().len(), 1);
    assert_eq!(mine.json[0]["config"], json!({ "view": "board" }));
    // 다른 사람에게는 보이지 않고 고치거나 지울 수도 없다.
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&admin), None).await.json,
        json!([])
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &view_uri,
            Some(&admin),
            Some(json!({ "request_id": new_request_id(), "base_version": 2, "config": {} }))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
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
        Some(json!({ "base_version": 2, "title": "항목", "description": "", "visibility": "private" })),
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
        Some(json!({ "base_version": 3, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(
        send(&app, Method::DELETE, &view_uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

/// `request` 요청 id로 보기를 하나 만들고 (보기 주소, 목록 주소, 보기)를 돌려준다.
async fn one_view(
    app: &axum::Router,
    user: &TestUser,
    key: &str,
    request: &str,
    config: Value,
) -> (String, String, Value) {
    let ws = create_workspace(app, user, key).await;
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    let item = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    let list_uri = format!("/api/items/{}/views", item.json["id"].as_str().unwrap());
    let created = send(
        app,
        Method::POST,
        &list_uri,
        Some(user),
        Some(json!({ "request_id": request, "name": "내 보기", "config": config })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let view_uri = format!("/api/views/{}", created.json["id"].as_str().unwrap());
    (view_uri, list_uri, created.json)
}

/// D-103(감사 P-26): 두 탭이 같은 보기를 읽은 뒤 탭 B가 먼저 저장하면, 탭 A가 읽은 판(1)으로 보낸 저장은 409와 최신
/// 보기를 받고 아무것도 바꾸지 않는다(마지막 저장 우선이 아니다). 고치기와 확인은 같은 트랜잭션이다.
#[tokio::test]
async fn a_view_saved_from_a_stale_tab_is_refused_with_the_latest() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "view-tabs").await;
    let (view_uri, list_uri, created) = one_view(
        &app,
        &me,
        "TABS",
        &new_request_id(),
        json!({ "view": "list", "sort": "due" }),
    )
    .await;
    let tab_b = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(json!({ "request_id": new_request_id(), "base_version": 1,
            "config": { "view": "board", "sort": "position" } })),
    )
    .await;
    assert_eq!(tab_b.status, StatusCode::OK, "{:?}", tab_b.json);
    assert_eq!(created["version"], 1);
    assert_eq!(tab_b.json["version"], 2);
    let tab_a = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(json!({ "request_id": new_request_id(), "base_version": 1,
            "config": { "view": "gantt", "sort": "due" } })),
    )
    .await;
    assert_eq!(tab_a.status, StatusCode::CONFLICT, "{:?}", tab_a.json);
    assert_eq!(tab_a.json["error"], "conflict");
    assert_eq!(tab_a.json["current"]["id"], created["id"]);
    assert_eq!(tab_a.json["current"]["version"], 2);
    assert_eq!(
        tab_a.json["current"]["config"],
        json!({ "view": "board", "sort": "position" })
    );
    let list = send(&app, Method::GET, &list_uri, Some(&me), None)
        .await
        .json;
    assert_eq!(list.as_array().unwrap().len(), 1, "{list:?}");
    assert_eq!(
        list[0]["config"],
        json!({ "view": "board", "sort": "position" })
    );
    assert_eq!(list[0]["version"], 2);
    // 최신 판으로 다시 보내면 저장된다.
    let again = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(json!({ "request_id": new_request_id(), "base_version": 2,
            "config": { "view": "gantt", "sort": "due" } })),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
    assert_eq!(again.json["version"], 3);
    assert_eq!(
        again.json["config"],
        json!({ "view": "gantt", "sort": "due" })
    );
}

/// D-103(감사 SD1): 응답을 잃은 저장을 같은 요청 id로 다시 보내면 이미 반영한 결과를 돌려주고 두 번 쓰지 않는다. 그
/// 뒤 다른 탭이 저장했으면(A 반영 → B 저장 → A 재시도) 그 재시도는 B를 덮지 않는다: 고치기가 남기는 요청 id는 마지막
/// 고치기의 것뿐이라 A의 id를 알아보지 못하고, A가 읽은 판(1)이 지금 판(3)과 달라 409다. 만들기 재시도도 같은 보기를
/// 돌려줄 뿐 새로 만들지 않는다.
#[tokio::test]
async fn a_lost_view_save_is_retried_without_overwriting_a_later_save() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "view-retry").await;
    let create_request = new_request_id();
    let (view_uri, list_uri, created) = one_view(
        &app,
        &me,
        "RETRY",
        &create_request,
        json!({ "view": "list" }),
    )
    .await;
    let a_request = new_request_id();
    let a_body =
        json!({ "request_id": a_request, "base_version": 1, "config": { "view": "board" } });

    let a = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(a_body.clone()),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{:?}", a.json);
    assert_eq!(a.json["version"], 2);
    // 응답을 잃었다고 보고 그대로 다시 보낸다: 반영한 결과(판 2), 두 번째 쓰기 없음.
    let retry = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(a_body.clone()),
    )
    .await;
    assert_eq!(retry.status, StatusCode::OK, "{:?}", retry.json);
    assert_eq!(retry.json["version"], 2);
    assert_eq!(retry.json["config"], json!({ "view": "board" }));

    let b_request = new_request_id();
    let b = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(json!({ "request_id": b_request, "base_version": 2,
            "config": { "view": "calendar" } })),
    )
    .await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.json);
    assert_eq!(b.json["version"], 3);
    let late = send(&app, Method::PUT, &view_uri, Some(&me), Some(a_body)).await;
    assert_eq!(late.status, StatusCode::CONFLICT, "{:?}", late.json);
    assert_eq!(
        late.json["current"]["config"],
        json!({ "view": "calendar" })
    );
    let list = send(&app, Method::GET, &list_uri, Some(&me), None)
        .await
        .json;
    assert_eq!(list[0]["config"], json!({ "view": "calendar" }), "{list:?}");
    assert_eq!(list[0]["version"], 3);

    // 처음 만들기를 같은 요청 id로 다시 보내도 같은 보기(지금 내용)이고 새 보기는 없다.
    let create_again = send(
        &app,
        Method::POST,
        &list_uri,
        Some(&me),
        Some(json!({ "request_id": create_request, "name": "내 보기", "config": { "view": "list" } })),
    )
    .await;
    assert_eq!(
        create_again.status,
        StatusCode::CREATED,
        "{:?}",
        create_again.json
    );
    assert_eq!(create_again.json["id"], created["id"]);
    assert_eq!(create_again.json["config"], json!({ "view": "calendar" }));
    // 남아 있는 요청 id(B의 고치기)를 다른 보기에 쓰면 받지 않는다(그 요청의 결과로 다른 보기를 돌려주지 않게).
    let other = send(
        &app,
        Method::POST,
        &list_uri,
        Some(&me),
        Some(json!({ "request_id": new_request_id(), "name": "둘째", "config": {} })),
    )
    .await;
    assert_eq!(other.status, StatusCode::CREATED, "{:?}", other.json);
    let reused = send(
        &app,
        Method::PUT,
        &format!("/api/views/{}", other.json["id"].as_str().unwrap()),
        Some(&me),
        Some(json!({ "request_id": b_request, "base_version": 1, "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(reused.status, StatusCode::BAD_REQUEST, "{:?}", reused.json);
    let list = send(&app, Method::GET, &list_uri, Some(&me), None)
        .await
        .json;
    assert_eq!(list.as_array().unwrap().len(), 2, "{list:?}");
}

/// D-103: 판 확인은 SQL 함수가 마지막으로 보장한다. 읽은 판을 NULL로 보내도(앱 역할로 직접 불러도) 확인을 건너뛰어
/// 덮어쓰지 않고 'conflict'다. 이미 반영한 요청 id의 재시도는 판을 보기 전에 그대로 'replayed'다.
#[tokio::test]
async fn a_null_base_version_does_not_skip_the_view_version_check() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "view-null").await;
    let (view_uri, list_uri, created) = one_view(
        &app,
        &me,
        "VNUL",
        &new_request_id(),
        json!({ "view": "list" }),
    )
    .await;
    let view: uuid::Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let saved_request = new_request_id();
    let saved = send(
        &app,
        Method::PUT,
        &view_uri,
        Some(&me),
        Some(json!({ "request_id": saved_request, "base_version": 1, "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);

    let mut tx = state.db.begin_as(me.id).await.unwrap();
    let update = |request: uuid::Uuid| {
        sqlx::query_scalar::<_, String>("SELECT app_update_view($1, $2, NULL::integer, $3)")
            .bind(request)
            .bind(view)
            .bind(json!({ "view": "gantt" }))
    };
    let stale = update(uuid::Uuid::now_v7())
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(stale, "conflict");
    let replay = update(saved_request.parse().unwrap())
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(replay, "replayed");
    tx.commit().await.unwrap();
    let list = send(&app, Method::GET, &list_uri, Some(&me), None)
        .await
        .json;
    assert_eq!(list[0]["config"], json!({ "view": "board" }), "{list:?}");
    assert_eq!(list[0]["version"], 2);
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

/// 표시 설정(D-81, D-109): 언어·주 시작·시각 표시·화면 밝기를 따로 저장하고, 보내지 않은 칸은 그대로 둔다. 본인만 보고
/// 모르는 값은 400. 화면 밝기는 처음부터 `system`이고 빈 값으로 지울 수 없다.
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
                v["theme"].as_str().unwrap().to_owned(),
            ]
        }
    };
    // 처음에는 아무것도 고르지 않았다(화면이 브라우저·로케일로 제안).
    assert_eq!(fields(me.clone()).await, ["", "", "", "", "system"]);
    assert_eq!(
        put(json!({ "time_zone": "Asia/Seoul", "locale": "en", "week_start": "sun", "hour_cycle": "h12", "theme": "dark" })).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        fields(me.clone()).await,
        ["Asia/Seoul", "en", "sun", "h12", "dark"]
    );
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
        ["America/New_York", "en", "mon", "h12", "dark"]
    );
    for bad in [
        json!({ "theme": "auto" }),
        json!({ "theme": "" }),
        json!({ "locale": "fr" }),
        json!({ "week_start": "sat" }),
        json!({ "hour_cycle": "h24" }),
        json!({ "time_zone": "Mars/Olympus" }),
    ] {
        assert_eq!(put(bad).await, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        fields(me.clone()).await,
        ["America/New_York", "en", "mon", "h12", "dark"]
    );
    assert_eq!(fields(other.clone()).await, ["", "", "", "", "system"]);
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
    // D-112·D-51: 초기 관리자는 서버 관리자이자 첫 조직의 관리자이고 첫 워크스페이스는 그 조직 안에 있다. 다시 불러도
    // 조직·워크스페이스가 늘지 않고, 먼저 로그인한 다른 사람은 아무 권한도 얻지 않는다.
    {
        use sqlx::Connection as _;
        let url = fresh.db.migration_url.as_ref().unwrap().expose().to_owned();
        let mut owner = sqlx::PgConnection::connect(&url).await.unwrap();
        let (is_server_admin, organizations, admin_of, in_organization): (bool, i64, i64, bool) =
            sqlx::query_as(
                "SELECT EXISTS (SELECT 1 FROM fvoci.server_admins WHERE user_id = $1),
                        (SELECT count(*) FROM fvoci.organizations),
                        (SELECT count(*) FROM fvoci.organization_members WHERE user_id = $1 AND role = 'admin'),
                        EXISTS (SELECT 1 FROM fvoci.workspaces w
                                JOIN fvoci.organization_members m
                                  ON m.organization_id = w.organization_id AND m.user_id = $1
                                WHERE w.id = $2)",
            )
            .bind(user.0)
            .bind(ws.unwrap())
            .fetch_one(&mut owner)
            .await
            .unwrap();
        assert!(is_server_admin, "the initial admin is a server admin");
        assert_eq!(
            organizations, 1,
            "one organization, also after the second claim"
        );
        assert_eq!(admin_of, 1, "organization admin of the first organization");
        assert!(
            in_organization,
            "the first workspace belongs to that organization"
        );
        let stranger_grants: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM fvoci.server_admins WHERE user_id = $1)
                  + (SELECT count(*) FROM fvoci.organization_members WHERE user_id = $1)",
        )
        .bind(stranger_id.0)
        .fetch_one(&mut owner)
        .await
        .unwrap();
        assert_eq!(stranger_grants, 0, "someone else gets nothing");
        owner.close().await.unwrap();
    }
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
