#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

// nextval은 실패한 하위 트랜잭션에도 되돌아가지 않아 실제 실패 주입의 증거가 된다.
async fn install_failure(table: &str, operation: &str, predicate: &str) -> String {
    let name = format!("sql_failure_{}", Uuid::now_v7().simple());
    let sql = format!(
        "CREATE SEQUENCE fvoci.{name};
         GRANT SELECT ON SEQUENCE fvoci.{name} TO fvoci_app;
         CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER
         SET search_path TO fvoci,pg_temp AS $body$
         BEGIN
           IF {predicate} THEN
             PERFORM nextval('fvoci.{name}');
             RAISE EXCEPTION 'injected failure' USING ERRCODE='ZY085';
           END IF;
           RETURN {row};
         END $body$;
         CREATE TRIGGER {name} BEFORE {operation} ON fvoci.{table}
           FOR EACH ROW EXECUTE FUNCTION fvoci.{name}();",
        row = if operation == "DELETE" { "OLD" } else { "NEW" }
    );
    let mut conn = common::fixture_conn().await;
    sqlx::raw_sql(&sql).execute(&mut conn).await.unwrap();
    name
}

async fn remove_failure(table: &str, name: &str) {
    let mut conn = common::fixture_conn().await;
    sqlx::raw_sql(&format!("DROP TRIGGER {name} ON fvoci.{table}; DROP FUNCTION fvoci.{name}(); DROP SEQUENCE fvoci.{name};"))
        .execute(&mut conn).await.unwrap();
}

#[tokio::test]
async fn expired_batch_failure_rolls_back_its_files_but_other_batch_commits() {
    let w = World::new().await;
    let bad = w.doc().await;
    let good = w.doc().await;
    let attached = attachment(&w, bad).await;
    let failed_batch = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_trash_document($1,@ids) AS id",
        &[bad],
    )
    .await;
    let good_batch = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_trash_document($1,@ids) AS id",
        &[good],
    )
    .await;
    let injection = install_failure("documents", "DELETE", &format!("OLD.id='{bad}'::uuid")).await;
    fixture(
        "UPDATE fvoci.trash_batches SET deleted_at=now()-interval '40 days' WHERE id IN ($1,$2)",
        &[failed_batch, good_batch],
    )
    .await;
    let mut worker = w.state.db.begin_anonymous().await.unwrap();
    let needed: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_expired_ids()")
        .fetch_one(worker.conn())
        .await
        .unwrap();
    let purge = sqlx::query("SELECT fvoci.app_system_purge_expired($1)")
        .bind(fvoci_server::db::new_ids(usize::try_from(needed).unwrap()))
        .execute(worker.conn())
        .await;
    if purge.is_ok() {
        worker.commit().await.unwrap();
    } else {
        // 실패한 트랜잭션은 바로 닫는다. 열어 두면 문서 행 잠금이 남아 아래 트리거 제거(ACCESS EXCLUSIVE)가 멈춘다.
        drop(worker);
    }
    let fired = one(
        &w.state,
        &w.owner,
        &format!("SELECT is_called FROM fvoci.{injection}"),
        &[],
    )
    .await;
    remove_failure("documents", &injection).await;
    purge.unwrap();
    assert_eq!(
        fired["is_called"], true,
        "the target trigger must actually raise"
    );
    let trash = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.app_trash_list($1)",
        &[w.ws],
    )
    .await;
    assert!(trash.iter().any(|r| r["id"] == failed_batch.to_string()));
    assert!(!trash.iter().any(|r| r["id"] == good_batch.to_string()));
    // 실패 묶음의 시각만 되돌려 정상 복구: 문서 앞에서 지웠을 수 있는 첨부도 전부 살아 있어야 한다.
    fixture(
        "UPDATE fvoci.trash_batches SET deleted_at=now() WHERE id=$1",
        &[failed_batch],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_restore_batch($1,@ids)",
        &[failed_batch],
    )
    .await;
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.documents WHERE id=$1",
            &[bad]
        )
        .await
        .len(),
        1
    );
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.files WHERE id=$1",
            &[attached]
        )
        .await
        .len(),
        1
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.file_versions WHERE file_id=$1",
            &[attached]
        )
        .await["n"],
        1
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.documents WHERE id=$1",
            &[good]
        )
        .await
        .is_empty()
    );
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_restore_batch($1,@ids)",
        &[good_batch],
        "P0002",
    )
    .await;
}

#[tokio::test]
async fn a_failed_series_does_not_rollback_another_series_transaction() {
    let w = World::new().await;
    let mut series = Vec::new();
    let mut roots = Vec::new();
    for _ in 0..2 {
        let task = w.task().await;
        fixture(
            "UPDATE fvoci.tasks SET due_date=current_date-2 WHERE id=$1",
            &[task],
        )
        .await;
        let row = one(&w.state,&w.owner,"SELECT * FROM fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}','UTC',NULL,@ids)",&[task]).await;
        series.push(Uuid::parse_str(row["series_id"].as_str().unwrap()).unwrap());
        roots.push(task);
    }
    // 첫 생성은 진행시키고 둘째 생성에서 실패하여 부분 생성까지 롤백되는지 본다.
    let injection = install_failure(
        "tasks",
        "INSERT",
        &format!(
            "NEW.series_id='{}'::uuid AND NEW.occurrence_date=current_date",
            series[0]
        ),
    )
    .await;
    let bad = system(
        &w.state,
        "SELECT fvoci.app_system_sync_task_series($1,@ids)",
        &[series[0]],
    )
    .await;
    let good = system(
        &w.state,
        "SELECT fvoci.app_system_sync_task_series($1,@ids)",
        &[series[1]],
    )
    .await;
    let fired = one(
        &w.state,
        &w.owner,
        &format!("SELECT is_called FROM fvoci.{injection}"),
        &[],
    )
    .await;
    remove_failure("tasks", &injection).await;
    code(&bad.unwrap_err(), "ZY085");
    good.unwrap();
    assert_eq!(fired["is_called"], true);
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.tasks WHERE series_id=$1",
            &[series[0]]
        )
        .await,
        vec![serde_json::json!({"id":roots[0]})]
    );
    let generated = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.tasks WHERE series_id=$1 AND id<>$2",
        &[series[1], roots[1]],
    )
    .await;
    assert_eq!(generated.len(), 3);
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.audit_log WHERE workspace_id=$1 AND detail->>'series'=$2::text",
            &[w.ws, series[0]]
        )
        .await
        .is_empty()
    );
    let creates = rows(&w.state,&w.owner,
        "SELECT target_id FROM fvoci.activity WHERE item_id=$1 AND kind='create' AND detail->>'reason'='recurrence'",
        &[w.item]).await;
    assert_eq!(creates.len(), generated.len());
    assert!(
        creates
            .iter()
            .all(|r| generated.iter().any(|task| r["target_id"] == task["id"]))
    );
    // 트리거를 치운 뒤 같은 계열을 재시도하면 누락 없이 세 회차를 커밋한다.
    system(
        &w.state,
        "SELECT fvoci.app_system_sync_task_series($1,@ids)",
        &[series[0]],
    )
    .await
    .unwrap();
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.tasks WHERE series_id=$1 AND id<>$2",
            &[series[0], roots[0]]
        )
        .await
        .len(),
        3
    );
}

async fn run_purge(state: &fvoci_server::http::AppState) -> Result<(), sqlx::Error> {
    let mut worker = state.db.begin_anonymous().await.unwrap();
    let needed: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_expired_ids()")
        .fetch_one(worker.conn())
        .await
        .unwrap();
    let purge = sqlx::query("SELECT fvoci.app_system_purge_expired($1)")
        .bind(fvoci_server::db::new_ids(usize::try_from(needed).unwrap()))
        .execute(worker.conn())
        .await;
    if let Err(error) = purge {
        // 실패한 트랜잭션은 바로 닫는다. 열어 두면 행 잠금이 남아 트리거 제거(ACCESS EXCLUSIVE)가 멈춘다.
        drop(worker);
        return Err(error);
    }
    worker.commit().await.unwrap();
    Ok(())
}

async fn count(sql: &str, arg: Uuid) -> i64 {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(sql)
        .bind(arg)
        .fetch_one(&mut conn)
        .await
        .unwrap()
}

// #85: app_system_purge_expired의 삭제된 워크스페이스 분기. 실패한 워크스페이스는 하위 트랜잭션째 되돌아가 부분 삭제가
// 남지 않고, 같은 실행에서 뒤에 처리되는 정상 워크스페이스는 커밋된다.
#[tokio::test]
async fn expired_workspace_failure_rolls_back_its_purge_but_other_workspace_commits() {
    let bad = World::new().await;
    let good = World::new().await;
    // 워크스페이스 행을 지우기 전에 지워지는 자식(업무·항목)이 있어야 되돌림이 보인다.
    bad.task().await;
    good.task().await;
    for w in [&bad, &good] {
        rows(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_delete_workspace($1,'SQL workspace',@ids)",
            &[w.ws],
        )
        .await;
    }
    // 실패할 쪽을 먼저 처리하게 더 오래전에 지운 것으로 한다(실패 뒤에도 다음 대상으로 넘어가는지 보인다).
    fixture(
        "UPDATE fvoci.workspaces SET deleted_at=now()-interval '41 days' WHERE id=$1",
        &[bad.ws],
    )
    .await;
    fixture(
        "UPDATE fvoci.workspaces SET deleted_at=now()-interval '40 days' WHERE id=$1",
        &[good.ws],
    )
    .await;
    let injection = install_failure(
        "workspaces",
        "DELETE",
        &format!("OLD.id='{}'::uuid", bad.ws),
    )
    .await;
    let purge = run_purge(&bad.state).await;
    let fired = one(
        &bad.state,
        &bad.owner,
        &format!("SELECT is_called FROM fvoci.{injection}"),
        &[],
    )
    .await;
    remove_failure("workspaces", &injection).await;
    purge.unwrap();
    assert_eq!(
        fired["is_called"], true,
        "the target trigger must actually raise"
    );

    // 실패한 워크스페이스: 아직 삭제 표시된 채 그대로이고, 먼저 지워졌을 자식과 비우기 기록도 남아 있다.
    for (table, column) in [
        ("workspaces", "id"),
        ("tasks", "workspace_id"),
        ("items", "workspace_id"),
    ] {
        assert_eq!(
            count(
                &format!("SELECT count(*) FROM fvoci.{table} WHERE {column}=$1"),
                bad.ws
            )
            .await,
            1,
            "{table} of the failed workspace must survive the rollback"
        );
    }
    assert_eq!(
        count(
            "SELECT count(*) FROM fvoci.audit_log WHERE target_id=$1 AND action='purge'",
            bad.ws
        )
        .await,
        0
    );
    // 정상 워크스페이스: 같은 실행에서 커밋되어 사라지고 비우기 기록이 남는다.
    for (table, column) in [
        ("workspaces", "id"),
        ("tasks", "workspace_id"),
        ("items", "workspace_id"),
    ] {
        assert_eq!(
            count(
                &format!("SELECT count(*) FROM fvoci.{table} WHERE {column}=$1"),
                good.ws
            )
            .await,
            0,
            "{table} of the other workspace must be purged and committed"
        );
    }
    assert_eq!(
        count(
            "SELECT count(*) FROM fvoci.audit_log WHERE target_id=$1 AND action='purge'",
            good.ws
        )
        .await,
        2
    );

    // 주입을 걷은 뒤 다음 실행은 실패했던 워크스페이스도 비운다(일시적 실패였고 시험이 남긴 만료 대상도 치운다).
    run_purge(&bad.state).await.unwrap();
    assert_eq!(
        count("SELECT count(*) FROM fvoci.workspaces WHERE id=$1", bad.ws).await,
        0
    );
}
