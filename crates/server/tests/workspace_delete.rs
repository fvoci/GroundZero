#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 워크스페이스 삭제·되살리기·영구 삭제 (D-51, D-41).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, fixture_sql, login, login_with_email,
    new_request_id, send, unique_email,
};
use serde_json::{Value, json};

async fn ids(app: &Router, user: &TestUser) -> Vec<String> {
    send(app, Method::GET, "/api/workspaces", Some(user), None)
        .await
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn deleted(app: &Router, user: &TestUser) -> Value {
    let r = send(
        app,
        Method::GET,
        "/api/me/deleted-workspaces",
        Some(user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

async fn delete(app: &Router, user: &TestUser, ws: &str, name: &str) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}"),
        Some(user),
        Some(json!({ "name": name })),
    )
    .await;
    (r.status, r.json)
}

/// 모듈·항목·업무(하위 포함)·일정·항목 문서·휴지통 묶음·그룹까지 갖춘 워크스페이스 내용.
async fn fill(app: &Router, owner: &TestUser, ws: &str) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "WSD", "key": "WSD" })),
    )
    .await;
    let item = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let item_id = item["id"].as_str().unwrap();
    let task = send(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "title": "업무" })),
    )
    .await
    .json;
    let sub = send(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "title": "하위", "parent_id": task["id"] })),
    )
    .await;
    assert_eq!(sub.status, StatusCode::CREATED, "{:?}", sub.json);
    let event = send(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/events"),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "kind": "meeting", "title": "회의", "description": "",
                     "all_day": true, "start_date": "2026-10-20", "end_date": "2026-10-20",
                     "starts_at": null, "ends_at": null, "timezone": "Asia/Seoul" })),
    )
    .await;
    assert_eq!(event.status, StatusCode::CREATED, "{:?}", event.json);
    let inner = send(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(owner),
        Some(json!({ "request_id": new_request_id(), "title": "항목 문서" })),
    )
    .await;
    assert_eq!(inner.status, StatusCode::CREATED, "{:?}", inner.json);
    let trashed = create_doc(app, owner, ws, "휴지통 문서").await;
    let r = send(
        app,
        Method::DELETE,
        &format!("/api/documents/{}", trashed["id"].as_str().unwrap()),
        Some(owner),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let group = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(owner),
        Some(json!({ "name": "팀" })),
    )
    .await;
    assert_eq!(group.status, StatusCode::CREATED, "{:?}", group.json);
    task
}

#[tokio::test]
async fn only_the_owner_deletes_and_everyone_loses_access_until_restored() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "지울 공간").await;
    let (admin, _) = add_member(&app, &state, &owner, &ws, "admin", "admin").await;
    let (writer, _) = add_member(&app, &state, &owner, &ws, "member", "member").await;
    let doc = create_doc(&app, &writer, &ws, "회의록").await;
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    // 밖의 사람에게 공유하고 공개 링크도 만든다.
    let outsider_email = unique_email("outsider");
    let outsider = login_with_email(&state, "outsider", &outsider_email, true).await;
    // D-114: 공간 밖 공유 대상도 같은 조직 멤버여야 한다.
    let organization = common::organization_of_workspace(&ws).await;
    common::join_organization(organization, &outsider, "member").await;
    let shared = send(
        &app,
        Method::POST,
        &format!("{doc_uri}/shares"),
        Some(&owner),
        Some(json!({ "email": outsider_email, "permission": "read", "organization_id": organization })),
    )
    .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    let link = send(
        &app,
        Method::POST,
        &format!("{doc_uri}/public-links"),
        Some(&owner),
        Some(json!({})),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED, "{:?}", link.json);
    let token = link.json["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    // 열린 초대. 받는 사람은 조직 멤버여야 수락할 수 있다(D-114).
    let invitee_email = unique_email("invitee");
    let invitee = login_with_email(&state, "invitee", &invitee_email, true).await;
    common::join_organization(
        common::organization_of_workspace(&ws).await,
        &invitee,
        "member",
    )
    .await;
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&owner),
        Some(json!({ "email": invitee_email, "role": "readonly" })),
    )
    .await;
    assert_eq!(inv.status, StatusCode::CREATED, "{:?}", inv.json);
    let inv_id = inv.json["id"].as_str().unwrap().to_owned();

    // 소유자만, 이름을 맞게 입력해야 한다.
    assert_eq!(
        delete(&app, &admin, &ws, "지울 공간").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        delete(&app, &outsider, &ws, "지울 공간").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&app, &owner, &ws, "지울공간").await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, body) = delete(&app, &owner, &ws, " 지울 공간 ").await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(body["expires_at"].is_string());

    // 곧바로 모두에게 없는 것과 같다(멤버·공유·공개 링크·초대).
    for user in [&owner, &admin, &writer] {
        assert!(!ids(&app, user).await.contains(&ws));
        assert_eq!(
            send(&app, Method::GET, &doc_uri, Some(user), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        send(&app, Method::GET, &doc_uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/me/shared?organization_id={organization}"),
            Some(&outsider),
            None
        )
        .await
        .json,
        json!([])
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/public/{token}"),
            None,
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/invitations/{inv_id}/accept"),
            Some(&invitee),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, "/api/invitations", Some(&invitee), None)
            .await
            .json,
        json!([])
    );
    let search = send(
        &app,
        Method::GET,
        &format!("/api/search?q=회의록&organization_id={organization}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(search.json, json!([]));
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/workspaces/{ws}/members"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 지운 워크스페이스는 소유자에게만 보이고, 되살리면 같은 번호로 돌아온다.
    let mine = deleted(&app, &owner).await;
    assert_eq!(mine[0]["id"], ws);
    assert_eq!(mine[0]["expired"], false);
    assert_eq!(deleted(&app, &admin).await, json!([]));
    let restore = format!("/api/me/deleted-workspaces/{ws}/restore");
    assert_eq!(
        send(&app, Method::POST, &restore, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::POST, &restore, Some(&owner), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let back = send(&app, Method::GET, &doc_uri, Some(&writer), None).await;
    assert_eq!(back.status, StatusCode::OK);
    assert_eq!(back.json["reference"], doc["reference"]);
    assert_eq!(
        send(&app, Method::GET, &doc_uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(deleted(&app, &owner).await, json!([]));
    // 지울 때 닫은 초대는 되살리면 다시 열린다.
    let invited = send(&app, Method::GET, "/api/invitations", Some(&invitee), None).await;
    assert_eq!(invited.json[0]["id"], inv_id.as_str(), "{:?}", invited.json);
    let accepted = send(
        &app,
        Method::POST,
        &format!("/api/invitations/{inv_id}/accept"),
        Some(&invitee),
        None,
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.json);
    assert!(ids(&app, &invitee).await.contains(&ws));

    // 소유권 인계가 진행 중이면 먼저 취소해야 한다.
    let offer = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/ownership"),
        Some(&owner),
        Some(json!({ "to_user": admin.id.0 })),
    )
    .await;
    assert!(offer.status.is_success(), "{:?}", offer.json);
    assert_eq!(
        delete(&app, &owner, &ws, "지울 공간").await.0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn purge_removes_everything_but_personal_time_and_expiry_purges_in_the_worker() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "영구 삭제").await;
    let (writer, _) = add_member(&app, &state, &owner, &ws, "member", "member").await;
    let task = fill(&app, &owner, &ws).await;
    let entry = send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&owner),
        Some(
            json!({ "request_id": new_request_id(), "started_at": "2026-10-20T09:00:00+09:00",
                     "ended_at": "2026-10-20T10:00:00+09:00", "paused_seconds": 0, "kind": "work",
                     "memo": "남는 기록", "target": { "kind": "task", "id": task["id"] } }),
        ),
    )
    .await;
    assert_eq!(entry.status, StatusCode::CREATED, "{:?}", entry.json);

    assert_eq!(
        delete(&app, &owner, &ws, "영구 삭제").await.0,
        StatusCode::OK
    );
    let purge = format!("/api/me/deleted-workspaces/{ws}");
    assert_eq!(
        send(&app, Method::DELETE, &purge, Some(&writer), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = send(&app, Method::DELETE, &purge, Some(&owner), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
    assert_eq!(deleted(&app, &owner).await, json!([]));
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{purge}/restore"),
            Some(&owner),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // 개인 시간 기록은 남는다.
    let entries = send(
        &app,
        Method::GET,
        "/api/me/time-entries?from=2026-10-20&to=2026-10-21&tz=Asia%2FSeoul",
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(entries.status, StatusCode::OK, "{:?}", entries.json);
    assert!(
        entries.json.to_string().contains("남는 기록"),
        "{:?}",
        entries.json
    );

    // 보존 기간이 지나면 되살릴 수 없고, 작업 프로세스가 영구 삭제한다.
    let ws2 = create_workspace(&app, &owner, "만료").await;
    fill(&app, &owner, &ws2).await;
    assert_eq!(delete(&app, &owner, &ws2, "만료").await.0, StatusCode::OK);
    fixture_sql(
        "UPDATE workspaces SET deleted_at = now() - interval '31 days' WHERE id = $1::uuid",
        &ws2,
    )
    .await;
    assert_eq!(deleted(&app, &owner).await[0]["expired"], true);
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/me/deleted-workspaces/{ws2}/restore"),
            Some(&owner),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    let report = fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    assert!(report.purged >= 1);
    assert_eq!(deleted(&app, &owner).await, json!([]));
}

/// 감사 5번: 휴지통에 든 모듈은 휴지통 묶음을 가리킨다. 워크스페이스를 영구 삭제하면 그 묶음을 지우기 전에 모듈의
/// 참조가 풀려야 한다(전에는 외래 키 위반으로 영구 삭제가 실패했다).
#[tokio::test]
async fn purge_succeeds_with_a_trashed_module() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "모듈 휴지통 영구 삭제").await;
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&owner),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "버림", "key": "TRM" })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/modules/{}", m.json["id"].as_str().unwrap()),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        delete(&app, &owner, &ws, "모듈 휴지통 영구 삭제").await.0,
        StatusCode::OK
    );
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/me/deleted-workspaces/{ws}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
}
