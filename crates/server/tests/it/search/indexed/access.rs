use super::*;

#[tokio::test]
async fn search_returns_only_what_each_person_can_see() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let other_ws = create_workspace(&app, &outsider, "남의 공간").await;

    let doc = create_doc(&app, &admin, &ws, &format!("회의록 {tok}")).await;
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": format!("회의록 {tok}"),
                     "body": doc_body(&format!("안건 정리와 {tok}body 결정")) })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    create_doc(&app, &outsider, &other_ws, &format!("남의 문서 {tok}")).await;
    let it = private_item(&app, &admin, &ws, "SRC").await;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("비공개 업무 {tok}") })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": format!("내 노트 {tok}") })),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);

    // 작성자: 워크스페이스 문서와 자기 노트. 남의 공간, 비공개 항목의 업무는 없다.
    let mine = settle(
        &state,
        &app,
        &writer,
        &tok,
        &[&format!("내 노트 {tok}"), &format!("회의록 {tok}")],
    )
    .await;
    let found = mine
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "document" && r["id"] == doc["id"])
        .unwrap();
    assert_eq!(found["reference"], doc["reference"]);
    // 관리자: 문서와 비공개 업무. 남의 개인 노트는 관리자에게도 없다.
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[&format!("비공개 업무 {tok}"), &format!("회의록 {tok}")],
    )
    .await;
    settle(
        &state,
        &app,
        &outsider,
        &tok,
        &[&format!("남의 문서 {tok}")],
    )
    .await;

    // 본문으로도 찾고, 찾은 곳 근처 글자를 보여 준다.
    let by_body = settle(
        &state,
        &app,
        &writer,
        &format!("{tok}body"),
        &[&format!("회의록 {tok}")],
    )
    .await;
    assert!(
        by_body[0]["snippet"]
            .as_str()
            .unwrap()
            .contains(&format!("{tok}body")),
        "{by_body:?}"
    );

    // 다른 워크스페이스로 좁히면 멤버가 아니므로 없는 것과 같다.
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&workspace_id={other_ws}&organization_id={}",
            common::organization_for(&writer).await
        ),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&kinds=nope&organization_id={}",
            common::organization_for(&writer).await
        ),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&kinds=task&organization_id={}",
            common::organization_for(&admin).await
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(titles(&r.json), [format!("비공개 업무 {tok}")]);

    // 멤버에서 빠지면 색인을 다시 만들지 않아도 바로 안 보인다.
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    assert_eq!(
        titles(&search(&app, &writer, &tok).await),
        [format!("내 노트 {tok}")]
    );

    // Meilisearch가 없으면 제목만 DB에서 찾고 같은 규칙으로 거른다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &tok).await),
        [format!("비공개 업무 {tok}"), format!("회의록 {tok}")]
    );
    assert_eq!(
        titles(&search(&plain_app, &writer, &tok).await),
        [format!("내 노트 {tok}")]
    );
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}body")).await),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn file_names_are_searchable_by_people_who_can_see_the_file() {
    let _queue = SHARED_QUEUE.read().await;
    use sha2::{Digest, Sha256};
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "파일 검색").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &admin, &ws, "자료 모음").await;
    let up = send(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                     "name": format!("{tok}-결과.csv"), "size": 3, "content_type": "text/csv" })),
    )
    .await;
    assert_eq!(up.status, StatusCode::CREATED, "{:?}", up.json);
    let id = up.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(b"a,b"));
    let (status, _, _) = common::send_raw(
        &app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(&admin),
        &[("x-content-sha256", hash.as_str())],
        b"a,b".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        &app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    let file_id = done.json["file_id"].as_str().unwrap().to_owned();

    let found = settle(&state, &app, &admin, &tok, &[&format!("{tok}-결과.csv")]).await;
    assert_eq!(found[0]["kind"], "file");
    assert_eq!(found[0]["document_id"], doc["id"]);
    settle(&state, &app, &outsider, &tok, &[]).await;

    // 파일을 휴지통에 넣으면 빠진다.
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    settle(&state, &app, &admin, &tok, &[]).await;
    assert!(!indexed_eventually(&state, &tok, &file_id, false).await);
}

/// 항목 기록(D-39 자료 검색): 제목·메모·칸의 글자로 찾고, 그 항목을 볼 수 있는 사람에게만 보인다.
#[tokio::test]
async fn item_records_are_searchable_by_people_who_see_the_item() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let field_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록 검색").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "RSR" })),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 연구" })),
    )
    .await
    .json;
    let record = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/records", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "reference", "title": format!("{tok} 선행연구"),
                     "body": "핵심 요약", "fields": { "summary": format!("요약 {field_tok}") } })),
    )
    .await;
    assert_eq!(record.status, StatusCode::CREATED, "{:?}", record.json);

    let found = settle(&state, &app, &admin, &tok, &[&format!("{tok} 선행연구")]).await;
    assert_eq!(found[0]["kind"], "record");
    assert_eq!(found[0]["item_id"], item["id"]);
    settle(
        &state,
        &app,
        &admin,
        &field_tok,
        &[&format!("{tok} 선행연구")],
    )
    .await;
    // 비공개 항목이라 멤버에게는 보이지 않는다.
    settle(&state, &app, &member, &tok, &[]).await;
    // 검색 서비스가 없을 때의 제목 검색도 기록을 찾는다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&kinds=record")).await),
        [format!("{tok} 선행연구")]
    );
}

/// 감사 델타 SD4: 비공개 항목 안의 문서를 공유로만 보는 사람에게, Meilisearch 경로는 그 항목 id를 돌려주고 모듈로
/// 걸러도 결과를 냈다. 대체(DB) 경로는 같은 요청에 결과를 내지 않아 두 경로가 갈렸다. 문서 조회와 같이 그 항목의
/// 맥락(항목 id·모듈)은 주지 않는다. 항목을 보는 사람은 그대로 항목 id와 모듈 필터를 쓴다. 대체 경로의 모듈 필터가
/// 다른 모듈·위키 문서를 빼는지도 본다(감사 델타 SD8).
#[tokio::test]
async fn a_document_shared_out_of_a_private_item_does_not_reveal_the_item() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공유 모듈").await;
    let (reader, email) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "SHM" }),
        ),
    )
    .await
    .json;
    let m = module["id"].as_str().unwrap().to_owned();
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{m}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 항목" })),
    )
    .await
    .json;
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("공유 문서 {tok}") })),
    )
    .await;
    assert_eq!(doc.status, StatusCode::CREATED, "{:?}", doc.json);
    let doc_id = doc.json["id"].as_str().unwrap().to_owned();
    // D-114: 개별 공유 행에 문서의 조직을 명시한다.
    let organization = common::organization_of_workspace(&ws).await;
    let shared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": email, "permission": "read", "organization_id": organization })),
    )
    .await;
    assert!(shared.status.is_success(), "{:?}", shared.json);
    let wiki_title = format!("위키 문서 {tok}");
    create_doc(&app, &admin, &ws, &wiki_title).await;
    let shared_title = format!("공유 문서 {tok}");
    let filtered = format!("{tok}&workspace_id={ws}&module_id={m}");
    let shared_of = |results: &Value| -> Value {
        results
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["title"] == shared_title.as_str())
            .cloned()
            .unwrap()
    };

    // Meilisearch 경로: 공유받은 사람은 문서를 찾지만 항목 id는 받지 않고, 모듈로 거르면 나오지 않는다.
    let found = settle(
        &state,
        &app,
        &reader,
        &format!("{tok}&kinds=document"),
        &[&shared_title, &wiki_title],
    )
    .await;
    assert!(shared_of(&found)["item_id"].is_null(), "{found}");
    assert!(titles(&search(&app, &reader, &filtered).await).is_empty());
    // 항목을 보는 사람은 그대로다(모듈 밖 위키 문서는 빠진다).
    let mine = settle(&state, &app, &admin, &filtered, &[&shared_title]).await;
    assert_eq!(mine[0]["item_id"], item["id"]);

    // 대체(DB) 경로도 같다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    let plain_found = search(&plain_app, &reader, &format!("{tok}&kinds=document")).await;
    let mut both = [shared_title.clone(), wiki_title];
    both.sort();
    assert_eq!(titles(&plain_found), both);
    assert!(
        shared_of(&plain_found)["item_id"].is_null(),
        "{plain_found}"
    );
    assert!(titles(&search(&plain_app, &reader, &filtered).await).is_empty());
    let plain_mine = search(&plain_app, &admin, &filtered).await;
    assert_eq!(titles(&plain_mine), [shared_title.as_str()]);
    assert_eq!(plain_mine[0]["item_id"], item["id"]);
}
