use super::*;
use time::{Date, Month, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

#[tokio::test]
async fn filters_narrow_results_and_attachment_text_is_searchable() {
    let _queue = SHARED_QUEUE.read().await;
    let fixed = [
        "2026-11-01T00:00:00+09:00",
        "2026-10-31T23:59:59+09:00",
        "2026-11-02T00:00:00+09:00",
        "2026-12-31T23:59:59+09:00",
        "2027-01-01T00:00:00+09:00",
    ]
    .map(|at| OffsetDateTime::parse(at, &Rfc3339).unwrap());
    for at in fixed.into_iter().chain([OffsetDateTime::now_utc()]) {
        filters_at(at).await;
    }
}

async fn filters_at(at: OffsetDateTime) {
    // 문서·파일은 기준 시각, 보고서 기한은 그다음 달에 둔다. 어느 실행일에도 날짜 필터는 보고서 하나로 좁혀진다.
    let day = at.to_offset(UtcOffset::from_hms(9, 0, 0).unwrap()).date();
    let from = Date::from_calendar_date(
        day.year() + i32::from(day.month() == Month::December),
        day.month().next(),
        1,
    )
    .unwrap();
    let to = (from + time::Duration::days(32))
        .replace_day(1)
        .unwrap()
        .previous_day()
        .unwrap();
    let due = from + time::Duration::days(4);
    let date_filter = format!("tz=Asia/Seoul&from={from}&to={to}");
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let body_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 필터").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "FLT" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let new_task = |title: String| {
        let app = app.clone();
        let admin = admin.clone();
        let item = item["id"].as_str().unwrap().to_owned();
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/items/{item}/tasks"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "title": title })),
            )
            .await
            .json
        }
    };
    let report = new_task(format!("{tok} 보고서")).await;
    new_task(format!("{tok} 기획")).await;
    let updated = send(&app, Method::PATCH, &format!("/api/tasks/{}", report["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": report["version"], "title": format!("{tok} 보고서"), "description": "", "status": "완료",
        "priority": "none", "start_date": null, "due_date": due.to_string(), "estimate_minutes": null, "tags": [],
        "milestone": false, "assignee_ids": [admin.id.0] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let doc = create_doc(&app, &admin, &ws, &format!("{tok} 회의록")).await;
    upload_bytes(
        &app,
        &admin,
        &doc,
        "계획서.docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        &docx(&format!("예산 {tok} {body_tok} 확정")),
    )
    .await;
    // DB의 시계 대신 이 시나리오의 기준 시각을 넣고, 같은 값을 다시 써 실제 색인 대기열에도 반영한다.
    let mut fixture = common::fixture_conn().await;
    for sql in [
        "UPDATE fvoci.documents SET title = title, updated_at = $2 WHERE id = $1::uuid",
        "UPDATE fvoci.files SET name = name, updated_at = $2 WHERE document_id = $1::uuid",
    ] {
        let result = sqlx::query(sql)
            .bind(doc["id"].as_str().unwrap())
            .bind(at)
            .execute(&mut fixture)
            .await
            .unwrap();
        assert_eq!(result.rows_affected(), 1, "fixture at {at}: {sql}");
    }
    drop(fixture);
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[
            &format!("{tok} 기획"),
            &format!("{tok} 보고서"),
            &format!("{tok} 회의록"),
            "계획서.docx",
        ],
    )
    .await;

    let found = |query: String| {
        let app = app.clone();
        let admin = admin.clone();
        async move { titles(&search(&app, &admin, &query).await) }
    };
    // 업무를 고친 내용(상태·담당·기한)이 색인에 반영될 때까지 기다린다(제목만으로는 알 수 없다).
    let report = format!("{tok} 보고서");
    for filter in ["status_category=done", "assignee=me", &date_filter] {
        settle(&state, &app, &admin, &format!("{tok}&{filter}"), &[&report]).await;
    }
    // 수정 시각을 지우거나 종류로 배제하지 않는다. 그날을 찾으면 문서·첨부가 모두 검색되어야 한다.
    settle(
        &state,
        &app,
        &admin,
        &format!("{tok}&tz=Asia/Seoul&from={day}&to={day}"),
        &[&format!("{tok} 회의록"), "계획서.docx"],
    )
    .await;
    assert_eq!(
        found(format!(
            "{tok}&module_id={}",
            module["id"].as_str().unwrap()
        ))
        .await,
        [format!("{tok} 기획"), format!("{tok} 보고서")]
    );
    // 상태 필터는 범주만 본다(D-116): 세 낱말 밖은 400, 다른 범주는 빈 결과다(이름이 바뀐 상태도 범주로 찾는다).
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&status_category=late&organization_id={}",
            common::organization_for(&admin).await
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    assert!(
        found(format!("{tok}&status_category=doing"))
            .await
            .is_empty()
    );
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&from=2026/11/01&organization_id={}",
            common::organization_for(&admin).await
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 첨부 본문(DOCX)의 낱말로 파일을 찾는다.
    let by_body = settle(&state, &app, &admin, &body_tok, &["계획서.docx"]).await;
    assert_eq!(by_body[0]["kind"], "file");
    assert!(
        by_body[0]["snippet"]
            .as_str()
            .is_some_and(|s| s.contains("예산")),
        "{by_body:?}"
    );

    // PDF 본문의 낱말로도 찾는다(D-73). 깨진 PDF는 이름으로만 색인되고 색인 작업을 멈추지 않는다.
    let pdf_tok = token();
    upload_bytes(
        &app,
        &admin,
        &doc,
        "논문.pdf",
        "application/pdf",
        &pdf(&format!("Rainbow {pdf_tok} outcome")),
    )
    .await;
    upload_bytes(
        &app,
        &admin,
        &doc,
        "깨진.pdf",
        "application/pdf",
        b"%PDF-1.4 broken",
    )
    .await;
    let by_pdf = settle(&state, &app, &admin, &pdf_tok, &["논문.pdf"]).await;
    assert_eq!(by_pdf[0]["kind"], "file");
    settle(&state, &app, &admin, "깨진.pdf", &["깨진.pdf"]).await;

    // 검색 서비스가 없을 때도 같은 필터를 쓴다(제목 검색).
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&{date_filter}")).await),
        [format!("{tok} 보고서")],
        "date filter at {at}"
    );
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&status_category=done")).await),
        [format!("{tok} 보고서")]
    );
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&assignee=me&kinds=task")).await),
        [format!("{tok} 보고서")]
    );
}

/// 작성자·제목만 거르기: 좁히기만 한다. 다른 사람이 만든 것 가운데 볼 수 없는 것(개인 노트)은 작성자로 골라도 나오지 않고,
/// 제목만이면 본문 낱말로는 찾지 않는다. 검색 서비스가 없을 때의 제목 검색도 작성자 조건을 따른다.
#[tokio::test]
async fn author_and_title_only_narrow_results_without_widening_access() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let body_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작성자 검색").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let mine = create_doc(&app, &admin, &ws, &format!("{tok} 관리자 문서")).await;
    let saved = common::live::save(
        &app,
        &admin,
        &format!("/api/documents/{}", mine["id"].as_str().unwrap()),
        &json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": format!("{tok} 관리자 문서"),
                     "body": doc_body(&format!("본문에만 있는 {body_tok}")) }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    create_doc(&app, &writer, &ws, &format!("{tok} 멤버 문서")).await;
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("{tok} 관리자 노트") })),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[
            &format!("{tok} 관리자 노트"),
            &format!("{tok} 관리자 문서"),
            &format!("{tok} 멤버 문서"),
        ],
    )
    .await;
    settle(
        &state,
        &app,
        &admin,
        &body_tok,
        &[&format!("{tok} 관리자 문서")],
    )
    .await;

    let found = |user: TestUser, query: String| {
        let app = app.clone();
        async move { titles(&search(&app, &user, &query).await) }
    };
    assert_eq!(
        found(admin.clone(), format!("{tok}&author=me")).await,
        [format!("{tok} 관리자 노트"), format!("{tok} 관리자 문서")]
    );
    assert_eq!(
        found(admin.clone(), format!("{tok}&author={}", writer.id.0)).await,
        [format!("{tok} 멤버 문서")]
    );
    // 멤버가 관리자를 작성자로 골라도 관리자의 개인 노트는 보이지 않는다(거르기는 좁히기만).
    assert_eq!(
        found(writer.clone(), format!("{tok}&author={}", admin.id.0)).await,
        [format!("{tok} 관리자 문서")]
    );
    // 제목만: 본문 낱말로는 찾지 않고, 제목 낱말로는 그대로 찾는다.
    assert!(
        found(admin.clone(), format!("{body_tok}&title_only=true"))
            .await
            .is_empty()
    );
    assert_eq!(
        found(
            admin.clone(),
            format!("{tok}&title_only=true&author=me&kinds=document")
        )
        .await,
        [format!("{tok} 관리자 노트"), format!("{tok} 관리자 문서")]
    );
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/search?q={tok}&author=someone&organization_id={}",
            common::organization_for(&admin).await
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 검색 서비스가 없을 때(제목 검색)도 같은 작성자 조건.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&author={}", writer.id.0)).await),
        [format!("{tok} 멤버 문서")]
    );
    assert_eq!(
        titles(&search(&plain_app, &writer, &format!("{tok}&author={}", admin.id.0)).await),
        [format!("{tok} 관리자 문서")]
    );
}

/// D-102(P-25): 작성자는 색인 필터로 거른다. 예전에는 후보를 60개씩 5쪽(300개)만 받은 뒤 작성자로 걸러, 다른 사람이 만든
/// 더 높은 순위의 결과가 300개를 넘으면 그 작성자의 결과를 놓쳤다. 작성자 필터가 볼 수 없는 것을 드러내지 않는지도 본다
/// (최종 접근 확인은 지금처럼 서버가 한다, P-14).
#[tokio::test]
async fn an_author_filter_finds_results_beyond_the_old_candidate_window() {
    let _queue = SHARED_QUEUE.read().await;
    const DECOYS: usize = 320;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작성자 창").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (decoy, _) = add_member(&app, &state, &admin, &ws, "decoy", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let target = format!("{tok} 대상 문서");
    let target_id = create_doc(&app, &writer, &ws, &target).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // 제목이 검색어와 꼭 같은 미끼가 대상보다 위에 온다.
    for _ in 0..DECOYS {
        create_doc(&app, &decoy, &ws, &tok).await;
    }
    // 비공개 항목 안의 관리자 업무: 멤버가 관리자를 작성자로 골라도 나오지 않는다.
    let it = private_item(&app, &admin, &ws, "AWN").await;
    let hidden = format!("{tok} 비공개 업무");
    let r = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": hidden })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let admin_doc = format!("{tok} 관리자 문서");
    create_doc(&app, &admin, &ws, &admin_doc).await;

    // 모두 색인된 뒤, 작성자 없이 받은 색인 순위에서 대상이 옛 후보 창(300개) 밖에 있는지 먼저 확인한다.
    let total = DECOYS + 3;
    let mut ranked = Vec::new();
    for _ in 0..100 {
        while drain(&state.db, client(&state), Some(&state.storage))
            .await
            .unwrap()
            > 0
        {}
        ranked = client(&state)
            .query(&tok, &format!("workspace_id = \"{ws}\""), 0, 1000, false)
            .await
            .unwrap();
        if ranked.len() == total {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(ranked.len(), total, "everything is indexed");
    let rank = ranked.iter().position(|h| h.id == target_id).unwrap();
    assert!(
        rank >= 300,
        "the target ranks {rank}, inside the old window"
    );

    let by = |author: &TestUser| format!("{tok}&author={}", author.id.0);
    assert_eq!(
        titles(&search(&app, &admin, &by(&writer)).await),
        std::slice::from_ref(&target)
    );
    assert_eq!(
        titles(&search(&plain, &admin, &by(&writer)).await),
        std::slice::from_ref(&target)
    );
    // 작성자 필터는 좁히기만 한다: 멤버는 관리자의 비공개 항목 업무를 보지 못하고, 관리자는 본다.
    for router in [&app, &plain] {
        assert_eq!(
            titles(&search(router, &reader, &by(&admin)).await),
            std::slice::from_ref(&admin_doc)
        );
        let mut mine = vec![admin_doc.clone(), hidden.clone()];
        mine.sort();
        assert_eq!(titles(&search(router, &admin, &by(&admin)).await), mine);
    }
}
