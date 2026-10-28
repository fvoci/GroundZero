use crate::common::{self, TestUser};
use crate::sql_support::{World, blocked, id, made, on, one, pid, rows};
use axum::http::{Method, StatusCode};
use fvoci_core::error::AppError;
use fvoci_db::{AnonTx, UserTx};
use sqlx::Connection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
enum Entry {
    Item,
    Series,
    System,
}

impl Entry {
    fn call(self) -> &'static str {
        match self {
            Self::Item => "SELECT fvoci.app_sync_item_series($1,@ids) AS made",
            Self::Series => "SELECT fvoci.app_sync_task_series($1,@ids) AS made",
            Self::System => "SELECT fvoci.app_system_sync_task_series($1,@ids) AS made",
        }
    }

    fn target(self, w: &World, series: Uuid) -> Uuid {
        match self {
            Self::Item => w.item,
            Self::Series | Self::System => series,
        }
    }

    async fn transaction(self, w: &World) -> SyncTx {
        match self {
            Self::System => SyncTx::System(w.state.db.begin_anonymous().await.unwrap()),
            Self::Item | Self::Series => {
                SyncTx::User(w.state.db.begin_as(w.owner.id).await.unwrap())
            }
        }
    }
}

enum SyncTx {
    User(UserTx),
    System(AnonTx),
}

impl SyncTx {
    fn conn(&mut self) -> &mut sqlx::PgConnection {
        match self {
            Self::User(tx) => tx.conn(),
            Self::System(tx) => tx.conn(),
        }
    }

    async fn commit(self) -> Result<(), AppError> {
        match self {
            Self::User(tx) => tx.commit().await,
            Self::System(tx) => tx.commit().await,
        }
    }
}

async fn assigned_series() -> (World, TestUser, Uuid) {
    let w = World::new().await;
    let user = common::login(&w.state, "recurrence organization admin").await;
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let root = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_task($1,$2,'recurrence recovery',NULL,'','할 일','none',NULL,current_date,NULL,'{}',ARRAY[$3],false,@ids) AS id",
        &[w.item, Uuid::now_v7(), user.id.0],
    )
    .await;
    // 리더 정리는 항목 행을 잠근다. 회차가 먼저 시작한 시험에서 대기를 관찰할 수 있다.
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_item_profile($1,1,$2,'','',NULL,NULL,'planned','{}',@ids)",
        &[w.item, user.id.0],
    )
    .await;
    let series = one(&w.state, &w.owner,
        "SELECT series_id AS id FROM fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}','UTC',NULL,@ids)", &[root]).await;
    (w, user, id(&series))
}

async fn demotion_bound(tx: &mut UserTx, w: &World, user: &TestUser) -> i32 {
    sqlx::query_scalar("SELECT fvoci.app_set_organization_member_ids($1,$2,'member')")
        .bind(w.org)
        .bind(user.id.0)
        .fetch_one(tx.conn())
        .await
        .unwrap()
}

async fn demotion_command(
    tx: &mut UserTx,
    organization: Uuid,
    member: Uuid,
    count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT fvoci.app_set_organization_member($1,$2,'member',$3)")
        .bind(organization)
        .bind(member)
        .bind(fvoci_db::new_ids(usize::try_from(count).unwrap()))
        .execute(tx.conn())
        .await?;
    Ok(())
}

async fn recovered(w: &World, user: &TestUser, series: Uuid) {
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT a.task_id FROM fvoci.task_assignees a JOIN fvoci.tasks t ON t.id=a.task_id WHERE t.series_id=$1 AND a.user_id=$2",
            &[series, user.id.0],
        )
        .await
        .is_empty(),
        "SR-201-5 an occurrence retained an assignee who lost workspace access"
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT workspace_id FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, user.id.0],
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.tasks WHERE series_id=$1",
            &[series]
        )
        .await
        .len()
            >= 2
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT leader_id FROM fvoci.items WHERE id=$1",
            &[w.item],
        )
        .await["leader_id"],
        serde_json::Value::Null
    );
}

async fn sync_after_bound(entry: Entry) {
    let (w, user, series) = assigned_series().await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let holder = pid(demotion.conn()).await;
    let count = demotion_bound(&mut demotion, &w, &user).await;
    let mut syncing = entry.transaction(&w).await;
    let waiter = pid(syncing.conn()).await;
    let target = entry.target(&w, series);
    let mut pending = tokio::spawn(async move {
        let result = on(syncing.conn(), entry.call(), &[target]).await?;
        syncing.commit().await?;
        Ok::<_, AppError>(result)
    });
    // 수정 전에는 회차가 커밋되고, 수정 뒤에는 멤버 잠금에서 기다린다. 고정 sleep 없이 둘을 관찰한다.
    let completed = tokio::select! {
        result = &mut pending => Some(result.unwrap()),
        () = blocked(holder, waiter) => None,
    };
    let result = demotion_command(&mut demotion, w.org, user.id.0, count).await;
    assert!(
        result.is_ok(),
        "SR-201-5 {entry:?} demotion failed: {result:?}"
    );
    demotion.commit().await.unwrap();
    let synced = match completed {
        Some(result) => result,
        None => tokio::time::timeout(std::time::Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap(),
    }
    .unwrap();
    assert!(synced[0]["made"].as_i64().unwrap() > 0);
    recovered(&w, &user, series).await;
}

async fn sync_before_bound(entry: Entry) {
    let (w, user, series) = assigned_series().await;
    let mut syncing = entry.transaction(&w).await;
    let target = entry.target(&w, series);
    let synced = on(syncing.conn(), entry.call(), &[target]).await.unwrap();
    assert!(synced[0]["made"].as_i64().unwrap() > 0);
    let holder = pid(syncing.conn()).await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(demotion.conn()).await;
    let (organization, member) = (w.org, user.id.0);
    let pending = tokio::spawn(async move {
        let count: i32 =
            sqlx::query_scalar("SELECT fvoci.app_set_organization_member_ids($1,$2,'member')")
                .bind(organization)
                .bind(member)
                .fetch_one(demotion.conn())
                .await?;
        demotion_command(&mut demotion, organization, member, count).await?;
        demotion.commit().await
    });
    blocked(holder, waiter).await;
    syncing.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.is_ok(),
        "SR-201-5 {entry:?} demotion failed: {result:?}"
    );
    recovered(&w, &user, series).await;
}

#[tokio::test]
async fn item_sync_after_demotion_bound_cannot_leave_assignees() {
    sync_after_bound(Entry::Item).await;
}

#[tokio::test]
async fn single_sync_after_demotion_bound_cannot_leave_assignees() {
    sync_after_bound(Entry::Series).await;
}

#[tokio::test]
async fn system_sync_after_demotion_bound_cannot_leave_assignees() {
    sync_after_bound(Entry::System).await;
}

#[tokio::test]
async fn demotion_counts_item_sync_committed_while_waiting() {
    sync_before_bound(Entry::Item).await;
}

#[tokio::test]
async fn demotion_counts_single_sync_committed_while_waiting() {
    sync_before_bound(Entry::Series).await;
}

#[tokio::test]
async fn demotion_counts_system_sync_committed_while_waiting() {
    sync_before_bound(Entry::System).await;
}

async fn blocked_waiter(holder: i32) -> i32 {
    let mut observer = common::fixture_conn().await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiter: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) ORDER BY pid LIMIT 1",
            )
            .bind(holder)
            .fetch_optional(&mut observer)
            .await
            .unwrap();
            if let Some(waiter) = waiter {
                return waiter;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("SR-201-5 sync did not take the assignee member lock before its lanes")
}

async fn assign_root(w: &World, root: Uuid, user: &TestUser) {
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_update_task($1,(SELECT version FROM fvoci.tasks WHERE id=$1),'recurrence recovery','','할 일','none',NULL,current_date,NULL,'{}',ARRAY[$2],false,'[]',@ids)",
        &[root, user.id.0],
    )
    .await;
}

/// 선행 잠금 뒤 바뀐 담당은 열을 쥔 채 기다리지 않고 전체 롤백·409 후 다시 만든다.
#[tokio::test]
async fn changed_root_assignee_returns_retryable_conflict_without_waiting_in_a_lane() {
    let (w, user, series) = assigned_series().await;
    let first = w.member("member").await;
    let root = id(&one(
        &w.state,
        &w.owner,
        "SELECT root_task_id AS id FROM fvoci.task_series WHERE id=$1",
        &[series],
    )
    .await);
    assign_root(&w, root, &first).await;
    // 관리자 연결은 선행 멤버 잠금의 대기를 고정하는 데만 쓴다. 담당 변경은 실제 명령이다.
    let mut fixture = common::fixture_conn().await;
    let mut holder = fixture.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(w.ws)
    .bind(first.id.0)
    .execute(&mut *holder)
    .await
    .unwrap();
    let holder_pid = pid(&mut holder).await;
    let app = common::app(&w.state);
    let actor = w.owner.clone();
    let uri = format!("/api/items/{}/recurrences/sync", w.item);
    let sync_uri = uri.clone();
    let pending = tokio::spawn(async move {
        common::send(&app, Method::POST, &sync_uri, Some(&actor), None).await
    });
    let waiter = blocked_waiter(holder_pid).await;
    blocked(holder_pid, waiter).await;
    assign_root(&w, root, &user).await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let count = demotion_bound(&mut demotion, &w, &user).await;
    holder.commit().await.unwrap();
    let reply = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reply.status, StatusCode::CONFLICT, "{:?}", reply.json);
    assert_eq!(reply.json["code"], "invalid_state");
    assert!(
        reply.json["message"]
            .as_str()
            .unwrap()
            .contains("try again")
    );
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.tasks WHERE series_id=$1",
            &[series]
        )
        .await
        .len(),
        1,
        "a failed NOWAIT sync must roll back its new occurrence"
    );
    demotion_command(&mut demotion, w.org, user.id.0, count)
        .await
        .unwrap();
    demotion.commit().await.unwrap();
    let retry = common::send(
        &common::app(&w.state),
        Method::POST,
        &uri,
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(retry.status, StatusCode::OK, "{:?}", retry.json);
    assert!(retry.json["created"].as_i64().unwrap() > 0);
    recovered(&w, &user, series).await;
}
