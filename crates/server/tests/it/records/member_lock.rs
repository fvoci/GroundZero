//! D-112 SR1-14·D-113 SR1-11: 멤버 회수와 기록 쓰기는 공간·현재 멤버 행에서 줄 선다.

use crate::{common, sql_support::*};
use common::TestUser;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use std::time::Duration;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type PendingCommand = tokio::task::JoinHandle<(fvoci_db::UserTx, Result<Vec<Value>, sqlx::Error>)>;

struct Command {
    sql: &'static str,
    args: Vec<Uuid>,
    personal: bool,
}

impl Command {
    fn shared(sql: &'static str, args: Vec<Uuid>) -> Self {
        Self {
            sql,
            args,
            personal: false,
        }
    }

    fn personal(sql: &'static str, args: Vec<Uuid>) -> Self {
        Self {
            sql,
            args,
            personal: true,
        }
    }
}

async fn commands(w: &World, user: &TestUser, kind: &str) -> Vec<Command> {
    match kind {
        "course" => {
            rows(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_add_episodes($1,jsonb_build_array(jsonb_build_object('title','original','request_id',$2)),@ids)",
                &[w.item, Uuid::now_v7()],
            )
            .await;
            let episode = made(
                &w.state,
                &w.owner,
                "SELECT id FROM fvoci.course_episodes WHERE item_id=$1",
                &[w.item],
            )
            .await;
            vec![
                Command::shared(
                    "SELECT fvoci.app_set_course_details($1,'edited','','','','',NULL,NULL,NULL,@ids)",
                    vec![w.item],
                ),
                Command::shared(
                    "SELECT fvoci.app_add_episodes($1,jsonb_build_array(jsonb_build_object('title','added','request_id',$2)),@ids)",
                    vec![w.item, Uuid::now_v7()],
                ),
                Command::shared(
                    "SELECT fvoci.app_update_episode($1,1,1,'edited','',60,'','',NULL)",
                    vec![episode],
                ),
                Command::shared("SELECT fvoci.app_delete_episode($1,@ids)", vec![episode]),
                Command::personal(
                    "SELECT fvoci.app_set_episode_progress($1,'in_progress',NULL,10,'private',current_date)",
                    vec![episode],
                ),
            ]
        }
        "study" => {
            rows(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_add_study_units($1,jsonb_build_array(jsonb_build_object('title','original','request_id',$2)),@ids)",
                &[w.item, Uuid::now_v7()],
            )
            .await;
            let unit = made(
                &w.state,
                &w.owner,
                "SELECT id FROM fvoci.study_units WHERE item_id=$1",
                &[w.item],
            )
            .await;
            vec![
                Command::shared(
                    "SELECT fvoci.app_add_study_units($1,jsonb_build_array(jsonb_build_object('title','added','request_id',$2)),@ids)",
                    vec![w.item, Uuid::now_v7()],
                ),
                Command::shared(
                    "SELECT fvoci.app_update_study_unit($1,1,1,'edited','','pages',10,NULL)",
                    vec![unit],
                ),
                Command::shared("SELECT fvoci.app_delete_study_unit($1,@ids)", vec![unit]),
                Command::personal(
                    "SELECT fvoci.app_set_unit_progress($1,'in_progress',3,NULL,'private',current_date)",
                    vec![unit],
                ),
                Command::personal(
                    "SELECT fvoci.app_plan_units($1,jsonb_build_array(jsonb_build_object('unit_id',$2,'planned_on','2026-11-02')))",
                    vec![w.item, unit],
                ),
            ]
        }
        _ => {
            let request = Uuid::now_v7();
            let create = "SELECT fvoci.app_create_record($1,$2,'retrospective','original',NULL,'','{}','[]',@ids) AS id";
            let record = made(&w.state, user, create, &[request, w.item]).await;
            vec![
                Command::shared(create, vec![Uuid::now_v7(), w.item]),
                // 재시도의 빠른 반환도 권한 회수 뒤에는 기록 id를 돌려주지 않는다.
                Command::shared(create, vec![request, w.item]),
                Command::shared(
                    "SELECT fvoci.app_update_record($1,1,'edited',NULL,'','{}','[]')",
                    vec![record],
                ),
                Command::shared("SELECT fvoci.app_delete_record($1,@ids)", vec![record]),
            ]
        }
    }
}

async fn shared_contents(w: &World) -> Result<Value, sqlx::Error> {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(
        "SELECT jsonb_build_array(
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.item_id) FROM fvoci.course_details r WHERE item_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.course_episodes r WHERE item_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.study_units r WHERE item_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.item_records r WHERE item_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.audit_log r WHERE target_id=$1))",
    )
    .bind(w.item)
    .fetch_one(&mut conn)
    .await
}

async fn wait_for_member(holder: i32, waiter: i32, call: &PendingCommand, sql: &str) -> TestResult {
    let url = std::env::var("FVOCI_TEST_ADMIN_URL")?;
    let mut observer = PgConnection::connect(&url).await?;
    let waiting = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(holder)
                .bind(waiter)
                .fetch_one(&mut observer)
                .await?;
            if blocked {
                return Ok::<_, sqlx::Error>(true);
            }
            if call.is_finished() {
                return Ok(false);
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if !matches!(waiting, Ok(Ok(true))) {
        call.abort();
        return Err(format!("command did not wait on the member row: {sql}: {waiting:?}").into());
    }
    Ok(())
}

async fn revocation_matrix(remove: bool) -> TestResult {
    for kind in ["course", "study", "project"] {
        let w = World::of_kind(kind).await;
        let member = w.member("member").await;
        for command in commands(&w, &member, kind).await {
            let before = shared_contents(&w).await?;
            let mut owner = common::fixture_conn().await;
            let mut held = owner.begin().await?;
            // 공간을 잠그지 않아 현재 멤버 행 자체에서의 대기를 증명한다.
            sqlx::query("SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR UPDATE")
                .bind(w.ws).bind(member.id.0).execute(&mut *held).await?;
            let holder = pid(&mut held).await;
            let mut tx = w.state.db.begin_as(member.id).await?;
            let waiter = pid(tx.conn()).await;
            let sql = command.sql;
            let call = tokio::spawn(async move {
                let result = on(tx.conn(), sql, &command.args).await;
                (tx, result)
            });
            wait_for_member(holder, waiter, &call, sql).await?;
            // 멤버 행을 기다릴 때 모듈·항목·기록을 먼저 쥐고 있으면 회수와 교착할 수 있다.
            let mut probe = common::fixture_conn().await;
            let mut probe_tx = probe.begin().await?;
            for table in [
                "modules",
                "items",
                "course_episodes",
                "study_units",
                "item_records",
            ] {
                let id = if table == "modules" { w.module } else { w.item };
                let column = if matches!(table, "modules" | "items") {
                    "id"
                } else {
                    "item_id"
                };
                sqlx::query(&format!(
                    "SELECT 1 FROM fvoci.{table} WHERE {column}=$1 FOR UPDATE NOWAIT"
                ))
                .bind(id)
                .execute(&mut *probe_tx)
                .await
                .map_err(|error| format!("{sql}: {error}"))?;
            }
            probe_tx.rollback().await?;
            let revoke = if remove {
                "DELETE FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2"
            } else {
                "UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1 AND user_id=$2"
            };
            sqlx::query(revoke)
                .bind(w.ws)
                .bind(member.id.0)
                .execute(&mut *held)
                .await?;
            held.commit().await?;
            let (tx, result) = tokio::time::timeout(Duration::from_secs(10), call).await??;
            if !remove && command.personal {
                assert!(result.is_ok(), "readonly personal write: {sql}: {result:?}");
                tx.commit().await?;
            } else {
                let Err(error) = result else {
                    return Err(format!("revoked command succeeded: {sql}").into());
                };
                code(&error, if remove { "P0002" } else { "42501" });
                drop(tx);
            }
            assert_eq!(
                shared_contents(&w).await?,
                before,
                "shared contents changed: {sql}"
            );
            // 다음 명령도 같은 회수 전 상태에서 시작한다.
            sqlx::query("INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member') ON CONFLICT(workspace_id,user_id) DO UPDATE SET role='member'")
                .bind(w.ws).bind(member.id.0).execute(&mut owner).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn records_commands_wait_for_member_demotion_and_recheck_access() -> TestResult {
    revocation_matrix(false).await
}

#[tokio::test]
async fn records_commands_wait_for_member_removal_and_recheck_access() -> TestResult {
    revocation_matrix(true).await
}

#[tokio::test]
async fn records_commands_hold_shared_workspace_and_member_locks_until_transaction_end() {
    for kind in ["course", "study", "project"] {
        let w = World::of_kind(kind).await;
        let member = w.member("member").await;
        for command in commands(&w, &member, kind).await {
            let mut tx = w.state.db.begin_as(member.id).await.unwrap();
            on(tx.conn(), command.sql, &command.args)
                .await
                .expect(command.sql);
            for lock in [
                "SELECT 1 FROM fvoci.workspaces WHERE id=$1 FOR NO KEY UPDATE NOWAIT",
                "SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR NO KEY UPDATE NOWAIT",
            ] {
                let mut probe = common::fixture_conn().await;
                let mut held = probe.begin().await.unwrap();
                let args = if lock.contains("user_id") {
                    vec![w.ws, member.id.0]
                } else {
                    vec![w.ws]
                };
                let error = on(&mut held, lock, &args).await.unwrap_err();
                code(&error, "55P03");
                held.rollback().await.unwrap();
            }
            // 성공한 함수가 배타 잠금을 잡아 모든 사용자를 줄 세우지는 않는다.
            let mut probe = common::fixture_conn().await;
            let mut held = probe.begin().await.unwrap();
            on(
                &mut held,
                "SELECT 1 FROM fvoci.workspaces WHERE id=$1 FOR SHARE NOWAIT",
                &[w.ws],
            )
            .await
            .unwrap();
            on(&mut held,"SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR SHARE NOWAIT",&[w.ws,member.id.0]).await.unwrap();
            held.rollback().await.unwrap();
            // 대상의 판·삭제 상태를 다음 명령에 남기지 않고 트랜잭션을 끝낸다.
            drop(tx);
        }
    }
}

const COPY_ITEM: &str = "SELECT * FROM fvoci.app_copy_item($1,$2,$3,'learning copy',false,@ids)";
const COPY_ITEM_RESET: &str =
    "SELECT * FROM fvoci.app_copy_item($1,$2,$3,'learning copy',true,@ids)";

async fn copy_fixture(kind: &str) -> (World, TestUser, Uuid) {
    let w = World::of_kind(kind).await;
    let member = w.member("member").await;
    if kind == "course" {
        rows(&w.state, &w.owner,
            "SELECT fvoci.app_set_course_details($1,'source subject','','','','',NULL,NULL,NULL,@ids)",
            &[w.item]).await;
        rows(&w.state, &w.owner,
            "SELECT fvoci.app_add_episodes($1,jsonb_build_array(jsonb_build_object('title','source episode','request_id',$2)),@ids)",
            &[w.item, Uuid::now_v7()]).await;
    } else {
        rows(&w.state, &w.owner,
            "SELECT fvoci.app_add_study_units($1,jsonb_build_array(jsonb_build_object('title','source unit','request_id',$2)),@ids)",
            &[w.item, Uuid::now_v7()]).await;
    }
    let target = made(&w.state, &w.owner,
        "SELECT fvoci.app_create_module($1,$2,(SELECT kind FROM fvoci.modules WHERE id=$3),'copy target','TARGET',@ids) AS id",
        &[w.ws, Uuid::now_v7(), w.module]).await;
    (w, member, target)
}

async fn copy_contents(w: &World) -> Result<Value, sqlx::Error> {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(
        "SELECT jsonb_build_array(
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.items r WHERE workspace_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.item_id) FROM fvoci.course_details r WHERE workspace_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.course_episodes r WHERE workspace_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.study_units r WHERE workspace_id=$1),
           (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.audit_log r WHERE workspace_id=$1))",
    )
    .bind(w.ws)
    .fetch_one(&mut conn)
    .await
}

async fn copy_during_revocation(remove: bool) -> TestResult {
    for kind in ["course", "study"] {
        for sql in [COPY_ITEM, COPY_ITEM_RESET] {
            let (w, member, target) = copy_fixture(kind).await;
            let before = copy_contents(&w).await?;
            let mut fixture = common::fixture_conn().await;
            let mut held = fixture.begin().await?;
            sqlx::query("SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR UPDATE")
                .bind(w.ws).bind(member.id.0).execute(&mut *held).await?;
            let holder = pid(&mut held).await;
            let mut tx = w.state.db.begin_as(member.id).await?;
            let waiter = pid(tx.conn()).await;
            let call = tokio::spawn(async move {
                let result = on(tx.conn(), sql, &[w.item, target, Uuid::now_v7()]).await;
                (tx, result)
            });
            wait_for_member(holder, waiter, &call, sql).await?;
            // 복제도 멤버 행을 기다리기 전에 원본·대상 모듈이나 항목을 잡지 않는다.
            let mut observer = common::fixture_conn().await;
            let mut probe = observer.begin().await?;
            sqlx::query(
                "SELECT 1 FROM fvoci.modules WHERE id=ANY($1) ORDER BY id FOR UPDATE NOWAIT",
            )
            .bind(vec![w.module, target])
            .execute(&mut *probe)
            .await?;
            sqlx::query("SELECT 1 FROM fvoci.items WHERE id=$1 FOR UPDATE NOWAIT")
                .bind(w.item)
                .execute(&mut *probe)
                .await?;
            probe.rollback().await?;
            let revoke = if remove {
                "DELETE FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2"
            } else {
                "UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1 AND user_id=$2"
            };
            sqlx::query(revoke)
                .bind(w.ws)
                .bind(member.id.0)
                .execute(&mut *held)
                .await?;
            held.commit().await?;
            let (tx, result) = tokio::time::timeout(Duration::from_secs(10), call).await??;
            let Err(error) = result else {
                return Err(format!("{kind}: copy succeeded after revocation: {sql}").into());
            };
            code(&error, if remove { "P0002" } else { "42501" });
            drop(tx);
            assert_eq!(copy_contents(&w).await?, before, "{kind}: {sql}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn learning_item_copies_wait_for_member_demotion_and_recheck_access() -> TestResult {
    copy_during_revocation(false).await
}

#[tokio::test]
async fn learning_item_copies_wait_for_member_removal_and_recheck_access() -> TestResult {
    copy_during_revocation(true).await
}

#[tokio::test]
async fn learning_item_copies_hold_shared_workspace_and_member_locks_until_commit() {
    for kind in ["course", "study"] {
        for sql in [COPY_ITEM, COPY_ITEM_RESET] {
            let (w, member, target) = copy_fixture(kind).await;
            let mut tx = w.state.db.begin_as(member.id).await.unwrap();
            let result = on(tx.conn(), sql, &[w.item, target, Uuid::now_v7()])
                .await
                .unwrap();
            let copied: Uuid = result[0]["copied_item"].as_str().unwrap().parse().unwrap();
            let table = if kind == "course" {
                "course_episodes"
            } else {
                "study_units"
            };
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE item_id=$1"))
                    .bind(copied)
                    .fetch_one(tx.conn())
                    .await
                    .unwrap();
            assert_eq!(count, 1, "{kind}: {sql}");
            if kind == "course" {
                let subject: String =
                    sqlx::query_scalar("SELECT subject FROM course_details WHERE item_id=$1")
                        .bind(copied)
                        .fetch_one(tx.conn())
                        .await
                        .unwrap();
                assert_eq!(subject, "source subject");
            }
            let mut observer = common::fixture_conn().await;
            let locks = [
                (
                    "SELECT 1 FROM fvoci.workspaces WHERE id=$1 FOR NO KEY UPDATE NOWAIT",
                    vec![w.ws],
                ),
                (
                    "SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR NO KEY UPDATE NOWAIT",
                    vec![w.ws, member.id.0],
                ),
            ];
            for (lock, args) in &locks {
                let mut probe = observer.begin().await.unwrap();
                code(&on(&mut probe, lock, args).await.unwrap_err(), "55P03");
                probe.rollback().await.unwrap();
            }
            let mut probe = observer.begin().await.unwrap();
            on(
                &mut probe,
                "SELECT 1 FROM fvoci.workspaces WHERE id=$1 FOR SHARE NOWAIT",
                &[w.ws],
            )
            .await
            .unwrap();
            on(&mut probe, "SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR SHARE NOWAIT", &[w.ws, member.id.0]).await.unwrap();
            probe.rollback().await.unwrap();
            tx.commit().await.unwrap();
            // 커밋 뒤에는 같은 두 행의 배타 잠금도 바로 잡을 수 있다.
            let mut probe = observer.begin().await.unwrap();
            for (lock, args) in &locks {
                on(&mut probe, lock, args).await.unwrap();
            }
            probe.rollback().await.unwrap();
        }
    }
}
