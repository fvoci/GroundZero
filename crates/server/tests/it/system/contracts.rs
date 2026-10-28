#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration assertions fail the test on invalid setup or response"
)]
//! 시스템 API 계약(D-96, PR3, D-131).
use crate::common;
use axum::http::{Method, StatusCode};
use common::{create_doc, create_workspace, login, send, send_raw};
use serde_json::{Value, json};
use uuid::Uuid;

async fn snapshot(ws: &str) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'workspace', (SELECT to_jsonb(w) FROM fvoci.workspaces w WHERE id = $1::uuid),
            'documents', (SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM fvoci.documents d WHERE workspace_id = $1::uuid),
            'trash', (SELECT jsonb_agg(to_jsonb(b) ORDER BY id) FROM fvoci.trash_batches b WHERE workspace_id = $1::uuid),
            'audit', (SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM fvoci.audit_log a WHERE workspace_id = $1::uuid))",
    ).bind(ws).fetch_one(&mut conn).await.unwrap()
}

#[tokio::test]
async fn sent_expected_user_is_compared_on_every_system_command_without_side_effects() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let other = login(&state, "other").await;
    let ws = create_workspace(&app, &owner, "system header").await;
    let doc = create_doc(&app, &owner, &ws, "still live").await;
    let deleted = create_doc(&app, &owner, &ws, "still trashed").await;
    let batch = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", deleted["id"].as_str().unwrap()),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(batch.status, StatusCode::OK);
    let batch = batch.json["batch_id"].as_str().unwrap();
    let id = Uuid::now_v7();
    let before = snapshot(&ws).await;
    let owner_id = owner.id.0.to_string();
    let other_id = other.id.0.to_string();
    for (method, uri, body) in [
        (
            Method::DELETE,
            format!("/api/documents/{}", doc["id"].as_str().unwrap()),
            None,
        ),
        (Method::DELETE, format!("/api/items/{id}"), None),
        (Method::DELETE, format!("/api/modules/{id}"), None),
        (
            Method::POST,
            format!("/api/trash-batches/{batch}/restore"),
            None,
        ),
        (Method::DELETE, format!("/api/trash-batches/{batch}"), None),
        (
            Method::POST,
            format!("/api/trash-batches/purge-expired?workspace_id={ws}"),
            None,
        ),
        (
            Method::POST,
            "/api/trash-batches/purge-expired".to_owned(),
            None,
        ),
        (
            Method::PUT,
            format!("/api/workspaces/{ws}/retention/trash"),
            Some(json!({"days": 45})),
        ),
        (
            Method::PUT,
            format!("/api/workspaces/{ws}/retention/file-versions"),
            Some(json!({"days": null})),
        ),
    ] {
        for (headers, expected) in [
            (
                vec![("x-fvoci-expected-user", other_id.as_str())],
                StatusCode::CONFLICT,
            ),
            (
                vec![("x-fvoci-expected-user", "invalid")],
                StatusCode::BAD_REQUEST,
            ),
            (
                vec![
                    ("x-fvoci-expected-user", owner_id.as_str()),
                    ("x-fvoci-expected-user", owner_id.as_str()),
                ],
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let mut headers = headers;
            headers.push(("content-type", "application/json"));
            let (status, _, bytes) = send_raw(
                &app,
                method.clone(),
                &uri,
                Some(&owner),
                &headers,
                body.as_ref()
                    .map(|v| serde_json::to_vec(v).unwrap())
                    .unwrap_or_default(),
            )
            .await;
            assert_eq!(
                status,
                expected,
                "{method} {uri}: {}",
                String::from_utf8_lossy(&bytes)
            );
            if status == StatusCode::CONFLICT {
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes).unwrap()["error"],
                    "account_changed"
                );
            }
        }
    }
    assert_eq!(snapshot(&ws).await, before);
    // 읽기는 기대 사용자 머리를 비교하지 않는다. 실제 세션의 읽기 권한을 쓴다.
    for uri in [
        format!("/api/trash-batches?workspace_id={ws}"),
        format!("/api/workspaces/{ws}/retention"),
    ] {
        assert_eq!(
            send_raw(
                &app,
                Method::GET,
                &uri,
                Some(&owner),
                &[("x-fvoci-expected-user", other_id.as_str())],
                vec![]
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    // 선택 머리를 필수로 만들지 않는다. 일치 머리와 없는 머리 모두 정상 명령이다.
    for headers in [vec![], vec![("x-fvoci-expected-user", owner_id.as_str())]] {
        let mut headers = headers;
        headers.push(("content-type", "application/json"));
        assert_eq!(
            send_raw(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{ws}/retention/trash"),
                Some(&owner),
                &headers,
                br#"{"days":45}"#.to_vec()
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            "/api/trash-batches/purge-expired",
            None,
            &[("x-fvoci-expected-user", owner_id.as_str())],
            vec![]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn retention_validation_names_the_days_field_and_preserves_settings() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "retention contract").await;
    let before = snapshot(&ws).await;
    for kind in ["trash", "file-versions"] {
        let uri = format!("/api/workspaces/{ws}/retention/{kind}");
        for days in [i32::MIN, 0, 3651] {
            let res = send(
                &app,
                Method::PUT,
                &uri,
                Some(&owner),
                Some(json!({"days": days})),
            )
            .await;
            assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.json);
            assert_eq!(res.json["reason"], "out_of_range");
            assert_eq!(res.json["field"], "days");
            assert_eq!(res.json["limit"], 3650);
        }
    }
    assert_eq!(snapshot(&ws).await, before);
    for (kind, days) in [
        ("trash", json!(1)),
        ("trash", json!(3650)),
        ("file-versions", Value::Null),
    ] {
        assert_eq!(
            send(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{ws}/retention/{kind}"),
                Some(&owner),
                Some(json!({"days": days}))
            )
            .await
            .status,
            StatusCode::NO_CONTENT
        );
    }
}

#[tokio::test]
async fn resource_reads_replace_the_combined_trash_api() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let outsider = login(&state, "outsider").await;
    let ws = create_workspace(&app, &owner, "trash resources").await;
    let list = send(
        &app,
        Method::GET,
        &format!("/api/trash-batches?workspace_id={ws}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.json, json!([]));
    let settings = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/retention"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(settings.status, StatusCode::OK);
    assert_eq!(
        settings.json,
        json!({"retention_days": 30, "version_retention_days": null})
    );
    for uri in [
        format!("/api/trash-batches?workspace_id={ws}"),
        format!("/api/workspaces/{ws}/retention"),
    ] {
        assert_eq!(
            send(&app, Method::GET, &uri, Some(&outsider), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    for (method, uri) in [
        (Method::GET, format!("/api/workspaces/{ws}/trash")),
        (Method::GET, "/api/me/trash".to_owned()),
        (Method::POST, "/api/me/trash/purge-expired".to_owned()),
        (
            Method::POST,
            format!("/api/workspaces/{ws}/trash/purge-expired"),
        ),
        (
            Method::POST,
            format!("/api/trash/{}/restore", Uuid::now_v7()),
        ),
        (Method::DELETE, format!("/api/trash/{}", Uuid::now_v7())),
        (Method::PUT, format!("/api/workspaces/{ws}/trash-retention")),
        (
            Method::PUT,
            format!("/api/workspaces/{ws}/version-retention"),
        ),
    ] {
        assert_eq!(
            send(&app, method, &uri, Some(&owner), None).await.status,
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
}
