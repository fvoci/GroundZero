#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use common::TestUser;
use fvoci_server::http::AppState;
use serde_json::json;
use sql_support::*;
use sqlx::{Connection, PgConnection};
use std::time::Duration;
use uuid::Uuid;

const RESTORE: &str = "SELECT fvoci.app_restore_batch($1,@ids)";
const DELETE_STATUS: &str = "SELECT fvoci.app_delete_task_status($1,NULL,@ids)";
const START: &str = "SELECT * FROM fvoci.app_start_timer($1,'work','','task',$2,@ids)";
const WIKI: &str = "SELECT fvoci.app_create_wiki_document($1,$2,'review',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id";
const UPLOAD_DOCUMENT: &str = "SELECT fvoci.app_create_upload($1,NULL,'document',$2,'review.txt','','text/plain',10,10,3600,1000,100000,@ids) AS id";

async fn verified_user(state: &AppState, label: &str) -> TestUser {
    common::login_with_email(state, label, &common::unique_email(label), true).await
}

async fn finish_upload(state: &AppState, user: &TestUser, upload: Uuid) -> Uuid {
    // 실제 바이트 저장은 SQL 계약 밖이다. 기존 attachment 헬퍼와 같은 완료 준비다.
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[upload],
    )
    .await;
    rows(
        state,
        user,
        "SELECT fvoci.app_finish_upload($1,repeat('a',64),100000,@ids)",
        &[upload],
    )
    .await;
    made(state, user, "SELECT file_id AS id FROM fvoci.file_versions WHERE id=(SELECT version_id FROM fvoci.uploads WHERE id=$1)", &[upload]).await
}

async fn upload_document(state: &AppState, user: &TestUser, doc: Uuid) -> Uuid {
    let upload = made(state, user, UPLOAD_DOCUMENT, &[Uuid::now_v7(), doc]).await;
    finish_upload(state, user, upload).await
}

async fn move_document(conn: &mut PgConnection, doc: Uuid, ws: Uuid) -> Result<(), sqlx::Error> {
    on(
        conn,
        "SELECT * FROM fvoci.app_move_preview($1,$2,NULL)",
        &[doc, ws],
    )
    .await?;
    let count: i32 = sqlx::query_scalar("SELECT fvoci.app_move_document_ids($1)")
        .bind(doc)
        .fetch_one(&mut *conn)
        .await?;
    let ids: Vec<_> = (0..count).map(|_| Uuid::now_v7()).collect();
    sqlx::query("SELECT * FROM fvoci.app_move_document($1,$2,NULL,$3)")
        .bind(doc)
        .bind(ws)
        .bind(ids)
        .execute(conn)
        .await?;
    Ok(())
}

async fn completed<T>(pending: tokio::task::JoinHandle<T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), pending)
        .await
        .expect("concurrent operation did not finish")
        .unwrap()
}

fn error_code(error: &sqlx::Error) -> String {
    error
        .as_database_error()
        .unwrap()
        .code()
        .unwrap()
        .into_owned()
}

#[tokio::test]
async fn shares_from_another_organization_stop_when_a_note_moves() {
    let state = common::state();
    let admin = common::login(&state, "review server admin").await;
    common::make_server_admin(&admin).await;
    let owner = common::login(&state, "review note owner").await;
    let mut orgs = Vec::new();
    for _ in 0..2 {
        let org = made(
            &state,
            &admin,
            "SELECT fvoci.app_create_organization($1,'review organization',$2,@ids) AS id",
            &[Uuid::now_v7(), admin.id.0],
        )
        .await;
        common::join_organization(org, &owner, "member").await;
        orgs.push(org);
    }
    let x = verified_user(&state, "review X").await;
    let y = verified_user(&state, "review Y").await;
    common::join_organization(orgs[0], &x, "member").await;
    common::join_organization(orgs[1], &y, "member").await;
    let ws = workspace(&state, &owner, orgs[1]).await;
    let doc = made(&state, &owner,
        "SELECT fvoci.app_create_personal_note($1,'review note',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",
        &[Uuid::now_v7()]).await;
    for (user, org) in [(&x, orgs[0]), (&y, orgs[1])] {
        rows(&state, &owner,
            "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids)",
            &[doc, user.id.0, org]).await;
        assert_eq!(
            one(
                &state,
                user,
                "SELECT fvoci.app_access('document',$1) AS grade",
                &[doc]
            )
            .await["grade"],
            "read"
        );
    }
    let mut tx = state.db.begin_as(owner.id).await.unwrap();
    move_document(tx.conn(), doc, ws).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        one(
            &state,
            &x,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "none"
    );
    assert!(
        rows(
            &state,
            &x,
            "SELECT id FROM fvoci.documents WHERE id=$1",
            &[doc]
        )
        .await
        .is_empty()
    );
    assert_eq!(
        one(
            &state,
            &y,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "read"
    );
    let shares = rows(
        &state,
        &owner,
        "SELECT grantee_user_id,organization_id FROM fvoci.document_shares WHERE document_id=$1",
        &[doc],
    )
    .await;
    assert_eq!(
        shares,
        vec![json!({"grantee_user_id":y.id.0,"organization_id":orgs[1]})]
    );
    assert_eq!(rows(&state, &owner,
        "SELECT id FROM fvoci.activity WHERE target_kind='document' AND target_id=$1 AND kind='unshare' AND detail->>'reason'='moved' AND detail->>'user'=$2::text",
        &[doc, x.id.0]).await.len(), 1);
}

#[tokio::test]
async fn restore_is_refused_when_the_deleter_loses_the_role_while_waiting() {
    let w = World::new().await;
    let user = w.member("member").await;
    let doc = w.doc().await;
    let file = upload_document(&w.state, &user, doc).await;
    let batch = made(
        &w.state,
        &user,
        "SELECT fvoci.app_trash_file($1,@ids) AS id",
        &[file],
    )
    .await;
    let mut holder = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        holder.conn(),
        "SELECT 1 FROM fvoci.documents WHERE id=$1 FOR UPDATE",
        &[doc],
    )
    .await
    .unwrap();
    let holder_pid = pid(holder.conn()).await;
    let mut restore = w.state.db.begin_as(user.id).await.unwrap();
    let waiter_pid = pid(restore.conn()).await;
    let pending = tokio::spawn(async move {
        let result = on(restore.conn(), RESTORE, &[batch]).await;
        if result.is_ok() {
            restore.commit().await.unwrap();
        }
        result
    });
    blocked(holder_pid, waiter_pid).await;
    let mut demote = w.state.db.begin_as(w.owner.id).await.unwrap();
    let n: i32 = sqlx::query_scalar("SELECT fvoci.app_set_member_role_ids($1,$2)")
        .bind(w.ws)
        .bind(user.id.0)
        .fetch_one(demote.conn())
        .await
        .unwrap();
    sqlx::query("SELECT fvoci.app_set_member_role($1,$2,'readonly',$3)")
        .bind(w.ws)
        .bind(user.id.0)
        .bind((0..n).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
        .execute(demote.conn())
        .await
        .unwrap();
    demote.commit().await.unwrap();
    holder.commit().await.unwrap();
    let error = completed(pending).await.unwrap_err();
    assert!(
        matches!(error_code(&error).as_str(), "P0002" | "42501"),
        "{error}"
    );
    let mut observer = common::fixture_conn().await;
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT deleted_at IS NOT NULL FROM fvoci.files WHERE id=$1")
            .bind(file)
            .fetch_one(&mut observer)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_timer_started_in_an_older_transaction_switches_without_violating_order() {
    let w = World::new().await;
    let a = w.task().await;
    let b = w.task().await;
    let mut older = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(older.conn(), "SELECT now()", &[]).await.unwrap();
    let first = one(&w.state, &w.owner, START, &[Uuid::now_v7(), a]).await;
    let old = Uuid::parse_str(first["entry_id"].as_str().unwrap()).unwrap();
    assert!(first["stopped_entry_id"].is_null());
    let switched = on(older.conn(), START, &[Uuid::now_v7(), b]).await.unwrap();
    assert_eq!(switched[0]["stopped_entry_id"], old.to_string());
    let new = Uuid::parse_str(switched[0]["entry_id"].as_str().unwrap()).unwrap();
    older.commit().await.unwrap();
    let order = one(&w.state, &w.owner,
        "SELECT a.ended_at >= a.started_at AS old_order, b.started_at >= a.ended_at AS switch_order, b.ended_at IS NULL AS running FROM fvoci.time_entries a CROSS JOIN fvoci.time_entries b WHERE a.id=$1 AND b.id=$2",
        &[old, new]).await;
    assert_eq!(
        order,
        json!({"old_order":true,"switch_order":true,"running":true})
    );
}

async fn reference_document(
    state: &AppState,
    user: &TestUser,
    ws: Option<Uuid>,
    file: Uuid,
) -> Uuid {
    let body = json!({"type":"doc","content":[{"type":"image","attrs":{"src":format!("/api/files/{file}/content?version=1")}}]});
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let doc: Uuid = if let Some(ws) = ws {
        sqlx::query_scalar(
            "SELECT fvoci.app_create_wiki_document($1,$2,'reference',1,$3,NULL,20,$4)",
        )
        .bind(ws)
        .bind(Uuid::now_v7())
        .bind(body)
        .bind(ids())
        .fetch_one(tx.conn())
        .await
        .unwrap()
    } else {
        sqlx::query_scalar("SELECT fvoci.app_create_personal_note($1,'reference',1,$2,NULL,20,$3)")
            .bind(Uuid::now_v7())
            .bind(body)
            .bind(ids())
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    tx.commit().await.unwrap();
    doc
}

async fn reference_state(doc: Uuid, file: Uuid) -> (i64, bool) {
    // 참조 색인과 보존 함수는 내부 계약이라 마이그레이션 역할로 관찰한다.
    let mut observer = common::fixture_conn().await;
    sqlx::query_as("SELECT (SELECT count(*) FROM fvoci.document_file_refs WHERE document_id=$1 AND file_id=$2 AND version=1), fvoci.app_version_referenced($2,1,(SELECT id FROM fvoci.file_versions WHERE file_id=$2 AND version=1))")
        .bind(doc).bind(file).fetch_one(&mut observer).await.unwrap()
}

#[tokio::test]
async fn file_versions_referenced_across_the_organization_are_kept() {
    let w = World::new().await; // 파일이 있는 B.
    let a = workspace(&w.state, &w.owner, w.org).await;
    let member = verified_user(&w.state, "review org member").await;
    common::join_organization(w.org, &member, "member").await;
    let outsider = verified_user(&w.state, "review outsider").await;
    // 두 범위를 독립적으로 검증한다. 앞 문서의 참조가 개인 노트 보존의 실패를 가리지 않는다.
    for ws in [Some(a), None] {
        let upload = made(&w.state, &w.owner,
            "SELECT fvoci.app_create_upload($1,NULL,'item',$2,'version.txt','','text/plain',10,10,3600,1000,100000,@ids) AS id",
            &[Uuid::now_v7(), w.item]).await;
        let file = finish_upload(&w.state, &w.owner, upload).await;
        let user = if ws.is_some() { &w.owner } else { &member };
        let doc = reference_document(&w.state, user, ws, file).await;
        assert_eq!(reference_state(doc, file).await, (1, true));
        // 파일 열람 거부는 앱 역할 SELECT로, 범위 밖 노드의 미기록은 내부 색인으로 따로 확인한다.
        assert!(
            rows(
                &w.state,
                &outsider,
                "SELECT id FROM fvoci.files WHERE id=$1",
                &[file]
            )
            .await
            .is_empty()
        );
        if ws.is_none() {
            assert!(
                rows(
                    &w.state,
                    &member,
                    "SELECT id FROM fvoci.files WHERE id=$1",
                    &[file]
                )
                .await
                .is_empty()
            );
        }
        let outside_doc = reference_document(&w.state, &outsider, None, file).await;
        assert_eq!(reference_state(outside_doc, file).await.0, 0);
    }
}

#[tokio::test]
async fn purge_does_not_swallow_short_ids() {
    // 시스템 purge는 DB 전체를 순회하므로 이 시험만의 DB를 쓴다(다른 시험의 만료 행을 건드리지 않는다).
    let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let fresh = common::fresh_database(&mut admin, "review_purge").await;
    let state =
        fvoci_server::app::build_state(common::config_with(&[("DATABASE_URL", &fresh.app_url)]))
            .unwrap();
    let owner = common::login(&state, "review purge owner").await;
    let mut migration = PgConnection::connect(fresh.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    sqlx::query("INSERT INTO fvoci.server_admins(user_id,detail) VALUES($1,'{\"seed\":\"test\"}')")
        .bind(owner.id.0)
        .execute(&mut migration)
        .await
        .unwrap();
    let org = made(
        &state,
        &owner,
        "SELECT fvoci.app_create_organization($1,'purge organization',$2,@ids) AS id",
        &[Uuid::now_v7(), owner.id.0],
    )
    .await;
    let ws = workspace(&state, &owner, org).await;
    let mut batches = Vec::new();
    for _ in 0..2 {
        let doc = made(&state, &owner, WIKI, &[ws, Uuid::now_v7()]).await;
        batches.push(
            made(
                &state,
                &owner,
                "SELECT fvoci.app_trash_document($1,@ids) AS id",
                &[doc],
            )
            .await,
        );
    }
    sqlx::query(
        "UPDATE fvoci.trash_batches SET deleted_at=now()-interval '1000 days' WHERE id=ANY($1)",
    )
    .bind(&batches)
    .execute(&mut migration)
    .await
    .unwrap();
    let mut tx = migration.begin().await.unwrap();
    let error = sqlx::query_scalar::<_, i32>("SELECT fvoci.app_system_purge_expired($1)")
        .bind(vec![Uuid::now_v7()])
        .fetch_one(&mut *tx)
        .await
        .unwrap_err();
    code(&error, "FV001");
    tx.rollback().await.unwrap();
    let intact: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.trash_batches WHERE id=ANY($1) AND purged_at IS NULL",
    )
    .bind(&batches)
    .fetch_one(&mut migration)
    .await
    .unwrap();
    assert_eq!(intact, 2);
    let n: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_expired_ids()")
        .fetch_one(&mut migration)
        .await
        .unwrap();
    assert_eq!(n, 2);
    let purged: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_expired($1)")
        .bind((0..n).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
        .fetch_one(&mut migration)
        .await
        .unwrap();
    assert_eq!(purged, 2);
    let gone: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.trash_batches WHERE id=ANY($1) AND purged_at IS NOT NULL",
    )
    .bind(&batches)
    .fetch_one(&mut migration)
    .await
    .unwrap();
    assert_eq!(gone, 2);
    let checksum: String = sqlx::query_scalar(
        "SELECT encode(checksum,'hex') FROM fvoci._sqlx_migrations WHERE version=202610250001",
    )
    .fetch_one(&mut migration)
    .await
    .unwrap();
    eprintln!("review migration checksum: {checksum}");
    migration.close().await.unwrap();
    state.db.close().await;
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fresh.name))
        .execute(&mut admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_last_status_survives_concurrent_deletes() {
    let w = World::new().await;
    let statuses = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.app_task_statuses($1)",
        &[w.item],
    )
    .await;
    assert_eq!(statuses.len(), 3);
    rows(&w.state, &w.owner, DELETE_STATUS, &[id(&statuses[0])]).await;
    let mut first = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(first.conn(), DELETE_STATUS, &[id(&statuses[1])])
        .await
        .unwrap();
    let holder = pid(first.conn()).await;
    let mut second = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(second.conn()).await;
    let last = id(&statuses[2]);
    let pending = tokio::spawn(async move {
        let result = on(second.conn(), DELETE_STATUS, &[last]).await;
        if result.is_ok() {
            second.commit().await.unwrap();
        }
        result
    });
    blocked(holder, waiter).await;
    let mut observer = common::fixture_conn().await;
    let advisory: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND locktype='advisory' AND NOT granted)")
        .bind(waiter).fetch_one(&mut observer).await.unwrap();
    assert!(advisory);
    first.commit().await.unwrap();
    code(&completed(pending).await.unwrap_err(), "55000");
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.app_task_statuses($1)",
            &[w.item]
        )
        .await,
        vec![json!({"id":last})]
    );
}

#[tokio::test]
async fn archived_items_answer_409_before_403_for_everyone_who_can_see() {
    let w = World::new().await;
    let reader = w.member("readonly").await;
    let shared = verified_user(&w.state, "review shared writer").await;
    common::join_organization(w.org, &shared, "member").await;
    let public_doc = w.doc().await;
    let private_doc = w.doc().await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_document_settings($1,'private',true,@ids)",
        &[private_doc],
    )
    .await;
    w.share(private_doc, &shared, "write").await;
    let public_file = upload_document(&w.state, &w.owner, public_doc).await;
    let private_file = upload_document(&w.state, &w.owner, private_doc).await;
    let task = w.task().await;
    assert!(
        rows(
            &w.state,
            &shared,
            "SELECT id FROM fvoci.items WHERE id=$1",
            &[w.item]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &shared,
            "SELECT id FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await
        .is_empty()
    );
    let mut failures = Vec::new();
    for archived in [true, false] {
        let sql = if archived {
            "SELECT fvoci.app_archive_item($1,true,@ids)"
        } else {
            "SELECT fvoci.app_archive_item($1,false,@ids)"
        };
        rows(&w.state, &w.owner, sql, &[w.item]).await;
        for (user, doc, file) in [
            (&reader, public_doc, public_file),
            (&shared, private_doc, private_file),
        ] {
            for (sql, target) in [
                ("SELECT fvoci.app_require_document($1,'write')", doc),
                (UPDATE_TASK, task),
                ("SELECT fvoci.app_rename_file($1,'renamed.txt',@ids)", file),
                ("SELECT fvoci.app_trash_file($1,@ids)", file),
            ] {
                // 내부 require 함수는 앱 EXECUTE가 없으므로 공개 변경 경로를 쓴다.
                // 문서 공유만으로 항목·업무가 공개되지는 않는다. 그 업무는 보관 여부와 무관하게 404다.
                let expected = if user.id == shared.id && target == task {
                    "P0002"
                } else if archived {
                    "55000"
                } else if user.id == reader.id {
                    "42501"
                } else {
                    "OK"
                };
                let actual = match attempt(&w.state, user, sql, &[target]).await {
                    Ok(_) => "OK".to_owned(),
                    Err(error) => format!(
                        "{}: {}",
                        error_code(&error),
                        error.as_database_error().unwrap().message()
                    ),
                };
                if actual.split(':').next().unwrap() != expected {
                    failures.push(format!("archived={archived}, shared={}, {sql}: expected {expected}, actual {actual}", user.id == shared.id));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn organization_concurrent_upload_limit_counts_every_workspace() {
    let w = World::new().await;
    let b = workspace(&w.state, &w.owner, w.org).await;
    let a_doc = w.doc().await;
    let b_doc = made(&w.state, &w.owner, WIKI, &[b, Uuid::now_v7()]).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_settings($1,'{\"max_concurrent_uploads\":1}',@ids)",
        &[w.org],
    )
    .await;
    let first = made(
        &w.state,
        &w.owner,
        UPLOAD_DOCUMENT,
        &[Uuid::now_v7(), a_doc],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        UPLOAD_DOCUMENT,
        &[Uuid::now_v7(), b_doc],
        "55000",
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_abort_upload($1)",
        &[first],
    )
    .await;
    let second = made(
        &w.state,
        &w.owner,
        UPLOAD_DOCUMENT,
        &[Uuid::now_v7(), b_doc],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_abort_upload($1)",
        &[second],
    )
    .await;
}

#[tokio::test]
async fn restore_and_move_do_not_deadlock_in_either_order() {
    for restore_first in [false, true] {
        let w = World::new().await;
        let b = workspace(&w.state, &w.owner, w.org).await;
        let doc = w.doc().await;
        let file = upload_document(&w.state, &w.owner, doc).await;
        let batch = made(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_trash_file($1,@ids) AS id",
            &[file],
        )
        .await;
        let mut moving = w.state.db.begin_as(w.owner.id).await.unwrap();
        let mut restoring = w.state.db.begin_as(w.owner.id).await.unwrap();
        let outcome = if restore_first {
            on(restoring.conn(), RESTORE, &[batch]).await.unwrap();
            let holder = pid(restoring.conn()).await;
            let waiter = pid(moving.conn()).await;
            let pending = tokio::spawn(async move {
                move_document(moving.conn(), doc, b).await.unwrap();
                moving.commit().await.unwrap();
            });
            blocked(holder, waiter).await;
            restoring.commit().await.unwrap();
            completed(pending).await;
            "OK".to_owned()
        } else {
            on(
                moving.conn(),
                "SELECT 1 FROM fvoci.documents WHERE id=$1 FOR UPDATE",
                &[doc],
            )
            .await
            .unwrap();
            let holder = pid(moving.conn()).await;
            let waiter = pid(restoring.conn()).await;
            let pending = tokio::spawn(async move {
                let result = on(restoring.conn(), RESTORE, &[batch]).await;
                if result.is_ok() {
                    restoring.commit().await.unwrap();
                }
                result
            });
            blocked(holder, waiter).await;
            tokio::time::timeout(
                Duration::from_secs(10),
                move_document(moving.conn(), doc, b),
            )
            .await
            .unwrap()
            .unwrap();
            moving.commit().await.unwrap();
            match completed(pending).await {
                Ok(_) => "OK".to_owned(),
                Err(error) => {
                    let code = error_code(&error);
                    assert!(matches!(code.as_str(), "55000" | "P0002"), "{error}");
                    code
                }
            }
        };
        eprintln!("restore/move: restore_first={restore_first}, restore={outcome}, move=OK");
        assert_eq!(
            one(
                &w.state,
                &w.owner,
                "SELECT workspace_id FROM fvoci.documents WHERE id=$1",
                &[doc]
            )
            .await["workspace_id"],
            b.to_string()
        );
        let mut observer = common::fixture_conn().await;
        let (ws, live): (Uuid, bool) =
            sqlx::query_as("SELECT workspace_id,deleted_at IS NULL FROM fvoci.files WHERE id=$1")
                .bind(file)
                .fetch_one(&mut observer)
                .await
                .unwrap();
        assert_eq!(ws, b);
        assert_eq!(live, outcome == "OK");
    }
}
