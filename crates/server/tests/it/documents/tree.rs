#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 문서 트리: 부모·순서, 같은 자리 규칙, 순환·깊이 한도, 숨은 부모, 다른 자리로 이동 (D-27).

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_doc, create_workspace, login, new_request_id, send};
use fvoci_server::app::build_state;
use serde_json::{Value, json};

async fn child(app: &Router, user: &TestUser, ws: &str, parent: &Value, title: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title, "parent_id": parent["id"] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

async fn set_parent(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    parent: Option<&Value>,
    before: Option<&Value>,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/documents/{}/parent", doc["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "parent_id": parent.map(|p| p["id"].clone()), "before_id": before.map(|b| b["id"].clone()) })),
    )
    .await;
    (r.status, r.json)
}

/// 위키 목록을 (제목, 부모 제목) 순서대로.
async fn tree(app: &Router, user: &TestUser, ws: &str) -> Vec<(String, Option<String>)> {
    let r = send(
        app,
        Method::GET,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(user),
        None,
    )
    .await;
    let list = r.json.as_array().unwrap().clone();
    // 화면과 같은 방식: 부모별로 묶되 서버 순서를 지키고, 깊이 우선으로 편다.
    fn walk(list: &[Value], parent: &Value, out: &mut Vec<(String, Option<String>)>) {
        for d in list.iter().filter(|d| &d["parent_id"] == parent) {
            let parent_title = list
                .iter()
                .find(|p| &p["id"] == parent)
                .map(|p| p["title"].as_str().unwrap().to_owned());
            out.push((d["title"].as_str().unwrap().to_owned(), parent_title));
            walk(list, &d["id"], out);
        }
    }
    let mut out = Vec::new();
    walk(&list, &Value::Null, &mut out);
    out
}

#[tokio::test]
async fn children_are_created_ordered_and_moved_within_the_tree() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "트리").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let guide = create_doc(&app, &admin, &ws, "안내").await;
    let notes = create_doc(&app, &admin, &ws, "기록").await;

    // 같은 request_id로 두 번 만들어도 하나, 부모 아래.
    let rid = new_request_id();
    let body = json!({ "request_id": rid, "title": "설치", "parent_id": guide["id"] });
    let uri = format!("/api/workspaces/{ws}/wiki/documents");
    let a = send(&app, Method::POST, &uri, Some(&admin), Some(body.clone())).await;
    let b = send(&app, Method::POST, &uri, Some(&admin), Some(body)).await;
    assert_eq!(a.json["id"], b.json["id"]);
    assert_eq!(a.json["parent_id"], guide["id"]);
    let install = a.json;
    let usage = child(&app, &admin, &ws, &guide, "사용").await;

    let s = |t: &str| t.to_owned();
    assert_eq!(
        tree(&app, &reader, &ws).await,
        vec![
            (s("안내"), None),
            (s("설치"), Some(s("안내"))),
            (s("사용"), Some(s("안내"))),
            (s("기록"), None),
        ]
    );

    // 순서 바꾸기: "사용"을 "설치" 앞으로. 리비전은 늘지 않는다.
    let (status, moved) = set_parent(&app, &admin, &usage, Some(&guide), Some(&install)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["revision"], usage["revision"]);
    let order: Vec<String> = tree(&app, &admin, &ws)
        .await
        .into_iter()
        .filter(|(_, p)| p.is_some())
        .map(|(t, _)| t)
        .collect();
    assert_eq!(order, vec!["사용", "설치"]);

    // 순환 금지: 자기 자신, 자기 하위 아래로.
    assert_eq!(
        set_parent(&app, &admin, &guide, Some(&guide), None).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        set_parent(&app, &admin, &guide, Some(&install), None)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // before는 새 부모 아래의 형제여야 한다.
    assert_eq!(
        set_parent(&app, &admin, &notes, None, Some(&install))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // 독자는 바꿀 수 없다.
    assert_eq!(
        set_parent(&app, &reader, &notes, Some(&guide), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // 최상위로 꺼내기, 감사 기록.
    let (status, _) = set_parent(&app, &admin, &install, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(tree(&app, &admin, &ws).await.contains(&(s("설치"), None)));
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE target_id = $1::uuid AND action = 'reparent'",
    )
    .bind(install["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(
        count, 2,
        "placed under 안내 at creation, then taken to the top level"
    );
}

#[tokio::test]
async fn parents_must_be_in_the_same_place_and_hidden_parents_are_not_revealed() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "같은 자리").await;
    let other = create_workspace(&app, &admin, "다른 자리").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
    let here = create_doc(&app, &admin, &ws, "여기").await;
    let there = create_doc(&app, &admin, &other, "저기").await;
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "노트" })),
    )
    .await
    .json;
    assert_eq!(
        set_parent(&app, &admin, &here, Some(&there), None).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        set_parent(&app, &admin, &here, Some(&note), None).await.0,
        StatusCode::BAD_REQUEST
    );
    // 볼 수 없는 부모는 없는 것과 같다.
    let stranger = login(&state, "stranger").await;
    let theirs = create_workspace(&app, &stranger, "남의 공간").await;
    let hidden = create_doc(&app, &stranger, &theirs, "남의 문서").await;
    assert_eq!(
        set_parent(&app, &admin, &here, Some(&hidden), None).await.0,
        StatusCode::NOT_FOUND
    );

    // 비공개 부모 아래의 공개 자식: 동료에게는 최상위로 보이고 부모 id를 받지 않는다.
    let private_parent = create_doc(&app, &writer, &ws, "비공개 부모").await;
    send(
        &app,
        Method::PATCH,
        &format!(
            "/api/documents/{}/settings",
            private_parent["id"].as_str().unwrap()
        ),
        Some(&admin),
        Some(
            json!({ "visibility": "private", "previous_visibility": private_parent["visibility"] }),
        ),
    )
    .await;
    let visible_child = child(&app, &writer, &ws, &private_parent, "공개 자식").await;
    let peer_view = tree(&app, &peer, &ws).await;
    assert!(
        peer_view.contains(&("공개 자식".to_owned(), None)),
        "{peer_view:?}"
    );
    assert!(!peer_view.iter().any(|(t, _)| t == "비공개 부모"));
    let (_, doc) = (
        StatusCode::OK,
        send(
            &app,
            Method::GET,
            &format!("/api/documents/{}", visible_child["id"].as_str().unwrap()),
            Some(&peer),
            None,
        )
        .await
        .json,
    );
    assert_eq!(doc["parent_id"], Value::Null);
    assert!(
        tree(&app, &admin, &ws)
            .await
            .contains(&("공개 자식".to_owned(), Some("비공개 부모".to_owned())))
    );
    // 동료에게 최상위로 보이는 그 자식 바로 앞에 놓을 수 있다(보이는 부모 기준으로 형제 판단).
    let (status, placed) = set_parent(&app, &peer, &here, None, Some(&visible_child)).await;
    assert_eq!(status, StatusCode::OK, "{placed:?}");
    let order: Vec<String> = tree(&app, &peer, &ws)
        .await
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    let at = |t: &str| order.iter().position(|x| x == t).unwrap();
    assert_eq!(at("여기") + 1, at("공개 자식"), "{order:?}");
}

#[tokio::test]
async fn moving_a_parent_elsewhere_leaves_its_children_on_the_old_parent() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "출발").await;
    let dst = create_workspace(&app, &admin, "도착").await;
    let top = create_doc(&app, &admin, &ws, "상위").await;
    let section = child(&app, &admin, &ws, &top, "구역").await;
    child(&app, &admin, &ws, &section, "구역 안").await;

    let uri = format!(
        "/api/documents/{}/placement",
        section["id"].as_str().unwrap()
    );
    let preview = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "target_workspace_id": dst })),
    )
    .await;
    assert_eq!(preview.json["children_left_behind"], 1);
    // D-114: 사용자 id·등급 변화 전체를 확인한 뒤 이동한다.
    let ack = json!({ "changes": preview.json["changes"] });
    let moved = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "target_workspace_id": dst, "acknowledged": ack })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert_eq!(moved.json["document"]["parent_id"], Value::Null);
    let s = |t: &str| t.to_owned();
    assert_eq!(
        tree(&app, &admin, &ws).await,
        vec![(s("상위"), None), (s("구역 안"), Some(s("상위")))]
    );
    assert_eq!(tree(&app, &admin, &dst).await, vec![(s("구역"), None)]);
}

#[tokio::test]
async fn depth_is_limited_and_concurrent_opposite_moves_cannot_form_a_cycle() {
    let state = build_state(common::config_with(&[("FVOCI_MAX_DOCUMENT_DEPTH", "3")])).unwrap();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "깊이").await;
    let one = create_doc(&app, &admin, &ws, "1").await;
    let two = child(&app, &admin, &ws, &one, "2").await;
    let three = child(&app, &admin, &ws, &two, "3").await;
    let r = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "4", "parent_id": three["id"] })),
    )
    .await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "depth 4 exceeds the limit of 3"
    );
    // 높이 2인 하위 트리를 깊이 2 아래로 옮기면 4가 되므로 거부한다.
    let other = create_doc(&app, &admin, &ws, "다른 1").await;
    child(&app, &admin, &ws, &other, "다른 2").await;
    assert_eq!(
        set_parent(&app, &admin, &other, Some(&two), None).await.0,
        StatusCode::BAD_REQUEST
    );

    // 서로를 부모로 삼는 두 변경을 동시에 보내도 하나만 성공하고 순환이 생기지 않는다.
    let a = create_doc(&app, &admin, &ws, "A").await;
    let b = create_doc(&app, &admin, &ws, "B").await;
    for _ in 0..5 {
        set_parent(&app, &admin, &a, None, None).await;
        set_parent(&app, &admin, &b, None, None).await;
        let (ra, rb) = tokio::join!(
            set_parent(&app, &admin, &a, Some(&b), None),
            set_parent(&app, &admin, &b, Some(&a), None)
        );
        let ok = [ra.0, rb.0]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count();
        assert_eq!(ok, 1, "exactly one succeeds: {:?} {:?}", ra, rb);
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        let cyclic: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM documents x JOIN documents y ON x.parent_id = y.id AND y.parent_id = x.id)",
        )
        .fetch_one(tx.conn())
        .await
        .unwrap();
        assert!(!cyclic);
    }
}

/// 검토 반영: 재시도된 생성은 문서를 옮기지 않고, 같은 틈에 반복해 끼워도 순서가 유지되며,
/// 이동으로 남는 자식은 원래 순서대로 그 자리에 남는다.
#[tokio::test]
async fn retries_precision_and_left_behind_order() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검토 반영").await;
    let dst = create_workspace(&app, &admin, "도착지").await;
    let guide = create_doc(&app, &admin, &ws, "안내").await;

    // 같은 request_id 재시도: 사용자가 그사이 최상위로 꺼낸 문서를 다시 부모 아래로 넣지 않는다.
    let rid = new_request_id();
    let body = json!({ "request_id": rid, "title": "설치", "parent_id": guide["id"] });
    let uri = format!("/api/workspaces/{ws}/wiki/documents");
    let created = send(&app, Method::POST, &uri, Some(&admin), Some(body.clone()))
        .await
        .json;
    set_parent(&app, &admin, &created, None, None).await;
    let replay = send(&app, Method::POST, &uri, Some(&admin), Some(body)).await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.json["id"], created["id"]);
    assert_eq!(
        replay.json["parent_id"],
        Value::Null,
        "the retry must not move it back"
    );

    // 같은 형제 앞에 70번 끼워 넣어도(가운데 값을 계속 반으로) 순서가 섞이지 않는다.
    let anchor = child(&app, &admin, &ws, &guide, "기준").await;
    let mut expected = Vec::new();
    for i in 0..70 {
        let d = child(&app, &admin, &ws, &guide, &format!("끼움 {i}")).await;
        let (status, _) = set_parent(&app, &admin, &d, Some(&guide), Some(&anchor)).await;
        assert_eq!(status, StatusCode::OK);
        expected.push(format!("끼움 {i}"));
    }
    expected.push("기준".to_owned());
    let under: Vec<String> = tree(&app, &admin, &ws)
        .await
        .into_iter()
        .filter(|(_, p)| p.as_deref() == Some("안내"))
        .map(|(t, _)| t)
        .collect();
    assert_eq!(under, expected);

    // 자식 둘을 둔 문서를 다른 워크스페이스로 옮기면, 자식은 그 문서가 있던 자리에 원래 순서대로 남는다.
    let a = create_doc(&app, &admin, &ws, "가").await;
    let section = create_doc(&app, &admin, &ws, "구역").await;
    let b = create_doc(&app, &admin, &ws, "나").await;
    child(&app, &admin, &ws, &section, "첫째").await;
    child(&app, &admin, &ws, &section, "둘째").await;
    let move_uri = format!(
        "/api/documents/{}/placement",
        section["id"].as_str().unwrap()
    );
    let preview = send(
        &app,
        Method::PUT,
        &move_uri,
        Some(&admin),
        Some(json!({ "target_workspace_id": dst })),
    )
    .await;
    assert_eq!(preview.json["children_left_behind"], 2);
    let ack = json!({ "changes": preview.json["changes"] });
    let moved = send(
        &app,
        Method::PUT,
        &move_uri,
        Some(&admin),
        Some(json!({ "target_workspace_id": dst, "acknowledged": ack })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let roots: Vec<String> = tree(&app, &admin, &ws)
        .await
        .into_iter()
        .filter(|(_, p)| p.is_none())
        .map(|(t, _)| t)
        .collect();
    let pos = |t: &str| roots.iter().position(|x| x == t).unwrap();
    assert!(
        pos("가") < pos("첫째") && pos("첫째") < pos("둘째") && pos("둘째") < pos("나"),
        "{roots:?}"
    );
    let _ = (a, b);
}

/// 검토 반영: 부모 이동과 자식 배치, 다른 문서가 들어오는 이동과 부모 지정 생성이 동시에 일어나도 교착(500) 없이 끝난다.
#[tokio::test]
async fn concurrent_moves_creates_and_reparents_do_not_deadlock() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "동시 출발").await;
    let dst = create_workspace(&app, &admin, "동시 도착").await;
    let anchor = create_doc(&app, &admin, &dst, "도착 기준").await;
    for i in 0..5 {
        let parent = create_doc(&app, &admin, &ws, &format!("부모 {i}")).await;
        let kid = child(&app, &admin, &ws, &parent, &format!("자식 {i}")).await;
        let other = create_doc(&app, &admin, &ws, &format!("다른 {i}")).await;
        let move_uri = format!(
            "/api/documents/{}/placement",
            parent["id"].as_str().unwrap()
        );
        let preview = send(
            &app,
            Method::PUT,
            &move_uri,
            Some(&admin),
            Some(json!({ "target_workspace_id": dst })),
        )
        .await;
        let ack = json!({ "changes": preview.json["changes"] });
        let create_uri = format!("/api/workspaces/{dst}/wiki/documents");
        let (moved, reparented, created) = tokio::join!(
            send(
                &app,
                Method::PUT,
                &move_uri,
                Some(&admin),
                Some(json!({ "target_workspace_id": dst, "acknowledged": ack }))
            ),
            set_parent(&app, &admin, &kid, None, Some(&other)),
            send(
                &app,
                Method::POST,
                &create_uri,
                Some(&admin),
                Some(
                    json!({ "request_id": new_request_id(), "title": format!("새 {i}"), "parent_id": anchor["id"] })
                )
            )
        );
        assert_ne!(
            moved.status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{:?}",
            moved.json
        );
        assert_ne!(
            reparented.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{:?}",
            reparented.1
        );
        assert_ne!(
            created.status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{:?}",
            created.json
        );
    }
}
