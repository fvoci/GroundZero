//! 소통·활동 API의 선택 기대 사용자와 칸 오류 계약.
#![expect(clippy::unwrap_used, reason = "integration fixtures must fail loudly")]

use crate::common::{self, TestUser, new_request_id, send, send_raw};
use crate::sql_support::{COMMENT, World, made, rows};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use uuid::Uuid;

const HEADER: &str = "x-fvoci-expected-user";

async fn raw(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    expected: &[&str],
    body: &Value,
) -> (StatusCode, Value) {
    let mut headers = vec![("content-type", "application/json")];
    for value in expected {
        headers.push((HEADER, value));
    }
    let (status, _, bytes) = send_raw(
        app,
        method,
        uri,
        user,
        &headers,
        serde_json::to_vec(body).unwrap(),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn snapshot(w: &World) -> Value {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let value = sqlx::query_scalar("SELECT jsonb_build_object(
        'comments', (SELECT jsonb_agg(to_jsonb(c) ORDER BY id) FROM comments c WHERE created_by=app_user_id()),
        'marks', (SELECT jsonb_agg(to_jsonb(m) ORDER BY target_id,mark) FROM user_marks m WHERE user_id=app_user_id()),
        'notifications', (SELECT jsonb_agg(to_jsonb(n) ORDER BY id) FROM notifications n WHERE user_id=app_user_id()),
        'activity', (SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM activity a WHERE actor=app_user_id()))")
        .fetch_one(tx.conn()).await.unwrap();
    tx.commit().await.unwrap();
    value
}

#[tokio::test]
async fn every_activity_command_compares_a_sent_user_before_changing_anything() {
    let w = World::new().await;
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let app = common::app(&w.state);
    let other = Uuid::now_v7().to_string();
    let me = w.owner.id.0.to_string();
    let commands = [
        (
            Method::POST,
            "/api/comments".to_owned(),
            json!({"request_id":new_request_id(),"task_id":task,"body":"new"}),
        ),
        (
            Method::PUT,
            format!("/api/comments/{comment}"),
            json!({"body":"edited"}),
        ),
        (
            Method::DELETE,
            format!("/api/comments/{comment}"),
            Value::Null,
        ),
        (
            Method::POST,
            format!("/api/comments/{comment}/resolve"),
            json!({"resolved":true}),
        ),
        (
            Method::POST,
            format!("/api/me/notifications/read?organization_id={}", w.org),
            json!({"id":null}),
        ),
        (
            Method::PUT,
            format!("/api/me/marks/task/{task}"),
            json!({"mark":"pin","on":true}),
        ),
    ];
    for (method, uri, body) in commands {
        let before = snapshot(&w).await;
        for (headers, user, expected, code) in [
            (
                vec![other.as_str()],
                Some(&w.owner),
                StatusCode::CONFLICT,
                "account_changed",
            ),
            (
                vec!["bad-user"],
                Some(&w.owner),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                vec![me.as_str(), me.as_str()],
                Some(&w.owner),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                vec!["bad-user"],
                None,
                StatusCode::UNAUTHORIZED,
                "unauthorized",
            ),
        ] {
            let (status, value) = raw(&app, method.clone(), &uri, user, &headers, &body).await;
            assert_eq!(status, expected, "{method} {uri}: {value:?}");
            assert_eq!(value["error"], code, "{method} {uri}");
            assert_eq!(snapshot(&w).await, before, "rejected {method} {uri}");
        }
        // 머리는 선택이다. 없을 때와 일치할 때 모두 평소 명령을 수행한다. 삭제는 마지막에 실행한다.
        if method != Method::DELETE {
            for headers in [vec![], vec![me.as_str()]] {
                let (status, value) =
                    raw(&app, method.clone(), &uri, Some(&w.owner), &headers, &body).await;
                assert!(status.is_success(), "{method} {uri}: {status} {value:?}");
            }
        }
    }
    for headers in [vec![], vec![me.as_str()]] {
        let id = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
        let (status, _) = raw(
            &app,
            Method::DELETE,
            &format!("/api/comments/{id}"),
            Some(&w.owner),
            &headers,
            &Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (status, _) = raw(
        &app,
        Method::GET,
        &format!("/api/tasks/{task}/comments"),
        Some(&w.owner),
        &["bad-user"],
        &Value::Null,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "reads do not compare a command header"
    );
}

#[tokio::test]
async fn comment_field_errors_report_required_and_unicode_length_without_writes() {
    let w = World::new().await;
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let app = common::app(&w.state);
    for body in ["   ".to_owned(), "가".repeat(10001)] {
        let before = snapshot(&w).await;
        for (method, uri, input) in [
            (
                Method::POST,
                "/api/comments".to_owned(),
                json!({"request_id":new_request_id(),"task_id":task,"body":body}),
            ),
            (
                Method::PUT,
                format!("/api/comments/{comment}"),
                json!({"body":body}),
            ),
        ] {
            let r = send(&app, method, &uri, Some(&w.owner), Some(input)).await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
            assert_eq!(r.json["field"], "body");
            if body.len() == 3 {
                assert_eq!(r.json["reason"], "required");
                assert!(r.json["limit"].is_null());
            } else {
                assert_eq!(r.json["reason"], "too_long");
                assert_eq!(r.json["limit"], 10000);
            }
            assert_eq!(snapshot(&w).await, before);
        }
    }
    // 한글 10,000자는 바이트 수와 무관하게 허용한다. SQL의 공백 정리도 유지한다.
    let r=send(&app, Method::POST, "/api/comments", Some(&w.owner), Some(json!({"request_id":new_request_id(),"task_id":task,"body":format!(" {} ","가".repeat(10000))}))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let comments = rows(
        &w.state,
        &w.owner,
        "SELECT length(body) AS length FROM fvoci.comments WHERE task_id=$1",
        &[task],
    )
    .await;
    assert!(comments.iter().any(|c| c["length"] == 10000));
}

#[tokio::test]
async fn incomplete_cursors_name_the_missing_field_in_both_feeds() {
    let w = World::new().await;
    let app = common::app(&w.state);
    for base in [
        format!("/api/me/notifications?organization_id={}", w.org),
        format!("/api/items/{}/activity?limit=5", w.item),
    ] {
        for (query, field) in [
            ("before=2026-10-28T00:00:00Z", "before_id"),
            ("before_id=00000000-0000-0000-0000-000000000001", "before"),
        ] {
            let r = send(
                &app,
                Method::GET,
                &format!("{base}&{query}"),
                Some(&w.owner),
                None,
            )
            .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
            assert_eq!(r.json["field"], field);
            assert_eq!(r.json["reason"], "required");
            assert!(r.json["limit"].is_null());
        }
    }
}

#[tokio::test]
async fn the_mark_limit_names_the_category_and_keeps_existing_marks() {
    let w = World::new().await;
    let app = common::app(&w.state);
    for _ in 0..100 {
        let task = w.task().await;
        rows(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_set_mark('pin','task',$1,true)",
            &[task],
        )
        .await;
    }
    let task = w.task().await;
    let uri = format!("/api/me/marks/task/{task}");
    let before = snapshot(&w).await;
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&w.owner),
        Some(json!({"mark":"pin","on":true})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    assert_eq!(r.json["field"], "mark");
    assert_eq!(r.json["reason"], "too_many");
    assert_eq!(r.json["limit"], 100);
    assert_eq!(snapshot(&w).await, before);
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&w.owner),
        Some(json!({"mark":"favorite","on":true})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
}

#[tokio::test]
async fn comment_validation_keeps_permission_archive_and_request_replay_order() {
    let w = World::new().await;
    let task = w.task().await;
    let reader = w.member("readonly").await;
    let outsider = common::login(&w.state, "comment outsider").await;
    let app = common::app(&w.state);
    let request = new_request_id();
    let first = send(
        &app,
        Method::POST,
        "/api/comments",
        Some(&w.owner),
        Some(json!({"request_id":request,"task_id":task,"body":"first"})),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{:?}", first.json);
    let replay = send(
        &app,
        Method::POST,
        "/api/comments",
        Some(&w.owner),
        Some(json!({"request_id":request,"task_id":task,"body":"x".repeat(10001)})),
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED, "{:?}", replay.json);
    assert_eq!(
        replay.json, first.json,
        "a replay returns before validation"
    );
    for (user, expected) in [
        (&reader, StatusCode::FORBIDDEN),
        (&outsider, StatusCode::NOT_FOUND),
    ] {
        let r = send(
            &app,
            Method::POST,
            "/api/comments",
            Some(user),
            Some(json!({"request_id":new_request_id(),"task_id":task,"body":""})),
        )
        .await;
        assert_eq!(r.status, expected, "{:?}", r.json);
    }
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_archive_item($1,true,@ids)",
        &[w.item],
    )
    .await;
    for (method, uri, input) in [
        (
            Method::POST,
            "/api/comments".to_owned(),
            json!({"request_id":new_request_id(),"task_id":task,"body":""}),
        ),
        (
            Method::PUT,
            format!("/api/comments/{}", first.json["id"].as_str().unwrap()),
            json!({"body":""}),
        ),
    ] {
        let r = send(&app, method, &uri, Some(&w.owner), Some(input)).await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
        assert_eq!(r.json["error"], "invalid_state");
    }
}

#[tokio::test]
async fn target_and_thread_validation_name_the_rejected_field() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let r = send(
        &app,
        Method::POST,
        "/api/comments",
        Some(&w.owner),
        Some(json!({"request_id":new_request_id(),"body":"no target"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    assert_eq!(r.json["field"], "document_id");
    assert_eq!(r.json["reason"], "invalid_format");
    let task = w.task().await;
    let root = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let reply = send(
        &app,
        Method::POST,
        "/api/comments",
        Some(&w.owner),
        Some(json!({"request_id":new_request_id(),"task_id":task,"parent_id":root,"body":"reply"})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json);
    for (uri, input) in [
        (
            "/api/comments".to_owned(),
            json!({"request_id":new_request_id(),"task_id":task,"parent_id":reply.json["id"],"body":"nested reply"}),
        ),
        (
            format!(
                "/api/comments/{}/resolve",
                reply.json["id"].as_str().unwrap()
            ),
            json!({"resolved":true}),
        ),
    ] {
        let r = send(&app, Method::POST, &uri, Some(&w.owner), Some(input)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
        assert_eq!(r.json["field"], "parent_id");
        assert_eq!(r.json["reason"], "invalid_format");
    }
}
