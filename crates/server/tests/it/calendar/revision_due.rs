//! 잘못 저장된 수정 마감일 하나가 다른 날짜·사용자의 읽기나 알림을 막지 않는다(#218).

use crate::common::{self, new_request_id, send};
use crate::sql_support::{World, fixture, ids};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

#[tokio::test]
async fn revision_due_parser_is_strict_and_independent_of_date_style_and_timezone() {
    let state = common::state();
    let mut tx = state.db.begin_anonymous().await.unwrap();
    let properties: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT provolatile='i',proisstrict,proparallel='s',NOT prosecdef
         FROM pg_proc WHERE oid='fvoci.app_iso_date(text)'::regprocedure",
    )
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(properties, (true, true, true, true));
    for (style, zone) in [
        ("ISO, MDY", "UTC"),
        ("SQL, DMY", "Asia/Seoul"),
        ("German, DMY", "America/Los_Angeles"),
        ("Postgres, MDY", "UTC"),
    ] {
        sqlx::query("SELECT set_config('DateStyle',$1,true),set_config('TimeZone',$2,true)")
            .bind(style)
            .bind(zone)
            .execute(tx.conn())
            .await
            .unwrap();
        for (input, expected) in [
            (Some("2026-02-28"), Some("2026-02-28")),
            (Some("2024-02-29"), Some("2024-02-29")),
            (Some("2000-02-29"), Some("2000-02-29")),
            (Some("2026-12-01"), Some("2026-12-01")),
            (Some("1900-02-29"), None),
            (Some("2026-02-31"), None),
            (Some("2026-13-01"), None),
            (Some("2026-01-00"), None),
            (Some("0000-01-01"), None),
            (Some("2026-2-28"), None),
            (Some("tomorrow"), None),
            (Some("infinity"), None),
            (Some(""), None),
            (None, None),
        ] {
            let actual: Option<String> =
                sqlx::query_scalar("SELECT to_char(fvoci.app_iso_date($1),'YYYY-MM-DD')")
                    .bind(input)
                    .fetch_one(tx.conn())
                    .await
                    .unwrap();
            assert_eq!(actual.as_deref(), expected, "{style} {zone}: {input:?}");
        }
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn invalid_revision_due_keeps_calendar_home_and_my_tasks_readable() {
    let w = World::new().await;
    fixture(
        "UPDATE fvoci.modules SET kind='research' WHERE id=$1",
        &[w.module],
    )
    .await;
    let reader = w.member("readonly").await;
    let app = common::app(&w.state);
    let uri = format!("/api/items/{}/records", w.item);
    let mut records = Vec::new();
    for title in ["잘못 저장된 마감일", "정상 마감일"] {
        let record = send(
            &app,
            Method::POST,
            &uri,
            Some(&w.owner),
            Some(json!({
                "request_id": new_request_id(), "kind": "submission", "title": title,
                "occurred_on": "2026-02-20", "body": "",
                "fields": {"status": "submitted", "revision_due": "2026-02-28"}
            })),
        )
        .await;
        assert_eq!(record.status, StatusCode::CREATED, "{}", record.json);
        records.push(record.json);
    }
    let broken = Uuid::parse_str(records[0]["id"].as_str().unwrap()).unwrap();
    let mut fixture = common::fixture_conn().await;
    for invalid in ["2026-02-31", "2026-13-01"] {
        // HTTP 쓰기는 이 값을 거절하므로 저장돼 있던 결함은 SQL로 준비한다.
        sqlx::query("UPDATE fvoci.item_records SET fields=jsonb_set(fields,'{revision_due}',to_jsonb($2::text)) WHERE id=$1")
            .bind(broken).bind(invalid).execute(&mut fixture).await.unwrap();
        for viewer in [&w.owner, &reader] {
            let calendar = send(
                &app,
                Method::GET,
                &format!(
                    "/api/workspaces/{}/calendar?from=2026-02-01&to=2026-03-01&tz=UTC",
                    w.ws
                ),
                Some(viewer),
                None,
            )
            .await;
            assert_eq!(
                calendar.status,
                StatusCode::OK,
                "{invalid}: {}",
                calendar.json
            );
            let dates = calendar.json["dates"].as_array().unwrap();
            assert_eq!(dates.len(), 3, "{invalid}: {dates:?}");
            for record in &records {
                assert!(
                    dates
                        .iter()
                        .any(|d| d["kind"] == "submission" && d["id"] == record["id"])
                );
            }
            let revisions: Vec<_> = dates
                .iter()
                .filter(|d| d["kind"] == "revision_due")
                .collect();
            assert_eq!(revisions.len(), 1);
            assert_eq!(revisions[0]["id"], records[1]["id"]);
            assert_eq!(revisions[0]["date"], "2026-02-28");
            for path in ["home", "tasks"] {
                let reply = send(
                    &app,
                    Method::GET,
                    &format!("/api/me/{path}?organization_id={}&tz=UTC", w.org),
                    Some(viewer),
                    None,
                )
                .await;
                assert_eq!(
                    reply.status,
                    StatusCode::OK,
                    "{invalid} {path}: {}",
                    reply.json
                );
            }
        }
    }
}

#[tokio::test]
async fn invalid_revision_due_keeps_item_reminders_and_their_id_count_working() {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").unwrap();
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    // 알림 함수는 DB 전체를 읽고 쓰므로 다른 시험과 분리한 새 DB에서 부른다.
    let fresh = common::fresh_database(&mut admin, "revision_due").await;
    let mut fixture = PgConnection::connect(fresh.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    let user = Uuid::now_v7();
    let org = Uuid::now_v7();
    let ws = Uuid::now_v7();
    let module = Uuid::now_v7();
    let item = Uuid::now_v7();
    for sql in [
        "INSERT INTO fvoci.users(id,issuer,subject,display_name) VALUES($1,'test','revision_due','reader')",
        "INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($2,'test',$1,$2)",
        "INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($2,$1,'admin')",
        "INSERT INTO fvoci.workspaces(id,organization_id,name,created_by,owner_id,request_id) VALUES($3,$2,'test',$1,$1,$3)",
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($3,$1,'admin')",
        "INSERT INTO fvoci.modules(id,workspace_id,kind,name,created_by,request_id) VALUES($4,$3,'research','test',$1,$4)",
        "INSERT INTO fvoci.items(id,workspace_id,module_id,title,created_by,updated_by,request_id) VALUES($5,$3,$4,'test',$1,$1,$5)",
    ] {
        // CTE를 통해 모든 문장의 동일한 UUID 바인드 집합을 선언한다.
        sqlx::query(&format!(
            "WITH args AS (SELECT $1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid) {sql}"
        ))
        .bind(user)
        .bind(org)
        .bind(ws)
        .bind(module)
        .bind(item)
        .execute(&mut fixture)
        .await
        .unwrap();
    }
    let mut record_ids = Vec::new();
    for due in ["2026-02-31", "2026-13-01", "2026-02-28"] {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO fvoci.item_records(id,item_id,workspace_id,kind,title,occurred_on,fields,created_by,request_id)
            VALUES($1,$2,$3,'submission','test','2026-02-28',jsonb_build_object('status','submitted','revision_due',$5::text),$4,$1)")
            .bind(id).bind(item).bind(ws).bind(user).bind(due)
            .execute(&mut fixture).await.unwrap();
        record_ids.push(id);
    }
    let mut app = PgConnection::connect(&fresh.app_url).await.unwrap();
    let needed = sqlx::query_scalar("SELECT fvoci.app_system_item_reminders_ids('2026-02-28')")
        .fetch_one(&mut app)
        .await;
    let sent = sqlx::query_scalar("SELECT fvoci.app_system_item_reminders('2026-02-28',$1)")
        .bind(ids().into_iter().take(4).collect::<Vec<_>>())
        .fetch_one(&mut app)
        .await;
    assert!(
        needed.is_ok() && sent.is_ok(),
        "id count: {needed:?}; reminders: {sent:?}"
    );
    let needed: i32 = needed.expect("invalid revision_due must not abort the id count");
    let sent: i32 = sent.expect("invalid revision_due must not abort reminders");
    assert_eq!(
        needed, 4,
        "three submission dates plus one valid revision date"
    );
    assert_eq!(sent, needed);
    let notifications: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',record_id,'field',detail->>'field') FROM fvoci.notifications ORDER BY record_id,detail->>'field'")
        .fetch_all(&mut fixture).await.unwrap();
    assert_eq!(notifications.len(), 4);
    for id in &record_ids {
        assert!(notifications.contains(&json!({"id": id, "field": "date"})));
    }
    let revisions: Vec<_> = notifications
        .iter()
        .filter(|n| n["field"] == "revision_due")
        .collect();
    assert_eq!(
        revisions,
        [&json!({"id": record_ids[2], "field": "revision_due"})]
    );
    app.close().await.unwrap();
    fixture.close().await.unwrap();
    sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", fresh.name))
        .execute(&mut admin)
        .await
        .unwrap();
}
