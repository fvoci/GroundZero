#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
mod common;
mod sql_support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sql_support::*;
use uuid::Uuid;

struct RunningServer(tokio::task::JoinHandle<()>);

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// Send over a real loopback socket to the production router and session middleware.
async fn send(
    app: &axum::Router,
    method: Method,
    path: &str,
    user: Option<&common::TestUser>,
    body: Option<Value>,
) -> common::Reply {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = app.clone();
    let _server = RunningServer(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let mut request = reqwest::Client::new()
        .request(method, format!("http://{address}{path}"))
        .timeout(std::time::Duration::from_secs(10))
        .header("origin", common::PUBLIC_URL);
    if let Some(user) = user {
        request = request
            .header("cookie", &user.cookie)
            .header("x-fvoci-expected-user", user.id.0.to_string());
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let bytes = response.bytes().await.unwrap();
    common::Reply {
        status,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

async fn assert_app_role(w: &World) {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let role: (String, bool) = sqlx::query_as(
        "SELECT current_user::text,rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(role, ("fvoci_app".to_owned(), false));
    tx.commit().await.unwrap();
}

fn save() -> Value {
    json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"denied edit","body":common::doc_body("denied")})
}

#[tokio::test]
async fn administrators_do_not_gain_content_access() {
    let w = World::new().await;
    assert_app_role(&w).await;
    let doc = w.doc().await;
    let task = w.task().await;
    let file = attachment(&w, doc).await;
    let server = common::login(&w.state, "server outside organization").await;
    common::make_server_admin(&server).await;
    let org_admin = common::login(&w.state, "organization nonmember admin").await;
    common::join_organization(w.org, &org_admin, "admin").await;
    let app = common::app(&w.state);
    // The baseline has no workspace GET detail. Its registered module-list route reads the
    // workspace's contents and must hide the workspace from nonmembers.
    let paths = [
        format!("/api/workspaces/{}/modules", w.ws),
        format!("/api/documents/{doc}"),
        format!("/api/items/{}", w.item),
        format!("/api/tasks/{task}"),
        format!("/api/files/{file}"),
        format!("/api/search?q=SQL&workspace_id={}", w.ws),
    ];
    let mut failures = Vec::new();
    for path in &paths {
        let owner = send(&app, Method::GET, path, Some(&w.owner), None).await;
        assert_eq!(
            owner.status,
            StatusCode::OK,
            "positive control {path}: {}",
            owner.json
        );
        for actor in [&server, &org_admin] {
            let r = send(&app, Method::GET, path, Some(actor), None).await;
            if r.status != StatusCode::NOT_FOUND {
                failures.push(format!("{} {path}: {} {}", actor.id.0, r.status, r.json));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    rows(
        &w.state,
        &org_admin,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let joined = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        Some(&org_admin),
        None,
    )
    .await;
    assert_eq!(joined.status, StatusCode::OK, "{}", joined.json);
}

#[tokio::test]
async fn server_admin_outside_organization_cannot_read_its_notifications() {
    let w = World::new().await;
    assert_app_role(&w).await;
    let server = common::login(&w.state, "server notifications outsider").await;
    common::make_server_admin(&server).await;
    let r = send(
        &common::app(&w.state),
        Method::GET,
        &format!("/api/me/notifications?organization_id={}", w.org),
        Some(&server),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.json);
}

#[tokio::test]
async fn readonly_writes_are_forbidden_and_archived_writes_conflict() {
    let w = World::new().await;
    assert_app_role(&w).await;
    let reader = w.member("readonly").await;
    let doc = w.doc().await;
    let task = w.task().await;
    let file = attachment(&w, doc).await;
    let requests = [
        (Method::PUT, format!("/api/documents/{doc}"), save()),
        (
            Method::POST,
            "/api/comments".to_owned(),
            json!({"request_id":Uuid::now_v7(),"document_id":doc,"body":"denied comment"}),
        ),
        (
            Method::PUT,
            format!("/api/tasks/{task}"),
            json!({"base_version":1,"title":"denied task","description":"","status":"todo","priority":"none","tags":[],"assignee_ids":[],"milestone":false}),
        ),
        (
            Method::PUT,
            format!("/api/files/{file}"),
            json!({"name":"denied.txt"}),
        ),
    ];
    let app = common::app(&w.state);
    let mut failures = Vec::new();
    for archived in [false, true] {
        if archived {
            fixture(
                "UPDATE fvoci.items SET archived_at=now() WHERE id=$1",
                &[w.item],
            )
            .await;
        }
        let expected = if archived {
            StatusCode::CONFLICT
        } else {
            StatusCode::FORBIDDEN
        };
        for (method, path, body) in &requests {
            let r = send(
                &app,
                method.clone(),
                path,
                Some(&reader),
                Some(body.clone()),
            )
            .await;
            if r.status != expected {
                failures.push(format!(
                    "archived={archived} {method} {path}: expected {expected}, got {} {}",
                    r.status, r.json
                ));
            }
        }
    }
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT revision,title FROM fvoci.documents WHERE id=$1",
            &[doc]
        )
        .await,
        json!({"revision":1,"title":"SQL document"})
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT version,title FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await,
        json!({"version":1,"title":"SQL task"})
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT name FROM fvoci.files WHERE id=$1",
            &[file]
        )
        .await["name"],
        "fixture.txt"
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.comments WHERE document_id=$1",
            &[doc]
        )
        .await["n"],
        0
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn readonly_status_definition_is_forbidden_and_archive_conflicts() {
    let w = World::new().await;
    let reader = w.member("readonly").await;
    let app = common::app(&w.state);
    let path = format!("/api/items/{}/statuses", w.item);
    let mut failures = Vec::new();
    // Worker B's status-definition API is absent at baseline d8670d0. Keep the required HTTP
    // expectation visible as a failure until that route exists; never substitute a SQL call.
    for archived in [false, true] {
        if archived {
            fixture(
                "UPDATE fvoci.items SET archived_at=now() WHERE id=$1",
                &[w.item],
            )
            .await;
        }
        let r = send(
            &app,
            Method::POST,
            &path,
            Some(&reader),
            Some(json!({"name":"denied","color":"","category":"doing"})),
        )
        .await;
        let expected = if archived {
            StatusCode::CONFLICT
        } else {
            StatusCode::FORBIDDEN
        };
        if r.status != expected {
            failures.push(format!(
                "archived={archived}: expected {expected}, got {} {}",
                r.status, r.json
            ));
        }
    }
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.task_statuses WHERE item_id=$1",
            &[w.item]
        )
        .await["n"],
        3
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn sharing_and_moving_cannot_cross_organization_boundaries() {
    let w = World::new().await;
    assert_app_role(&w).await;
    let doc = w.doc().await;
    let email = common::unique_email("foreign");
    let foreign = common::login_with_email(&w.state, "foreign", &email, true).await;
    let foreign_org = common::organization_for(&foreign).await;
    common::join_organization(foreign_org, &w.owner, "member").await;
    let destination = workspace(&w.state, &w.owner, foreign_org).await;
    let app = common::app(&w.state);
    let shared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc}/shares"),
        Some(&w.owner),
        Some(json!({"organization_id":w.org,"email":email,"permission":"read"})),
    )
    .await;
    assert_eq!(shared.status, StatusCode::BAD_REQUEST, "{}", shared.json);
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc}/move/preview"),
        Some(&w.owner),
        Some(json!({"target_workspace_id":destination})),
    )
    .await;
    // Preview has no acknowledgement requirement: its 409 proves an organization boundary
    // denial, rather than the move endpoint's separate "acknowledgement missing" conflict.
    assert_eq!(preview.status, StatusCode::CONFLICT, "{}", preview.json);
    let moved = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc}/move"),
        Some(&w.owner),
        Some(json!({"target_workspace_id":destination})),
    )
    .await;
    assert_eq!(moved.status, StatusCode::CONFLICT, "{}", moved.json);
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT workspace_id FROM fvoci.documents WHERE id=$1",
            &[doc]
        )
        .await["workspace_id"],
        w.ws.to_string()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1",
            &[doc]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn individual_read_share_reaches_a_nonmember_but_does_not_allow_writes() {
    let w = World::new().await;
    assert_app_role(&w).await;
    let doc = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[doc],
    )
    .await;
    let email = common::unique_email("nonmember");
    let recipient =
        common::login_with_email(&w.state, "organization member only", &email, true).await;
    common::join_organization(w.org, &recipient, "member").await;
    let app = common::app(&w.state);
    let path = format!("/api/documents/{doc}");
    assert_eq!(
        send(&app, Method::GET, &path, Some(&recipient), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let share = send(
        &app,
        Method::POST,
        &format!("{path}/shares"),
        Some(&w.owner),
        Some(json!({"organization_id":w.org,"email":email,"permission":"read"})),
    )
    .await;
    assert!(share.status.is_success(), "{} {}", share.status, share.json);
    let read = send(&app, Method::GET, &path, Some(&recipient), None).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.json);
    let write = send(&app, Method::PUT, &path, Some(&recipient), Some(save())).await;
    assert_eq!(write.status, StatusCode::FORBIDDEN, "{}", write.json);
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT revision FROM fvoci.documents WHERE id=$1",
            &[doc]
        )
        .await["revision"],
        1
    );
}
