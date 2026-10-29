#![expect(
    clippy::unwrap_used,
    reason = "HTTP integration assertions panic on failure"
)]
//! D-19·D-20·D-111: 읽기 공유의 고정·즐겨찾기도 현재 조직과 현재 접근권한을 따른다.
use crate::common::{self, TestUser, send};
use crate::sql_support::{World, fixture};
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

async fn check_marks(w: &World, user: &TestUser, expected: &[Uuid], failures: &mut Vec<String>) {
    let home = super::fixtures::home(w, user).await;
    let mut expected: Vec<String> = expected.iter().map(Uuid::to_string).collect();
    expected.sort();
    for category in ["pins", "favorites"] {
        let mut ids: Vec<String> = home[category]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        if ids != expected {
            failures.push(format!(
                "user {} organization {} {category}: {ids:?}, expected {expected:?}",
                user.id.0, w.org
            ));
        }
    }
}

async fn check_document_label(w: &World, user: &TestUser, document: Uuid, expected: &str) {
    let home = super::fixtures::home(w, user).await;
    for category in ["pins", "favorites"] {
        let row = home[category]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == document.to_string())
            .unwrap();
        assert_eq!(
            row["label"], expected,
            "{category} for {}: {row:?}",
            user.id.0
        );
    }
}

#[tokio::test]
async fn home_shared_document_labels_hide_inaccessible_item_references() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let reader = w.member("readonly").await;
    let email = common::unique_email("home-label-outside");
    let outside = common::login_with_email(&w.state, "outside", &email, true).await;
    common::join_organization(w.org, &outside, "member").await;
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let doc = w.doc().await;
    let path = format!("/api/documents/{doc}");
    let owner_document = send(&app, Method::GET, &path, Some(&w.owner), None).await;
    assert_eq!(
        owner_document.status,
        StatusCode::OK,
        "{:?}",
        owner_document.json
    );
    let reference = owner_document.json["reference"].as_str().unwrap();
    assert!(reference.starts_with("SQL-"), "{:?}", owner_document.json);
    assert_eq!(owner_document.json["title"], "SQL document");
    let full_label = format!("{reference} SQL document");

    for user in [&w.owner, &reader, &outside] {
        if user.id != w.owner.id {
            w.share(doc, user, "read").await;
            let document = send(&app, Method::GET, &path, Some(user), None).await;
            assert_eq!(document.status, StatusCode::OK, "{:?}", document.json);
            assert_eq!(document.json["reference"], serde_json::Value::Null);
            assert_eq!(document.json["item_id"], serde_json::Value::Null);
        }
        for mark in ["pin", "favorite"] {
            let reply = send(
                &app,
                Method::PUT,
                &format!("/api/me/marks/document/{doc}"),
                Some(user),
                Some(json!({"mark": mark, "on": true})),
            )
            .await;
            assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
        }
    }
    // 같은 문서도 상위 항목을 읽을 수 있는 소유자에게만 참조를 붙인다.
    check_document_label(&w, &w.owner, doc, &full_label).await;
    check_document_label(&w, &reader, doc, "SQL document").await;
    check_document_label(&w, &outside, doc, "SQL document").await;

    fixture(
        "UPDATE fvoci.items SET visibility='workspace' WHERE id=$1",
        &[w.item],
    )
    .await;
    let visible = send(&app, Method::GET, &path, Some(&reader), None).await;
    assert_eq!(visible.status, StatusCode::OK, "{:?}", visible.json);
    assert_eq!(visible.json["reference"], reference);
    check_document_label(&w, &reader, doc, &full_label).await;
    check_document_label(&w, &outside, doc, "SQL document").await;
}

#[tokio::test]
async fn shared_document_marks_follow_current_organization_and_revocation() {
    let w = World::new().await;
    let foreign = World::new().await;
    let app = common::app(&w.state);
    let reader = w.member("readonly").await;
    let email = common::unique_email("marks-outside");
    let outside = common::login_with_email(&w.state, "outside", &email, true).await;
    common::join_organization(w.org, &outside, "member").await;
    for user in [&reader, &outside] {
        common::join_organization(foreign.org, user, "member").await;
    }
    let doc = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[doc],
    )
    .await;
    let note = w.note().await;
    let foreign_doc = foreign.doc().await;
    let foreign_note = foreign.note().await;
    let mut shares = Vec::new();
    for user in [&reader, &outside] {
        for target in [doc, note] {
            shares.push(w.share(target, user, "read").await);
        }
        for target in [foreign_doc, foreign_note] {
            foreign.share(target, user, "read").await;
        }
        for target in [doc, note, foreign_doc, foreign_note] {
            for category in ["pin", "favorite"] {
                let reply = send(
                    &app,
                    Method::PUT,
                    &format!("/api/me/marks/document/{target}"),
                    Some(user),
                    Some(json!({"mark": category, "on": true})),
                )
                .await;
                assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
            }
        }
    }
    let mut failures = Vec::new();
    for user in [&reader, &outside] {
        check_marks(&w, user, &[doc, note], &mut failures).await;
        check_marks(&foreign, user, &[foreign_doc, foreign_note], &mut failures).await;
    }
    // 공유 문서만 읽는 수신자에게는 볼 수 없는 상위 항목 id를 내보내지 않는다.
    for (scope, user) in [(&w, &outside), (&foreign, &reader), (&foreign, &outside)] {
        let home = super::fixtures::home(scope, user).await;
        for category in ["pins", "favorites"] {
            for row in home[category].as_array().unwrap() {
                if !row["item_id"].is_null() {
                    failures.push(format!("hidden item in {category}: {row:?}"));
                }
            }
        }
    }
    for share in shares {
        let reply = send(
            &app,
            Method::DELETE,
            &format!("/api/document-shares/{share}"),
            Some(&w.owner),
            None,
        )
        .await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.json);
    }
    for user in [&reader, &outside] {
        check_marks(&w, user, &[], &mut failures).await;
        check_marks(&foreign, user, &[foreign_doc, foreign_note], &mut failures).await;
        // 고정 행은 남는다. 재공유하면 다시 표시하고 다른 조직 공유는 섞지 않는다.
        w.share(note, user, "read").await;
        check_marks(&w, user, &[note], &mut failures).await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
