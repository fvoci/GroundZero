use super::*;
use fvoci_server::ids::UserId;
use std::time::Instant;

const BASELINE: &str = "SELECT m.target_kind, m.target_id, app_time_target_label(m.target_kind, m.target_id),
                    coalesce(t.workspace_id, d.workspace_id, i.workspace_id),
                    coalesce(t.item_id, d.item_id)
             FROM user_marks m
             LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
             LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
             LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
             WHERE m.mark = $1 ORDER BY m.created_at, m.target_id";

type Row = (String, Uuid, Option<String>, Option<Uuid>, Option<Uuid>);

const GROUPED: &str = "SELECT m.mark, m.target_kind, m.target_id, app_time_target_label(m.target_kind, m.target_id),
                    coalesce(t.workspace_id, d.workspace_id, i.workspace_id),
                    coalesce(t.item_id, d.item_id)
             FROM user_marks m
             LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
             LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
             LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
             WHERE m.mark IN ('pin', 'favorite') ORDER BY m.created_at, m.target_id";

type Tagged = (
    String,
    String,
    Uuid,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
);

fn artifact(name: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/ablation-wave2-updated")
        .join(name)
}

fn grouped_sql() -> &'static str {
    GROUPED
}

// Match product row decoding/filtering/splitting; JSON encoding stays outside timing.
fn target(row: Row) -> Option<fvoci_server::features::home::MarkedTarget> {
    use fvoci_server::features::home::{MarkTarget, MarkedTarget};
    let (kind, id, label, workspace_id, item_id) = row;
    let kind = match kind.as_str() {
        "item" => MarkTarget::Item,
        "document" => MarkTarget::Document,
        "task" => MarkTarget::Task,
        _ => return None,
    };
    Some(MarkedTarget {
        kind,
        id,
        label: label?,
        workspace_id: workspace_id.map(fvoci_server::ids::WorkspaceId),
        item_id: item_id.map(fvoci_server::ids::ItemId),
    })
}

async fn stage(
    conn: &mut sqlx::PgConnection,
    variant: bool,
) -> (
    Vec<fvoci_server::features::home::MarkedTarget>,
    Vec<fvoci_server::features::home::MarkedTarget>,
) {
    let mut pins = Vec::new();
    let mut favorites = Vec::new();
    if variant {
        let rows: Vec<Tagged> = sqlx::query_as(&grouped_sql())
            .fetch_all(conn)
            .await
            .unwrap();
        for (mark, kind, id, label, workspace_id, item_id) in rows {
            if let Some(t) = target((kind, id, label, workspace_id, item_id)) {
                match mark.as_str() {
                    "pin" => pins.push(t),
                    "favorite" => favorites.push(t),
                    _ => {}
                }
            }
        }
    } else {
        let rows: Vec<Row> = sqlx::query_as(BASELINE)
            .bind("pin")
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        pins = rows.into_iter().filter_map(target).collect();
        let rows: Vec<Row> = sqlx::query_as(BASELINE)
            .bind("favorite")
            .fetch_all(conn)
            .await
            .unwrap();
        favorites = rows.into_iter().filter_map(target).collect();
    }
    (pins, favorites)
}

async fn read(conn: &mut sqlx::PgConnection, variant: bool) -> (Value, usize) {
    if variant {
        let rows: Vec<Tagged> = sqlx::query_as(&grouped_sql())
            .fetch_all(conn)
            .await
            .unwrap();
        let mut pins = Vec::new();
        let mut favorites = Vec::new();
        let count = rows.len();
        for (mark, kind, id, label, workspace_id, item_id) in rows {
            if let Some(label) = label {
                let target = json!({"kind":kind,"id":id,"label":label,"workspace_id":workspace_id,"item_id":item_id});
                if mark == "pin" {
                    pins.push(target);
                } else {
                    favorites.push(target);
                }
            }
        }
        (json!({"pins":pins,"favorites":favorites}), count)
    } else {
        let mut lists = Vec::new();
        let mut count = 0;
        for mark in ["pin", "favorite"] {
            let rows: Vec<Row> = sqlx::query_as(BASELINE)
                .bind(mark)
                .fetch_all(&mut *conn)
                .await
                .unwrap();
            count += rows.len();
            lists.push(Value::Array(rows.into_iter().filter_map(|(kind,id,label,workspace_id,item_id)| {
                Some(json!({"kind":kind,"id":id,"label":label?,"workspace_id":workspace_id,"item_id":item_id}))
            }).collect()));
        }
        (json!({"pins":lists[0],"favorites":lists[1]}), count)
    }
}

async fn counts(conn: &mut sqlx::PgConnection) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('marks',(SELECT count(*) FROM user_marks),'audit',(SELECT count(*) FROM audit_log),'events',(SELECT count(*) FROM events),'notifications',(SELECT count(*) FROM notifications),'documents',(SELECT count(*) FROM documents),'tasks',(SELECT count(*) FROM tasks),'items',(SELECT count(*) FROM items))")
        .fetch_one(conn).await.unwrap()
}

#[tokio::test]
#[ignore = "baseline-only fixture and output capture; already run before product edit"]
async fn raw_prepare_and_baseline_cost() {
    assert!(include_str!("../../crates/server/src/features/home.rs").contains("WHERE m.mark = $1"));
    let state = common::state();
    let mut users = Vec::new();
    for n in [0, 1, 100] {
        let user = common::login(&state, &format!("marks-n{n}")).await;
        let mut tx = state.db.begin_as(user.id).await.unwrap();
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(tx.conn())
            .await
            .unwrap();
        assert_eq!(isolation, "read committed");
        for index in 0..n {
            let id: Uuid =
                sqlx::query_scalar("SELECT app_create_personal_note($1,$2,1,$3,NULL,32,$4)")
                    .bind(Uuid::now_v7())
                    .bind(format!("synthetic-{index}"))
                    .bind(json!({"type":"doc","content":[]}))
                    .bind(fvoci_server::db::new_ids(3))
                    .fetch_one(tx.conn())
                    .await
                    .unwrap();
            for category in ["pin", "favorite"] {
                sqlx::query("SELECT app_set_mark($1,'document',$2,true)")
                    .bind(category)
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .unwrap();
            }
        }
        tx.commit().await.unwrap();
        users.push(json!({"n":n,"id":user.id.0,"cookie":user.cookie}));
        let mut tx = state.db.begin_as(user.id).await.unwrap();
        let before = counts(tx.conn()).await;
        let (a, rows) = read(tx.conn(), false).await;
        let (b, rows_b) = read(tx.conn(), true).await;
        assert_eq!(a, b);
        assert_eq!(rows, rows_b);
        assert_eq!(rows, n * 2);
        assert_eq!(before, counts(tx.conn()).await);
        for category in ["pin", "favorite"] {
            let plan: Value = sqlx::query_scalar(&format!(
                "EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, FORMAT JSON) {BASELINE}"
            ))
            .bind(category)
            .fetch_one(tx.conn())
            .await
            .unwrap();
            std::fs::write(
                artifact(format!("baseline-n{n}-{category}-plan.json")),
                serde_json::to_vec_pretty(&plan).unwrap(),
            )
            .unwrap();
        }
        let plan: Value = sqlx::query_scalar(&format!(
            "EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, FORMAT JSON) {}",
            grouped_sql().to_owned()
        ))
        .fetch_one(tx.conn())
        .await
        .unwrap();
        std::fs::write(
            artifact(format!("grouped-n{n}-plan.json")),
            serde_json::to_vec_pretty(&plan).unwrap(),
        )
        .unwrap();
        std::fs::write(
            artifact(format!("baseline-n{n}-output.json")),
            serde_json::to_vec_pretty(&a).unwrap(),
        )
        .unwrap();
        tx.commit().await.unwrap();
        let axum::Json(home) = fvoci_server::features::home::home(
            axum::extract::State(state.clone()),
            fvoci_server::features::accounts::CurrentUser(user.id),
            axum::extract::Query(fvoci_server::features::home::HomeQuery {
                tz: "UTC".to_owned(),
            }),
        )
        .await
        .unwrap();
        std::fs::write(
            artifact(format!("baseline-n{n}-home.json")),
            serde_json::to_vec_pretty(&home).unwrap(),
        )
        .unwrap();
        println!(
            "BASELINE n={n} rows={rows} isolation={isolation} exact_grouped_output=true no_effects=true roundtrips=2 grouped_roundtrips=1"
        );
    }
    // Only synthetic fixture identities; cookies deliberately excluded from durable output.
    let ids: Vec<Value> = users
        .into_iter()
        .map(|u| json!({"n":u["n"],"id":u["id"]}))
        .collect();
    std::fs::write(
        artifact("fixtures.json"),
        serde_json::to_vec_pretty(&ids).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn raw_errors_cancel_and_revocation() {
    let state = common::state();
    let app = common::app(&state);
    let owner = common::login(&state, "race-owner").await;
    let (reader, email) = {
        let e = common::unique_email("race-reader");
        (
            common::login_with_email(&state, "race-reader", &e, true).await,
            e,
        )
    };
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&owner),
        Some(json!({"request_id":new_request_id(),"title":"race"})),
    )
    .await
    .json;
    let share = send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/shares", note["id"].as_str().unwrap()),
        Some(&owner),
        Some(json!({"email":email,"permission":"read"})),
    )
    .await;
    assert_eq!(share.status, StatusCode::CREATED, "{:?}", share.json);
    for category in ["pin", "favorite"] {
        mark(&state, &reader, "document", &note["id"], category).await;
    }
    let mut tx = state.db.begin_as(reader.id).await.unwrap();
    let pin: Vec<Row> = sqlx::query_as(BASELINE)
        .bind("pin")
        .fetch_all(tx.conn())
        .await
        .unwrap();
    assert!(pin[0].2.is_some());
    let revoked = send(
        &app,
        Method::DELETE,
        &format!(
            "/api/document-shares/{}",
            share.json["id"].as_str().unwrap()
        ),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT, "{:?}", revoked.json);
    let favorite: Vec<Row> = sqlx::query_as(BASELINE)
        .bind("favorite")
        .fetch_all(tx.conn())
        .await
        .unwrap();
    assert!(favorite[0].2.is_none());
    let (grouped, _) = read(tx.conn(), true).await;
    assert_eq!(grouped, json!({"pins":[],"favorites":[]}));
    tx.commit().await.unwrap();
    // Statement errors propagate and dropping the failed UserTx rolls back.
    for variant in [false, true] {
        let mut tx = state.db.begin_as(reader.id).await.unwrap();
        sqlx::query("SET LOCAL search_path = pg_catalog")
            .execute(tx.conn())
            .await
            .unwrap();
        let q = if variant {
            grouped_sql().to_owned()
        } else {
            BASELINE.replace("$1", "'pin'")
        };
        let err = sqlx::query(&q).fetch_all(tx.conn()).await.unwrap_err();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("42P01")
        );
        drop(tx);
        let mut tx = state.db.begin_as(reader.id).await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(tx.conn())
            .await
            .unwrap();
        let admin = std::env::var("FVOCI_TEST_ADMIN_URL").unwrap();
        let mut ctl = sqlx::PgConnection::connect(&admin).await.unwrap();
        let mut locker =
            sqlx::PgConnection::connect(&std::env::var("FVOCI_TEST_MIGRATION_URL").unwrap())
                .await
                .unwrap();
        sqlx::query("BEGIN").execute(&mut locker).await.unwrap();
        sqlx::query("LOCK TABLE user_marks IN ACCESS EXCLUSIVE MODE")
            .execute(&mut locker)
            .await
            .unwrap();
        let cancel = async {
            loop {
                let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')").bind(pid).fetch_one(&mut ctl).await.unwrap();
                if waiting {
                    break;
                }
                tokio::task::yield_now().await;
            }
            let cancelled: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
                .bind(pid)
                .fetch_one(&mut ctl)
                .await
                .unwrap();
            assert!(cancelled);
        };
        let q = if variant {
            grouped_sql().to_owned()
        } else {
            BASELINE.replace("$1", "'pin'")
        };
        let query = async { sqlx::query(&q).fetch_all(tx.conn()).await.unwrap_err() };
        let (err, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(query, cancel)
        })
        .await
        .unwrap();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("57014")
        );
        drop(tx);
        sqlx::query("ROLLBACK").execute(&mut locker).await.unwrap();
        state.db.ping().await.unwrap();
    }
    println!(
        "RACE baseline can show pin before revoke and omit favorite after revoke; grouped after revoke omits both; authorization checked at each statement snapshot. ERROR 42P01; cancellation 57014; pool reusable after rollback."
    );
}

#[tokio::test]
async fn raw_observe_roundtrips() {
    let fixtures: Vec<Value> =
        serde_json::from_slice(&std::fs::read(artifact("fixtures.json")).unwrap()).unwrap();
    let mut conn = sqlx::PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL").unwrap())
        .await
        .unwrap();
    sqlx::query("SET search_path=fvoci,pg_temp")
        .execute(&mut conn)
        .await
        .unwrap();
    let before = counts(&mut conn).await;
    sqlx::query("SET log_statement='all'")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("SET application_name='wave2-e-marks-observer'")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("SET ROLE fvoci_app")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("BEGIN").execute(&mut conn).await.unwrap();
    sqlx::query("SELECT set_config('app.user_id',$1,true)")
        .bind(fixtures[1]["id"].as_str().unwrap())
        .execute(&mut conn)
        .await
        .unwrap();
    let (a, rows_a) = read(&mut conn, false).await;
    let (b, rows_b) = read(&mut conn, true).await;
    assert_eq!(a, b);
    assert_eq!(rows_a, rows_b);
    sqlx::query("COMMIT").execute(&mut conn).await.unwrap();
    sqlx::query("RESET ROLE").execute(&mut conn).await.unwrap();
    sqlx::query("SET log_statement='none'")
        .execute(&mut conn)
        .await
        .unwrap();
    assert_eq!(before, counts(&mut conn).await);
    println!(
        "Server observer exercised two baseline SELECT executions and one grouped SELECT under fvoci_app, global side effect counts unchanged: {before}"
    );
}

#[tokio::test]
#[ignore = "run only after product variant edit"]
async fn raw_verify_full_home() {
    let state = common::state();
    let fixtures: Vec<Value> =
        serde_json::from_slice(&std::fs::read(artifact("fixtures.json")).unwrap()).unwrap();
    for f in fixtures {
        let user = UserId(f["id"].as_str().unwrap().parse().unwrap());
        let axum::Json(home) = fvoci_server::features::home::home(
            axum::extract::State(state.clone()),
            fvoci_server::features::accounts::CurrentUser(user),
            axum::extract::Query(fvoci_server::features::home::HomeQuery {
                tz: "UTC".to_owned(),
            }),
        )
        .await
        .unwrap();
        let expected: Value = serde_json::from_slice(
            &std::fs::read(artifact(format!("baseline-n{}-home.json", f["n"]))).unwrap(),
        )
        .unwrap();
        assert_eq!(serde_json::to_value(home).unwrap(), expected);
    }
}

#[tokio::test]
async fn raw_current_cost() {
    assert!(include_str!("../../crates/server/src/features/home.rs").contains(GROUPED));
    let state = common::state();
    let fixtures: Vec<Value> = serde_json::from_slice(&std::fs::read(artifact("fixtures.json")).unwrap()).unwrap();
    let mut ctl = sqlx::PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL").unwrap()).await.unwrap();
    sqlx::query("SET search_path=fvoci,pg_temp").execute(&mut ctl).await.unwrap();
    let before = counts(&mut ctl).await;
    let mut cardinalities = Vec::new();
    for f in fixtures {
        let user = UserId(f["id"].as_str().unwrap().parse().unwrap());
        let n = f["n"].as_u64().unwrap();
        let mut tx = state.db.begin_as(user).await.unwrap();
        let (a, rows_a) = read(tx.conn(), false).await;
        let (b, rows_b) = read(tx.conn(), true).await;
        assert_eq!(a,b); assert_eq!(rows_a, rows_b);
        assert_eq!(rows_a, usize::try_from(n).unwrap() * 2);
        for category in ["pin", "favorite"] {
            let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, FORMAT JSON) {BASELINE}"))
                .bind(category).fetch_one(tx.conn()).await.unwrap();
            std::fs::write(artifact(format!("current-baseline-n{n}-{category}-plan.json")), serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
        }
        let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, FORMAT JSON) {GROUPED}"))
            .fetch_one(tx.conn()).await.unwrap();
        std::fs::write(artifact(format!("current-grouped-n{n}-plan.json")), serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
        cardinalities.push(json!({"n":n,"user":user.0,"raw_rows_A":rows_a,"raw_rows_B":rows_b,"json_bytes":serde_json::to_vec(&a).unwrap().len()}));
        tx.commit().await.unwrap();
    }
    assert_eq!(before,counts(&mut ctl).await);
    std::fs::write(artifact("current-cardinality.json"),serde_json::to_vec_pretty(&json!({"global_counts":before,"fixtures":cardinalities})).unwrap()).unwrap();
}

#[tokio::test]
#[ignore = "timing requires coordinator quiet-slot authorization"]
async fn raw_benchmark() {
    assert!(include_str!("../../crates/server/src/features/home.rs").contains(GROUPED));
    let state = common::state();
    let fixtures: Vec<Value> =
        serde_json::from_slice(&std::fs::read(artifact("fixtures.json")).unwrap()).unwrap();
    let mut output = String::from("n,phase,block,position,variant,microseconds,rows,bytes\n");
    for fixture in fixtures {
        let user = UserId(fixture["id"].as_str().unwrap().parse().unwrap());
        let n = fixture["n"].as_u64().unwrap();
        for first in [true, false] {
            for block in 0..if first { 12 } else { 80 } {
                // ABBA / BAAB alternate block order to balance drift and first execution.
                for (position, variant) in if block % 2 == 0 {
                    [false, true, true, false]
                } else {
                    [true, false, false, true]
                }
                .into_iter()
                .enumerate()
                {
                    let mut tx = state.db.begin_as(user).await.unwrap();
                    // Drop driver/server prepared statements; connection establishment is outside the stage.
                    if first {
                        tx.conn().clear_cached_statements().await.unwrap();
                    }
                    let start = Instant::now();
                    let value = stage(tx.conn(), variant).await;
                    let elapsed = start.elapsed().as_secs_f64() * 1_000_000.;
                    tx.commit().await.unwrap();
                    let rows = value.0.len() + value.1.len();
                    assert_eq!(rows, usize::try_from(n).unwrap() * 2);
                    output.push_str(&format!(
                        "{n},{},{block},{position},{},{elapsed:.3},{rows},{}\n",
                        if first { "plan_first" } else { "warm" },
                        if variant { "B" } else { "A" },
                        serde_json::to_vec(&value).unwrap().len()
                    ));
                }
            }
        }
    }
    std::fs::write(artifact("latency.csv"), output).unwrap();
}
