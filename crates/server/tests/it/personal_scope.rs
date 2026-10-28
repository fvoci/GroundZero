#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 조직 범위 개인 화면은 실제 앱 역할·HTTP로 확인한다.
use crate::common;
use crate::sql_support;

use axum::http::{Method, StatusCode};
use common::{TestUser, send};
use serde_json::{Value, json};
use sql_support::*;
use uuid::Uuid;

async fn get(w: &World, user: &TestUser, path: &str) -> Value {
    let reply = send(&common::app(&w.state), Method::GET, path, Some(user), None).await;
    assert_eq!(reply.status, StatusCode::OK, "{path}: {:?}", reply.json);
    reply.json
}

async fn join(w: &World, user: &TestUser) {
    common::join_organization(w.org, user, "member").await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[w.ws, user.id.0],
    )
    .await;
}

async fn assign(task: Uuid, user: &TestUser) {
    fixture(
        "INSERT INTO fvoci.task_assignees(task_id,user_id,assigned_by) VALUES($1,$2,$2)",
        &[task, user.id.0],
    )
    .await;
}

#[tokio::test]
async fn tasks_are_assigned_to_me_grouped_by_workspace_and_scoped_to_organization() {
    let w = World::new().await;
    let other = World::new().await;
    join(&other, &w.owner).await;
    let peer_ws = workspace(&w.state, &w.owner, w.org).await;
    let peer_module = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_module($1,$2,'project','Peer','PEER',@ids) AS id",
        &[peer_ws, Uuid::now_v7()],
    )
    .await;
    let peer_item = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_item($1,$2,'Peer item','',@ids) AS id",
        &[peer_module, Uuid::now_v7()],
    )
    .await;
    let peer_task = task(&w.state, &w.owner, peer_item).await;
    assign(peer_task, &w.owner).await;
    let mut expected = Vec::new();
    for (bucket, due, done) in [
        ("overdue", Some("2026-10-25"), false),
        ("today", Some("2026-10-26"), false),
        ("upcoming", Some("2026-10-27"), false),
        ("none", None, false),
        ("done", None, true),
    ] {
        let task = w.task().await;
        assign(task, &w.owner).await;
        let mut conn = common::fixture_conn().await;
        sqlx::query("UPDATE fvoci.tasks SET due_date=$2::text::date, status_id=CASE WHEN $3 THEN (SELECT id FROM fvoci.task_statuses WHERE item_id=$4 AND category='done' LIMIT 1) ELSE status_id END WHERE id=$1")
            .bind(task).bind(due).bind(done).bind(w.item).execute(&mut conn).await.unwrap();
        expected.push((task, bucket));
    }
    let unassigned = w.task().await;
    let foreign = other.task().await;
    assign(foreign, &w.owner).await;
    let result = get(
        &w,
        &w.owner,
        &format!("/api/me/tasks?organization_id={}&today=2026-10-26", w.org),
    )
    .await;
    assert_eq!(
        result["counts"],
        json!({"overdue":1,"today":1,"upcoming":1,"undated":2,"done":1})
    );
    let groups = result["workspaces"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let primary = groups
        .iter()
        .find(|g| g["workspace_id"] == w.ws.to_string())
        .unwrap();
    assert_eq!(primary["workspace_name"], "SQL workspace");
    let tasks = primary["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 5);
    for (id, bucket) in expected {
        let row = tasks.iter().find(|t| t["id"] == id.to_string()).unwrap();
        assert_eq!(row["bucket"], bucket);
        assert_eq!(row["item_id"], w.item.to_string());
        assert!(row["updated_at"].is_string());
    }
    assert!(tasks.iter().all(|t| t["id"] != unassigned.to_string()));
    let result = get(
        &w,
        &w.owner,
        &format!("/api/me/tasks?organization_id={}", other.org),
    )
    .await;
    assert_eq!(result["workspaces"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["workspaces"][0]["workspace_id"],
        other.ws.to_string()
    );
    assert_eq!(
        result["workspaces"][0]["tasks"][0]["id"],
        foreign.to_string()
    );
    let reply = send(
        &common::app(&w.state),
        Method::GET,
        &format!("/api/me/tasks?organization_id={}&today=invalid", w.org),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn home_shared_and_search_stay_in_current_organization() {
    let w = World::new().await;
    // World의 기본 소유자는 이메일 없는 계정이다. 공유 수신은 검증된 이메일이 필요하다.
    sqlx::query("UPDATE fvoci.users SET email=$2,email_verified=true WHERE id=$1")
        .bind(w.owner.id)
        .bind(common::unique_email("personal-scope-owner"))
        .execute(&mut common::fixture_conn().await)
        .await
        .unwrap();
    let other = World::new().await;
    join(&other, &w.owner).await;
    let first = w.doc().await;
    let second = other.doc().await;
    other.share(second, &w.owner, "write").await;
    fixture(
        "UPDATE fvoci.documents SET updated_by=$1 WHERE id=$2",
        &[w.owner.id.0, second],
    )
    .await;
    let own_note = w.note().await;
    let shared_note = other.note().await;
    other.share(shared_note, &w.owner, "read").await;
    let first_task = w.task().await;
    let second_task = other.task().await;
    for target in [first_task, second_task] {
        assign(target, &w.owner).await;
        fixture(
            "UPDATE fvoci.tasks SET updated_by=$1,due_date=current_date WHERE id=$2",
            &[w.owner.id.0, target],
        )
        .await;
        rows(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_set_mark('pin','task',$1,true)",
            &[target],
        )
        .await;
    }
    for (org, ws, doc, task) in [
        (w.org, w.ws, first, first_task),
        (other.org, other.ws, second, second_task),
    ] {
        let home = get(&w, &w.owner, &format!("/api/me/home?organization_id={org}")).await;
        for key in ["recent_tasks", "active_items", "up_next", "pins"] {
            let list = home[key].as_array().unwrap();
            assert!(!list.is_empty(), "{key}: {home}");
            assert!(
                list.iter().all(|r| r["workspace_id"] == ws.to_string()),
                "{key}: {home}"
            );
        }
        assert!(
            home["recent_tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == task.to_string())
        );
        let docs = home["recent_documents"].as_array().unwrap();
        assert!(docs.iter().any(|r| r["id"] == doc.to_string()));
        assert!(
            docs.iter()
                .all(|r| r["workspace_id"] == ws.to_string() || r["id"] == own_note.to_string())
        );
        let search = get(
            &w,
            &w.owner,
            &format!("/api/search?organization_id={org}&q=SQL"),
        )
        .await;
        let results = search.as_array().unwrap();
        assert!(results.iter().any(|r| r["id"] == doc.to_string()));
        assert!(results.iter().any(|r| r["id"] == own_note.to_string()));
        assert!(
            results
                .iter()
                .all(|r| r["workspace_id"] == ws.to_string() || r["id"] == own_note.to_string())
        );
    }
    let empty = get(
        &w,
        &w.owner,
        &format!("/api/me/shared?organization_id={}", w.org),
    )
    .await;
    assert_eq!(empty, json!([]));
    let shared = get(
        &w,
        &w.owner,
        &format!("/api/me/shared?organization_id={}", other.org),
    )
    .await;
    assert_eq!(shared.as_array().unwrap().len(), 2);
    for id in [second, shared_note] {
        assert!(
            shared
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == id.to_string())
        );
    }
    let mismatch = send(
        &common::app(&w.state),
        Method::GET,
        &format!(
            "/api/search?organization_id={}&workspace_id={}&q=SQL",
            w.org, other.ws
        ),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(mismatch.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn notifications_keyset_invites_counts_and_read_all_follow_sql_scope() {
    let w = World::new().await;
    let other = World::new().await;
    let user = w.member("member").await;
    join(&other, &user).await;
    let task = w.task().await;
    let foreign = other.task().await;
    let mut ids = Vec::new();
    for (org, task) in [
        (w.org, task),
        (w.org, task),
        (w.org, task),
        (other.org, foreign),
    ] {
        let id = Uuid::now_v7();
        fixture("INSERT INTO fvoci.notifications(id,user_id,organization_id,kind,task_id,detail,created_at) VALUES($1,$2,$3,'due',$4,'{\"due_date\":\"2026-10-26\"}','2026-01-01 00:00Z')",&[id,user.id.0,org,task]).await;
        ids.push(id);
    }
    let third = World::new().await;
    let email: String = sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
        .bind(user.id)
        .fetch_one(&mut common::fixture_conn().await)
        .await
        .unwrap();
    let mut tx = third.state.db.begin_as(third.owner.id).await.unwrap();
    let invitation: Uuid = sqlx::query_scalar("SELECT fvoci.app_invite($1,$2,'member',3600,$3)")
        .bind(third.ws)
        .bind(email)
        .bind(sql_support::ids())
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let first = get(
        &w,
        &user,
        &format!("/api/me/notifications?organization_id={}&limit=2", w.org),
    )
    .await;
    let cursor = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("before", first["next_before"].as_str().unwrap())
        .append_pair("before_id", first["next_before_id"].as_str().unwrap())
        .finish();
    let second = get(
        &w,
        &user,
        &format!(
            "/api/me/notifications?organization_id={}&limit=2&{cursor}",
            w.org
        ),
    )
    .await;
    let a = first["notifications"].as_array().unwrap();
    let b = second["notifications"].as_array().unwrap();
    assert_eq!((a.len(), b.len()), (2, 2));
    assert!(a.iter().all(|a| b.iter().all(|b| a["id"] != b["id"])));
    assert!(
        a.iter()
            .chain(b)
            .any(|r| r["invitation_id"] == invitation.to_string())
    );
    assert!(a.iter().chain(b).all(|r| r["id"] != ids[3].to_string()));
    for (org, count) in [(w.org, 4), (other.org, 2)] {
        let list = get(
            &w,
            &user,
            &format!("/api/me/notifications?organization_id={org}"),
        )
        .await;
        assert!(
            list["notifications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["invitation_id"] == invitation.to_string())
        );
        assert_eq!(
            get(
                &w,
                &user,
                &format!("/api/me/notifications/unread-count?organization_id={org}")
            )
            .await["unread_count"],
            count
        );
    }
    let read = send(
        &common::app(&w.state),
        Method::POST,
        &format!("/api/me/notifications/read?organization_id={}", w.org),
        Some(&user),
        Some(json!({"id":ids[0]})),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.json["marked"], 1);
    let read = send(
        &common::app(&w.state),
        Method::POST,
        &format!("/api/me/notifications/read?organization_id={}", w.org),
        Some(&user),
        Some(json!({})),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.json["marked"], 3);
    assert_eq!(
        get(
            &w,
            &user,
            &format!(
                "/api/me/notifications/unread-count?organization_id={}",
                w.org
            )
        )
        .await["unread_count"],
        0
    );
    assert_eq!(
        get(
            &w,
            &user,
            &format!(
                "/api/me/notifications/unread-count?organization_id={}",
                other.org
            )
        )
        .await["unread_count"],
        1
    );
    let read = get(
        &w,
        &user,
        &format!("/api/me/notifications?organization_id={}", w.org),
    )
    .await;
    assert!(
        read["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["read"] == true && r["read_at"].is_string())
    );
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let hidden = get(
        &w,
        &user,
        &format!("/api/me/notifications?organization_id={}", w.org),
    )
    .await;
    assert_eq!(hidden["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(hidden["notifications"][0]["kind"], "invite");
}

#[tokio::test]
async fn activity_and_scope_rejections_use_current_access() {
    let w = World::new().await;
    let other = World::new().await;
    let user = w.member("member").await;
    join(&other, &user).await;
    let task = w.task().await;
    let doc = w.doc().await;
    w.share(doc, &user, "read").await;
    other.task().await;
    for _ in 0..3 {
        fixture("INSERT INTO fvoci.activity(id,organization_id,workspace_id,item_id,target_kind,target_id,kind,created_at) VALUES($1,$2,$3,$4,'task',$5,'status','2026-01-01 00:00Z')",&[Uuid::now_v7(),w.org,w.ws,w.item,task]).await;
    }
    for (org, ws) in [(w.org, w.ws), (other.org, other.ws)] {
        let activity = get(
            &w,
            &user,
            &format!("/api/me/activity?organization_id={org}"),
        )
        .await;
        assert!(!activity.as_array().unwrap().is_empty());
        assert!(
            activity
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["workspace_id"] == ws.to_string())
        );
    }
    let first = get(&w, &user, &format!("/api/tasks/{task}/activity?limit=2")).await;
    let cursor = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("before", first["next_before"].as_str().unwrap())
        .append_pair("before_id", first["next_before_id"].as_str().unwrap())
        .finish();
    let second = get(
        &w,
        &user,
        &format!("/api/tasks/{task}/activity?limit=2&{cursor}"),
    )
    .await;
    assert!(!second["activity"].as_array().unwrap().is_empty());
    assert!(first["activity"].as_array().unwrap().iter().all(|a| {
        second["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| a["id"] != b["id"])
    }));
    for uri in [
        format!("/api/items/{}/activity", w.item),
        format!("/api/documents/{doc}/activity"),
    ] {
        let activity = get(&w, &user, &uri).await;
        assert!(!activity["activity"].as_array().unwrap().is_empty());
    }
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let recent = get(
        &w,
        &user,
        &format!("/api/me/activity?organization_id={}", w.org),
    )
    .await;
    assert!(
        recent
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["target_id"] != task.to_string() && r["target_id"] != w.item.to_string())
    );
    let app = common::app(&w.state);
    for uri in [
        format!("/api/items/{}/activity", w.item),
        format!("/api/tasks/{task}/activity"),
    ] {
        assert_eq!(
            send(&app, Method::GET, &uri, Some(&user), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    for path in [
        "/api/me/home",
        "/api/me/tasks",
        "/api/me/shared",
        "/api/search",
        "/api/me/notifications",
        "/api/me/notifications/unread-count",
        "/api/me/activity",
        "/api/me/notifications/read",
    ] {
        let method = if path.ends_with("/read") {
            Method::POST
        } else {
            Method::GET
        };
        let body = (method == Method::POST).then(|| json!({}));
        assert_eq!(
            send(&app, method.clone(), path, Some(&user), body.clone())
                .await
                .status,
            StatusCode::BAD_REQUEST,
            "{path}"
        );
        let uri = format!("{path}?organization_id={}&q=SQL", Uuid::now_v7());
        assert_eq!(
            send(&app, method, &uri, Some(&user), body).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let third = World::new().await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/me/tasks?organization_id={}", third.org),
            Some(&user),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}
