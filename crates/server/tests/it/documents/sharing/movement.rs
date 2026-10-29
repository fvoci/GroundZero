//! 공유·가시성을 보존하는 문서 이동.

use super::*;

/// 미리보기 → 그 미리보기를 확인한 채로 이동. (상태, 응답) 반환.
async fn move_doc(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    target: Option<&str>,
) -> (StatusCode, Value) {
    let uri = format!("/api/documents/{}/placement", doc["id"].as_str().unwrap());
    let preview = send(
        app,
        Method::PUT,
        &uri,
        Some(user),
        Some(json!({ "target_workspace_id": target })),
    )
    .await;
    // 409라도 미리보기가 아니라 상태 오류(예: 팀 문서 → 개인)면 그대로 돌려준다.
    if preview.status != StatusCode::CONFLICT || preview.json.get("changes").is_none() {
        return (preview.status, preview.json);
    }
    // D-114: 확인은 사용자 id와 이동 전후 등급을 포함한 변화 전체에 대해 한다.
    let ack = json!({ "changes": preview.json["changes"] });
    let r = send(
        app,
        Method::PUT,
        &uri,
        Some(user),
        Some(json!({ "target_workspace_id": target, "acknowledged": ack })),
    )
    .await;
    (r.status, r.json)
}

#[tokio::test]
async fn move_requires_the_reviewed_preview_and_changes_who_can_see() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let src = create_workspace(&app, &alice, "출발").await;
    let dst = create_workspace(&app, &alice, "도착").await;
    let (src_reader, _) = add_member(&app, &state, &alice, &src, "src-reader", "readonly").await;
    let (dst_writer, _) = add_member(&app, &state, &alice, &dst, "dst-writer", "member").await;
    let doc = create_doc(&app, &alice, &src, "옮길 문서").await;
    assert_eq!(get(&app, &src_reader, &doc).await.0, StatusCode::OK);

    let uri = format!("/api/documents/{}/placement", doc["id"].as_str().unwrap());
    let preview = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::CONFLICT);
    assert_eq!(
        preview.json["changes"],
        json!([
            { "change": "gain", "user_id": dst_writer.id.0, "display_name": "dst-writer", "from_grade": null, "to_grade": "write" },
            { "change": "lose", "user_id": src_reader.id.0, "display_name": "src-reader", "from_grade": "read", "to_grade": null }
        ])
    );
    assert_eq!(preview.json["history_moves_with_document"], true);

    // 확인한 미리보기가 지금과 다르면(그사이 멤버가 바뀐 경우 등) 옮기지 않고 새 미리보기를 준다.
    let stale = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst, "acknowledged": { "changes": [] } })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(
        get(&app, &src_reader, &doc).await.0,
        StatusCode::OK,
        "not moved"
    );

    let (status, moved) = move_doc(&app, &alice, &doc, Some(&dst)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["workspace_id"], dst.as_str());
    assert_eq!(moved["reference"], "WIKI-1");
    assert_eq!(
        moved["document"]["revision"], doc["revision"],
        "moving does not create a revision"
    );
    assert_eq!(get(&app, &src_reader, &doc).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &dst_writer, &doc).await.0, StatusCode::OK);

    // 팀 문서를 개인 노트로 빼는 이동은 막는다 (D-25).
    assert_eq!(
        move_doc(&app, &alice, &doc, None).await.0,
        StatusCode::CONFLICT
    );
    // 대상 워크스페이스에서 작성 권한이 없는 사람은 옮길 수 없다.
    let note = create_note(&app, &src_reader, "독자의 노트").await;
    assert_eq!(
        move_doc(&app, &src_reader, &note, Some(&src)).await.0,
        StatusCode::FORBIDDEN
    );
}

/// 비공개 문서를 작성 권한만 있는 워크스페이스로 옮기면 옮긴 사람이 볼 수 없게 될 수 있다. 이동은 완료되고
/// 응답은 새 위치와 참조만 준다(검토에서 찾은 404·롤백 결함의 회귀 테스트).
#[tokio::test]
async fn moving_a_private_document_where_the_mover_loses_access_still_completes() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let src = create_workspace(&app, &alice, "출발").await;
    let (creator, _) = add_member(&app, &state, &alice, &src, "creator", "member").await;
    let boss = login(&state, "boss").await;
    // D-114: 공간 간 이동은 같은 조직 안에서 검증한다.
    common::join_organization(common::organization_for(&alice).await, &boss, "member").await;
    let dst = create_workspace(&app, &boss, "남의 팀").await;
    let alice_email = unique_email("alice-dst");
    // 앨리스를 대상 워크스페이스의 작성자로 들인다(같은 사람이 다른 이메일로 두 번 로그인하는 대신 초대를 쓴다).
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{dst}/invitations"),
        Some(&boss),
        Some(json!({ "email": alice_email, "role": "member" })),
    )
    .await;
    let alice_with_email = login_with_email(&state, "alice-src", &alice_email, true).await;
    // 워크스페이스 초대는 그 조직의 멤버만 수락한다(D-114).
    common::join_organization(
        common::organization_of_workspace(&dst).await,
        &alice_with_email,
        "member",
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&alice_with_email),
        None,
    )
    .await;
    // alice_with_email을 출발 워크스페이스의 관리자로 만든다.
    let inv2 = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{src}/invitations"),
        Some(&alice),
        Some(json!({ "email": alice_email, "role": "admin" })),
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv2.json["id"].as_str().unwrap()
        ),
        Some(&alice_with_email),
        None,
    )
    .await;

    let doc = create_doc(&app, &creator, &src, "작성자 비공개").await;
    assert_eq!(
        settings(&app, &alice_with_email, &doc, "private", false).await,
        StatusCode::OK
    );
    let (status, moved) = move_doc(&app, &alice_with_email, &doc, Some(&dst)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["workspace_id"], dst.as_str());
    assert_eq!(moved["document"], Value::Null);
    assert_eq!(
        get(&app, &boss, &doc).await.0,
        StatusCode::OK,
        "target admin sees it"
    );
    assert_eq!(
        get(&app, &alice_with_email, &doc).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn personal_note_moves_into_a_workspace_as_private() {
    let state = common::state();
    let app = common::app(&state);
    let boss = login(&state, "boss").await;
    let ws = create_workspace(&app, &boss, "팀").await;
    let alice_email = unique_email("alice2");
    let alice2 = login_with_email(&state, "alice2", &alice_email, true).await;
    common::join_organization(
        common::organization_of_workspace(&ws).await,
        &alice2,
        "member",
    )
    .await;
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&boss),
        Some(json!({ "email": alice_email, "role": "member" })),
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&alice2),
        None,
    )
    .await;
    let (peer, _) = add_member(&app, &state, &boss, &ws, "peer", "member").await;

    let note = create_note(&app, &alice2, "개인 → 팀").await;
    let uri = format!("/api/documents/{}/placement", note["id"].as_str().unwrap());
    let preview = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice2),
        Some(json!({ "target_workspace_id": ws })),
    )
    .await;
    // 개인 노트는 비공개로 들어가므로 관리자만 새로 볼 수 있다.
    let gains: Vec<_> = preview.json["changes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["change"] == "gain")
        .collect();
    assert_eq!(
        gains,
        vec![
            &json!({ "change": "gain", "user_id": boss.id.0, "display_name": "boss", "from_grade": null, "to_grade": "owner" })
        ]
    );
    let (status, moved) = move_doc(&app, &alice2, &note, Some(&ws)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["document"]["placement"], "wiki");
    assert_eq!(moved["document"]["visibility"], "private");
    assert_eq!(get(&app, &boss, &note).await.0, StatusCode::OK);
    assert_eq!(get(&app, &peer, &note).await.0, StatusCode::NOT_FOUND);
}
