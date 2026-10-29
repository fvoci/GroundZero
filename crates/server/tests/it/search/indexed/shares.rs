//! D-20·D-39·D-111: 공유 변경은 재색인을 기다리지 않고 현재 조직의 검색에 반영한다.
use super::*;

async fn share(
    app: &Router,
    owner: &TestUser,
    document: &Value,
    email: &str,
    organization: Uuid,
) -> String {
    let reply = send(
        app,
        Method::POST,
        &format!("/api/documents/{}/shares", document["id"].as_str().unwrap()),
        Some(owner),
        Some(json!({"email": email, "permission": "read", "organization_id": organization})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json);
    reply.json["id"].as_str().unwrap().to_owned()
}

async fn note(app: &Router, owner: &TestUser, title: &str, body: &str) -> Value {
    let reply = send(
        app,
        Method::POST,
        "/api/me/notes",
        Some(owner),
        Some(json!({"request_id": new_request_id(), "title": title})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json);
    let saved = common::live::save(
        app,
        owner,
        &format!("/api/documents/{}", reply.json["id"].as_str().unwrap()),
        &json!({"request_id": new_request_id(), "base_revision": 1, "title": title, "body": doc_body(body)}),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    reply.json
}

async fn check_titles(
    app: &Router,
    user: &TestUser,
    organization: Uuid,
    query: &str,
    expected: &[String],
    failures: &mut Vec<String>,
) {
    let reply = send(
        app,
        Method::GET,
        &format!("/api/search?organization_id={organization}&q={query}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
    let mut expected = expected.to_vec();
    expected.sort();
    if titles(&reply.json) != expected {
        failures.push(format!(
            "user {} organization {organization} query {query}: {:?}, expected {expected:?}",
            user.id.0,
            titles(&reply.json)
        ));
    }
}

#[tokio::test]
async fn shared_documents_follow_current_organization_without_reindexing() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let body_tok = token();
    let owner = login(&state, "shared-search-owner").await;
    let ws = create_workspace(&app, &owner, "shared search").await;
    let organization = common::organization_for(&owner).await;
    let (reader, reader_email) = add_member(
        &app,
        &state,
        &owner,
        &ws,
        "shared-search-reader",
        "readonly",
    )
    .await;
    let outside_email = common::unique_email("shared-search-outside");
    let outside = common::login_with_email(&state, "outside", &outside_email, true).await;
    common::join_organization(organization, &outside, "member").await;
    let foreign_owner = login(&state, "foreign-search-owner").await;
    let foreign_ws = create_workspace(&app, &foreign_owner, "foreign search").await;
    let foreign_organization = common::organization_for(&foreign_owner).await;
    for user in [&reader, &outside] {
        common::join_organization(foreign_organization, user, "member").await;
    }

    let doc_title = format!("shared document {tok}");
    let note_title = format!("shared note {tok}");
    let file_title = format!("shared attachment {tok}.txt");
    let doc = create_doc(&app, &owner, &ws, &doc_title).await;
    common::fixture_sql(
        "UPDATE fvoci.documents SET visibility = 'private' WHERE id = $1::uuid",
        doc["id"].as_str().unwrap(),
    )
    .await;
    let note = note(&app, &owner, &note_title, &body_tok).await;
    upload_bytes(&app, &owner, &doc, &file_title, "text/plain", b"attachment").await;
    let foreign_doc_title = format!("foreign document {tok}");
    let foreign_note_title = format!("foreign note {tok}");
    let foreign_doc = create_doc(&app, &foreign_owner, &foreign_ws, &foreign_doc_title).await;
    let foreign_note = self::note(&app, &foreign_owner, &foreign_note_title, &body_tok).await;
    settle(
        &state,
        &app,
        &owner,
        &tok,
        &[&doc_title, &note_title, &file_title],
    )
    .await;
    settle(
        &state,
        &app,
        &foreign_owner,
        &tok,
        &[&foreign_doc_title, &foreign_note_title],
    )
    .await;

    let mut failures = Vec::new();
    for user in [&reader, &outside] {
        for path in [&app, &plain] {
            check_titles(path, user, organization, &tok, &[], &mut failures).await;
        }
    }
    let mut shares = Vec::new();
    for (user, email) in [(&reader, &reader_email), (&outside, &outside_email)] {
        for document in [&doc, &note] {
            shares.push(share(&app, &owner, document, email, organization).await);
            let opened = send(
                &app,
                Method::GET,
                &format!("/api/documents/{}", document["id"].as_str().unwrap()),
                Some(user),
                None,
            )
            .await;
            assert_eq!(opened.status, StatusCode::OK, "{:?}", opened.json);
            assert_eq!(opened.json["access"], "read");
        }
        for document in [&foreign_doc, &foreign_note] {
            share(&app, &foreign_owner, document, email, foreign_organization).await;
        }
    }
    // 이 뒤에는 drain/settle을 부르지 않는다. 공유·철회는 색인 갱신과 무관하게 반영되어야 한다.
    let expected = [doc_title, note_title.clone(), file_title];
    let foreign_expected = [foreign_doc_title, foreign_note_title.clone()];
    for user in [&reader, &outside] {
        for path in [&app, &plain] {
            check_titles(path, user, organization, &tok, &expected, &mut failures).await;
            check_titles(
                path,
                user,
                foreign_organization,
                &tok,
                &foreign_expected,
                &mut failures,
            )
            .await;
        }
        check_titles(
            &app,
            user,
            organization,
            &body_tok,
            std::slice::from_ref(&note_title),
            &mut failures,
        )
        .await;
        check_titles(
            &app,
            user,
            foreign_organization,
            &body_tok,
            std::slice::from_ref(&foreign_note_title),
            &mut failures,
        )
        .await;
    }
    for id in shares {
        let reply = send(
            &app,
            Method::DELETE,
            &format!("/api/document-shares/{id}"),
            Some(&owner),
            None,
        )
        .await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.json);
    }
    for user in [&reader, &outside] {
        for path in [&app, &plain] {
            check_titles(path, user, organization, &tok, &[], &mut failures).await;
            check_titles(
                path,
                user,
                foreign_organization,
                &tok,
                &foreign_expected,
                &mut failures,
            )
            .await;
        }
        check_titles(&app, user, organization, &body_tok, &[], &mut failures).await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
