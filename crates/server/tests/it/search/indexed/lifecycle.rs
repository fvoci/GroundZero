use super::*;

#[tokio::test]
async fn trashed_content_leaves_the_index_and_returns_on_restore() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 휴지통").await;
    let doc = create_doc(&app, &admin, &ws, &format!("지울 문서 {tok}")).await;
    let id = doc["id"].as_str().unwrap();
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(indexed_eventually(&state, &tok, id, true).await);

    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let batch = r.json["batch_id"].as_str().unwrap().to_owned();
    // 거르기 덕분에 색인 반영 전에도 안 보이고, 반영 뒤에는 색인에서도 빠진다.
    assert_eq!(
        titles(&search(&app, &admin, &tok).await),
        Vec::<String>::new()
    );
    settle(&state, &app, &admin, &tok, &[]).await;
    assert!(!indexed_eventually(&state, &tok, id, false).await);

    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash-batches/{batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(indexed_eventually(&state, &tok, id, true).await);

    // 항목을 지우면 그 안의 문서·업무도 색인에서 빠지고, 되살리면 함께 돌아온다.
    let it = private_item(&app, &admin, &ws, "SRT").await;
    let item_id = it["id"].as_str().unwrap();
    let inner = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("항목 문서 {tok}") })),
    )
    .await;
    assert_eq!(inner.status, StatusCode::CREATED, "{:?}", inner.json);
    let inner_id = inner.json["id"].as_str().unwrap().to_owned();
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("항목 업무 {tok}") })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let task_id = task.json["id"].as_str().unwrap().to_owned();
    let all = [
        format!("지울 문서 {tok}"),
        format!("항목 문서 {tok}"),
        format!("항목 업무 {tok}"),
    ];
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    settle(&state, &app, &admin, &tok, &all).await;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let item_batch = r.json["batch_id"].as_str().unwrap().to_owned();
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(!indexed_eventually(&state, &tok, &inner_id, false).await);
    assert!(!indexed_eventually(&state, &tok, &task_id, false).await);
    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash-batches/{item_batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    settle(&state, &app, &admin, &tok, &all).await;
    assert!(indexed_eventually(&state, &tok, &inner_id, true).await);
}

/// 감사 델타 SD2: 항목 정보(상태·목표 등)나 템플릿 표시만 바꾸면 `updated_at`은 바뀌어도 다시 색인되지 않아,
/// Meilisearch 경로의 항목 날짜가 옛 날짜로 남았다(대체 경로는 지금 날짜). 이제 다시 색인한다.
#[tokio::test]
async fn changing_an_item_profile_reindexes_its_day() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 날짜").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "IDY" }),
        ),
    )
    .await
    .json;
    let title = format!("{tok} 항목");
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await
    .json;
    let id = item["id"].as_str().unwrap().to_owned();
    // 지난날 고친 항목으로 만든다(제목을 다시 써 색인한다).
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    sqlx::query(
        "UPDATE fvoci.items SET title = title, updated_at = '2026-09-01T03:00:00Z' WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    settle(
        &state,
        &app,
        &admin,
        &format!("{tok}&kinds=item&from=2026-09-01&to=2026-09-01"),
        &[&title],
    )
    .await;

    let profile = send(
        &app,
        Method::PUT,
        &format!("/api/items/{id}/profile"),
        Some(&admin),
        Some(json!({ "base_version": 1, "leader_id": null, "goal": "목표", "scope": "", "starts_on": null, "ends_on": null,
                     "status": "active", "details": {} })),
    )
    .await;
    assert_eq!(profile.status, StatusCode::NO_CONTENT, "{:?}", profile.json);
    let (from, to) = around_today(&state, &admin).await;
    settle(
        &state,
        &app,
        &admin,
        &format!("{tok}&kinds=item&from={from}&to={to}"),
        &[&title],
    )
    .await;
}

/// 감사 델타 SD8: 기록의 날짜는 발생일이 있으면 발생일, 없으면 고친 날이다. 이 규칙은 색인(payload)과 대체 SQL에 따로
/// 있는데 시험이 없어, 한쪽이 바뀌어도 시험이 통과했다. 두 경로에서 같은 날짜 필터로 확인한다.
#[tokio::test]
async fn record_dates_follow_the_occurred_day_then_the_change_day() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록 날짜").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "RDT" })),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "연구" })),
    )
    .await
    .json;
    let uri = format!("/api/items/{}/records", item["id"].as_str().unwrap());
    let occurred = format!("{tok} 발생일");
    let changed = format!("{tok} 고친 날");
    for (title, day) in [(&occurred, json!("2026-03-15")), (&changed, Value::Null)] {
        let r = send(
            &app,
            Method::POST,
            &uri,
            Some(&admin),
            Some(
                json!({ "request_id": new_request_id(), "kind": "reference", "title": title,
                         "occurred_on": day, "body": "", "fields": {} }),
            ),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    }
    let march = format!("{tok}&kinds=record&from=2026-03-01&to=2026-03-31");
    let (from, to) = around_today(&state, &admin).await;
    let now = format!("{tok}&kinds=record&from={from}&to={to}");
    settle(&state, &app, &admin, &march, &[&occurred]).await;
    settle(&state, &app, &admin, &now, &[&changed]).await;

    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &march).await),
        [occurred]
    );
    assert_eq!(titles(&search(&plain_app, &admin, &now).await), [changed]);
}

#[tokio::test]
async fn an_unreachable_search_service_falls_back_to_titles() {
    let _queue = SHARED_QUEUE.write().await;
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_SEARCH_URL", "http://127.0.0.1:9"),
        ("FVOCI_SEARCH_KEY", "unused-test-key"),
    ]))
    .unwrap();
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 대신").await;
    create_doc(&app, &admin, &ws, &format!("제목만 {tok}")).await;
    assert_eq!(
        titles(&search(&app, &admin, &tok).await),
        [format!("제목만 {tok}")]
    );
    // 검색 서비스가 없어도 다른 주기 작업은 계속 돈다.
    let report =
        fvoci_server::worker::run_once(&state.db, state.search.as_ref(), Some(&state.storage))
            .await;
    assert_eq!(report.indexed, 0);
}
