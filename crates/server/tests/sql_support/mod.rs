//! 새 기준 SQL의 앱 역할 계약 시험. 관리자 연결은 준비와 잠금 관찰에만 사용한다.
#![allow(
    dead_code,
    reason = "each integration binary uses a subset of SQL fixtures"
)]

use crate::common::{self, TestUser};
use fvoci_server::http::AppState;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

pub fn ids() -> Vec<Uuid> {
    (0..512).map(|_| Uuid::now_v7()).collect()
}

/// @ids는 새 UUID 배열 바인드다. 나머지 매개변수도 모두 바인드한다.
pub async fn on(
    conn: &mut PgConnection,
    sql: &str,
    args: &[Uuid],
) -> Result<Vec<Value>, sqlx::Error> {
    let sql = format!(
        "SELECT to_jsonb(result) FROM ({}) result",
        sql.replace("@ids", &format!("${}::uuid[]", args.len() + 1))
    );
    let mut query = sqlx::query_scalar::<_, Value>(&sql);
    for arg in args {
        query = query.bind(arg);
    }
    if sql.contains(&format!("${}::uuid[]", args.len() + 1)) {
        query = query.bind(ids());
    }
    query.fetch_all(conn).await
}

pub async fn attempt(
    state: &AppState,
    user: &TestUser,
    sql: &str,
    args: &[Uuid],
) -> Result<Vec<Value>, sqlx::Error> {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let rows = on(tx.conn(), sql, args).await?;
    tx.commit().await.unwrap();
    Ok(rows)
}

pub async fn rows(state: &AppState, user: &TestUser, sql: &str, args: &[Uuid]) -> Vec<Value> {
    attempt(state, user, sql, args).await.expect(sql)
}

pub async fn one(state: &AppState, user: &TestUser, sql: &str, args: &[Uuid]) -> Value {
    let mut result = rows(state, user, sql, args).await;
    assert_eq!(result.len(), 1, "{sql}: {result:?}");
    result.remove(0)
}

pub fn id(row: &Value) -> Uuid {
    Uuid::parse_str(row["id"].as_str().unwrap()).unwrap()
}

pub async fn made(state: &AppState, user: &TestUser, sql: &str, args: &[Uuid]) -> Uuid {
    id(&one(state, user, sql, args).await)
}

pub fn code(error: &sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some(expected),
        "{error}"
    );
}

pub async fn denied(state: &AppState, user: &TestUser, sql: &str, args: &[Uuid], expected: &str) {
    code(
        &attempt(state, user, sql, args).await.unwrap_err(),
        expected,
    );
}

pub async fn system(state: &AppState, sql: &str, args: &[Uuid]) -> Result<Vec<Value>, sqlx::Error> {
    let mut tx = state.db.begin_anonymous().await.unwrap();
    let result = on(tx.conn(), sql, args).await?;
    tx.commit().await.unwrap();
    Ok(result)
}

pub async fn fixture(sql: &str, args: &[Uuid]) {
    let mut conn = common::fixture_conn().await;
    let mut query = sqlx::query(sql);
    for arg in args {
        query = query.bind(arg);
    }
    query.execute(&mut conn).await.unwrap();
}

#[derive(Debug)]
pub struct World {
    pub state: AppState,
    pub owner: TestUser,
    pub org: Uuid,
    pub ws: Uuid,
    pub module: Uuid,
    pub item: Uuid,
}

impl World {
    pub async fn new() -> Self {
        Self::of_kind("project").await
    }

    pub async fn of_kind(kind: &str) -> Self {
        let state = common::state();
        let owner = common::login(&state, "SQL owner").await;
        let org = common::organization_for(&owner).await;
        let ws = workspace(&state, &owner, org).await;
        assert!(matches!(kind, "project" | "course" | "study"));
        let module = made(
            &state,
            &owner,
            &format!(
                "SELECT fvoci.app_create_module($1,$2,'{kind}','SQL module','SQL',@ids) AS id"
            ),
            &[ws, Uuid::now_v7()],
        )
        .await;
        let item = made(
            &state,
            &owner,
            "SELECT fvoci.app_create_item($1,$2,'SQL item','',@ids) AS id",
            &[module, Uuid::now_v7()],
        )
        .await;
        fixture(
            "UPDATE fvoci.items SET visibility='workspace' WHERE id=$1",
            &[item],
        )
        .await;
        Self {
            state,
            owner,
            org,
            ws,
            module,
            item,
        }
    }

    pub async fn member(&self, role: &str) -> TestUser {
        let email = common::unique_email(role);
        let user = common::login_with_email(&self.state, role, &email, true).await;
        common::join_organization(self.org, &user, "member").await;
        let mut conn = common::fixture_conn().await;
        sqlx::query(
            "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,$3)",
        )
        .bind(self.ws)
        .bind(user.id.0)
        .bind(role)
        .execute(&mut conn)
        .await
        .unwrap();
        user
    }

    pub async fn task(&self) -> Uuid {
        task(&self.state, &self.owner, self.item).await
    }
    pub async fn doc(&self) -> Uuid {
        made(&self.state, &self.owner, "SELECT fvoci.app_create_module_document($1,$2,'SQL document',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id", &[self.item, Uuid::now_v7()]).await
    }
    pub async fn note(&self) -> Uuid {
        made(&self.state, &self.owner, "SELECT fvoci.app_create_personal_note($1,'SQL note',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id", &[Uuid::now_v7()]).await
    }
    pub async fn share(&self, doc: Uuid, user: &TestUser, permission: &str) -> Uuid {
        assert!(matches!(permission, "read" | "write"));
        made(&self.state, &self.owner, &format!("SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'{permission}',$3,@ids) AS id"), &[doc,user.id.0,self.org]).await
    }
}

pub async fn workspace(state: &AppState, user: &TestUser, org: Uuid) -> Uuid {
    made(
        state,
        user,
        "SELECT fvoci.app_create_workspace($1,$2,'SQL workspace',@ids) AS id",
        &[Uuid::now_v7(), org],
    )
    .await
}

pub const CREATE_TASK: &str = "SELECT fvoci.app_create_task($1,$2,'SQL task',NULL,'','할 일','none',NULL,NULL,NULL,'{}','{}',false,@ids) AS id";
pub const UPDATE_TASK: &str = "SELECT fvoci.app_update_task($1,1,'edited','','할 일','none',NULL,NULL,NULL,'{}','{}',false,'[]',@ids) AS outcome";
pub const COMMENT: &str = "SELECT fvoci.app_add_comment($1,NULL,$2,NULL,'SQL comment',@ids) AS id";

pub async fn task(state: &AppState, user: &TestUser, item: Uuid) -> Uuid {
    made(state, user, CREATE_TASK, &[item, Uuid::now_v7()]).await
}

pub async fn attachment(w: &World, doc: Uuid) -> Uuid {
    let upload = made(&w.state,&w.owner,"SELECT fvoci.app_create_upload($1,NULL,'document',$2,'fixture.txt','','text/plain',10,10,3600,1000,100000,@ids) AS id", &[Uuid::now_v7(),doc]).await;
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[upload],
    )
    .await;
    made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_finish_upload($1,repeat('a',64),100000,@ids) AS id",
        &[upload],
    )
    .await;
    made(&w.state,&w.owner,"SELECT file_id AS id FROM fvoci.file_versions WHERE id=(SELECT version_id FROM fvoci.uploads WHERE id=$1)", &[upload]).await
}

/// 잠금 보유자의 PID와 실제 대기자의 PID를 모두 고정한다. 시간 경과 자체를 성공으로 보지 않는다.
pub async fn blocked(holder: i32, waiter: i32) {
    let url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via cargo xtask test");
    let mut observer = PgConnection::connect(&url).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$2 AND $1=ANY(pg_blocking_pids(pid)))")
                .bind(holder).bind(waiter).fetch_one(&mut observer).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("expected database lock wait was not observed");
}

pub async fn pid(conn: &mut PgConnection) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(conn)
        .await
        .unwrap()
}
