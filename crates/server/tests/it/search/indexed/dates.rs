use super::*;

/// D-101(P-24): 실제 시각(문서의 고친 때)은 보는 사람 시간대의 그날 시작·끝과 비교한다. 서울 10-21 08:30(UTC 10-20)에
/// 고친 문서는 서울의 10-21, 뉴욕의 10-20에 나온다. 뉴욕의 DST 날(2026-03-08은 23시간, 2026-11-01은 25시간)은 그날
/// 처음·마지막 1초가 들고 바로 앞·뒤 1초는 빠진다. 대체 경로도 같다. 시간대를 보내지 않으면 저장한 시간대(D-81)를 쓰고,
/// 모르는 시간대는 400이다.
#[tokio::test]
async fn search_dates_use_the_viewers_day_boundaries() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let (tok, spring, fall) = (token(), token(), token());
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "시간대 경계").await;
    let morning = format!("{tok} 서울 아침");
    doc_changed_at(&app, &admin, &ws, &morning, "2026-10-20T23:30:00Z").await;
    let s = ["앞", "처음", "끝", "뒤"].map(|w| format!("{spring} {w}"));
    for (title, at) in s.iter().zip([
        "2026-03-08T04:59:59Z",
        "2026-03-08T05:00:00Z",
        "2026-03-09T03:59:59Z",
        "2026-03-09T04:00:00Z",
    ]) {
        doc_changed_at(&app, &admin, &ws, title, at).await;
    }
    let f = ["앞", "처음", "끝", "뒤"].map(|w| format!("{fall} {w}"));
    for (title, at) in f.iter().zip([
        "2026-11-01T03:59:59Z",
        "2026-11-01T04:00:00Z",
        "2026-11-02T04:59:59Z",
        "2026-11-02T05:00:00Z",
    ]) {
        doc_changed_at(&app, &admin, &ws, title, at).await;
    }
    settle(&state, &app, &admin, &spring, &[&s[0], &s[1], &s[2], &s[3]]).await;
    settle(&state, &app, &admin, &fall, &[&f[0], &f[1], &f[2], &f[3]]).await;
    settle(&state, &app, &admin, &tok, &[&morning]).await;

    let day = |q: &str, tz: &str, d: &str| format!("{q}&kinds=document&tz={tz}&from={d}&to={d}");
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "Asia/Seoul", "2026-10-21"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "Asia/Seoul", "2026-10-20"),
        &[],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "America/New_York", "2026-10-20"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "America/New_York", "2026-10-21"),
        &[],
    )
    .await;
    // 23시간인 날: [03-08 05:00Z, 03-09 04:00Z).
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&spring, "America/New_York", "2026-03-08"),
        &[&s[1], &s[2]],
    )
    .await;
    // 25시간인 날: [11-01 04:00Z, 11-02 05:00Z).
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&fall, "America/New_York", "2026-11-01"),
        &[&f[1], &f[2]],
    )
    .await;
    // 한쪽 끝만: 시작은 그날 0시부터, 끝은 그날 마지막까지.
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{spring}&kinds=document&tz=America/New_York&from=2026-03-09"),
        &[&s[3]],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{spring}&kinds=document&tz=America/New_York&to=2026-03-08"),
        &[&s[0], &s[1], &s[2]],
    )
    .await;

    // 시간대를 보내지 않으면 저장한 시간대(D-81)로 본다.
    let saved = send(
        &app,
        Method::PUT,
        "/api/me/settings",
        Some(&admin),
        Some(json!({ "time_zone": "America/New_York" })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{:?}", saved.json);
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{tok}&kinds=document&from=2026-10-20&to=2026-10-20"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{tok}&kinds=document&from=2026-10-21&to=2026-10-21"),
        &[],
    )
    .await;

    // 모르는 시간대·시간대가 아닌 이름은 400(두 경로 모두).
    for tz in ["Mars/Olympus", "posix/Asia/Seoul", ""] {
        for router in [&app, &plain] {
            let r = send(
                router,
                Method::GET,
                &format!(
                    "/api/search?q={tok}&from=2026-10-21&tz={tz}&organization_id={}",
                    common::organization_for(&admin).await
                ),
                Some(&admin),
                None,
            )
            .await;
            if tz.is_empty() {
                // 빈 값은 보내지 않은 것과 같다(저장한 시간대).
                assert_eq!(r.status, StatusCode::OK, "{tz}: {:?}", r.json);
            } else {
                assert_eq!(r.status, StatusCode::BAD_REQUEST, "{tz}: {:?}", r.json);
            }
        }
    }
    // 경계 시각을 나타낼 수 없는 날(다음 날이 없는 9999-12-31)은 500이 아니라 400(CodeRabbit #99).
    for router in [&app, &plain] {
        let r = send(
            router,
            Method::GET,
            &format!(
                "/api/search?q={tok}&to=9999-12-31&tz=UTC&organization_id={}",
                common::organization_for(&admin).await
            ),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    }
}

/// D-101(P-24): 날짜만 있는 값(업무 기한, 종일 일정의 시작일, 기록의 발생일)은 시각으로 바꾸지 않아 어느 시간대에서도 같은
/// 날에 나온다. 시각 일정은 일정 자체의 시간대가 아니라 보는 사람 시간대의 날짜로 나온다(감사 델타 SD5: 서울 10-21 08:00
/// 일정은 뉴욕의 달력에서 10-20이다).
#[tokio::test]
async fn date_only_values_keep_their_day_and_timed_events_follow_the_viewer() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "날짜만").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "DTZ" }),
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
    let item_id = item["id"].as_str().unwrap();
    let due = format!("{tok} 기한 업무");
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": due })),
    )
    .await
    .json;
    let updated = send(&app, Method::PATCH, &format!("/api/tasks/{}", task["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": task["version"], "title": due, "description": "", "status": "할 일",
        "priority": "none", "start_date": null, "due_date": "2026-10-21", "estimate_minutes": null, "tags": [],
        "milestone": false, "assignee_ids": [] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let all_day = format!("{tok} 종일 일정");
    let timed = format!("{tok} 시각 일정");
    for body in [
        json!({ "request_id": new_request_id(), "kind": "meeting", "title": all_day, "description": "",
                "all_day": true, "start_date": "2026-10-21", "end_date": "2026-10-21", "starts_at": null,
                "ends_at": null, "timezone": "Asia/Seoul" }),
        // 서울 10-21 08:00 = UTC 10-20 23:00.
        json!({ "request_id": new_request_id(), "kind": "meeting", "title": timed, "description": "",
                "all_day": false, "start_date": null, "end_date": null, "starts_at": "2026-10-20T23:00:00Z",
                "ends_at": "2026-10-21T00:00:00Z", "timezone": "Asia/Seoul" }),
    ] {
        let r = send(
            &app,
            Method::POST,
            &format!("/api/items/{item_id}/events"),
            Some(&admin),
            Some(body),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    }
    let occurred = format!("{tok} 발생 기록");
    let r = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/records"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "reference", "title": occurred,
                     "occurred_on": "2026-10-21", "body": "", "fields": {} }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[&due, &all_day, &timed, &occurred],
    )
    .await;

    let day = |tz: &str, d: &str| format!("{tok}&tz={tz}&from={d}&to={d}");
    // 시각 일정이 10-21인 시간대(서울 08:00, 키리티마티 13:00)와 10-20인 시간대(뉴욕 19:00, 파고파고 12:00).
    for tz in ["Asia/Seoul", "Pacific/Kiritimati"] {
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-21"),
            &[&due, &all_day, &timed, &occurred],
        )
        .await;
        both_paths(&state, &app, &plain, &admin, &day(tz, "2026-10-20"), &[]).await;
    }
    for tz in ["America/New_York", "Pacific/Pago_Pago"] {
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-21"),
            &[&due, &all_day, &occurred],
        )
        .await;
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-20"),
            &[&timed],
        )
        .await;
    }
}

/// D-101: 0시에 여름시간이 끝나 0시가 두 번 있는 날(America/Havana 2026-11-01, 01:00 CDT → 00:00 CST)은 첫 번째 0시
/// (04:00Z)에 시작한다. `날짜::timestamp AT TIME ZONE`만 쓰면 두 번째 0시(05:00Z)를 시작으로 보아 그날 첫 1시간이 앞날에
/// 들어간다(시간 기록의 날 경계와 같은 `app_local_instant`, 감사 S21).
#[tokio::test]
async fn a_day_with_a_repeated_midnight_starts_at_the_first_one() {
    let _queue = SHARED_QUEUE.read().await;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "두 번의 0시").await;
    let before = format!("{tok} 전날 밤");
    let first_hour = format!("{tok} 첫 0시 뒤");
    // 10-31 23:30 CDT, 11-01 00:30 CDT(첫 번째).
    doc_changed_at(&app, &admin, &ws, &before, "2026-11-01T03:30:00Z").await;
    doc_changed_at(&app, &admin, &ws, &first_hour, "2026-11-01T04:30:00Z").await;
    settle(&state, &app, &admin, &tok, &[&before, &first_hour]).await;
    let day = |d: &str| format!("{tok}&kinds=document&tz=America/Havana&from={d}&to={d}");
    both_paths(&state, &app, &plain, &admin, &day("2026-10-31"), &[&before]).await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day("2026-11-01"),
        &[&first_hour],
    )
    .await;
}
