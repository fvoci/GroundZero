use super::Result;
use crate::{common, sql_support};
use fvoci_server::http::AppState;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

pub(super) struct Fixture {
    pub admin: PgConnection,
    pub ddl: PgConnection,
    pub fresh: common::FreshDb,
    pub state: AppState,
    pub owner: common::TestUser,
    pub actor: common::TestUser,
    pub org: Uuid,
    pub other_org: Uuid,
    pub ws: Uuid,
    pub module: Uuid,
    pub item: Uuid,
    pub other: Uuid,
    pub tasks: [Uuid; 2],
    pub roots: [Uuid; 2],
    pub group: Uuid,
}

impl Fixture {
    pub async fn new() -> Result<Self> {
        let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
        let fresh = common::fresh_database(&mut admin, "task_screen_reads").await;
        let state = fvoci_server::app::build_state(common::config_with(&[(
            "DATABASE_URL",
            &fresh.app_url,
        )]))?;
        let owner = common::login(&state, "screen-owner").await;
        let actor = common::login(&state, "screen-actor").await;
        let mut ddl = PgConnection::connect(
            fresh
                .db
                .migration_url
                .as_ref()
                .ok_or("migration URL")?
                .expose(),
        )
        .await?;
        let org = Uuid::now_v7();
        let other_org = Uuid::now_v7();
        sqlx::query("INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($1,'screen',$2,$3)")
            .bind(org).bind(owner.id.0).bind(Uuid::now_v7()).execute(&mut ddl).await?;
        sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,'admin')")
            .bind(org).bind(owner.id.0).execute(&mut ddl).await?;
        sqlx::query("INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($1,'outside organization',$2,$3)")
            .bind(other_org).bind(owner.id.0).bind(Uuid::now_v7()).execute(&mut ddl).await?;
        let ws = sql_support::workspace(&state, &owner, org).await;
        let module = sql_support::made(
            &state,
            &owner,
            "SELECT fvoci.app_create_module($1,$2,'project','screen','SCR',@ids) AS id",
            &[ws, Uuid::now_v7()],
        )
        .await;
        let mut items = Vec::new();
        let mut tasks = Vec::new();
        let mut roots = Vec::new();
        for n in 0..2 {
            let current_module = if n == 0 {
                module
            } else {
                sql_support::made(&state,&owner,"SELECT fvoci.app_create_module($1,$2,'project','other screen','OTH',@ids) AS id", &[ws,Uuid::now_v7()]).await
            };
            let item = sql_support::made(
                &state,
                &owner,
                "SELECT fvoci.app_create_item($1,$2,'screen','',@ids) AS id",
                &[current_module, Uuid::now_v7()],
            )
            .await;
            items.push(item);
            tasks.push(sql_support::task(&state, &owner, item).await);
            let root = sql_support::task(&state, &owner, item).await;
            roots.push(root);
            sqlx::query("UPDATE fvoci.tasks SET due_date='2026-11-01' WHERE id=$1")
                .bind(root)
                .execute(&mut ddl)
                .await?;
            let mut tx = state.db.begin_as(owner.id).await?;
            sqlx::query("SELECT fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[],\"until\":null,\"count\":3}', 'UTC',NULL,$2)")
                .bind(root).bind(sql_support::ids()).execute(tx.conn()).await?;
            tx.commit().await?;
            let selected = *tasks.last().ok_or("task fixture")?;
            sqlx::query(
                "INSERT INTO fvoci.task_assignees(task_id,user_id,assigned_by) VALUES($1,$2,$2)",
            )
            .bind(selected)
            .bind(owner.id.0)
            .execute(&mut ddl)
            .await?;
            sqlx::query("INSERT INTO fvoci.checklist_items(id,task_id,text,position,request_id,created_by) VALUES($1,$2,'screen check',0,$3,$4)")
                .bind(Uuid::now_v7()).bind(selected).bind(Uuid::now_v7()).bind(owner.id.0).execute(&mut ddl).await?;
            sqlx::query("INSERT INTO fvoci.task_dependencies(predecessor_id,successor_id,item_id,created_by) VALUES($1,$2,$3,$4)")
                .bind(root).bind(selected).bind(item).bind(owner.id.0).execute(&mut ddl).await?;
            let doc = sql_support::made(&state,&owner,"SELECT fvoci.app_create_module_document($1,$2,'screen document',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",&[item,Uuid::now_v7()]).await;
            sql_support::made(&state,&owner,"SELECT fvoci.app_create_module_document($1,$2,'screen child',1,'{\"type\":\"doc\",\"content\":[]}',$3,20,@ids) AS id",&[item,Uuid::now_v7(),doc]).await;
            sqlx::query("INSERT INTO fvoci.module_keys(workspace_id,key,module_id,retired_at) VALUES($1,$2,$3,now())")
                .bind(ws).bind(if n == 0 {"OLD"} else {"PAST"}).bind(current_module).execute(&mut ddl).await?;
            sqlx::query("UPDATE fvoci.items SET visibility='workspace' WHERE id=$1")
                .bind(item)
                .execute(&mut ddl)
                .await?;
        }
        let group = Uuid::now_v7();
        sqlx::query("INSERT INTO fvoci.groups(id,workspace_id,name) VALUES($1,$2,'screen')")
            .bind(group)
            .bind(ws)
            .execute(&mut ddl)
            .await?;
        Ok(Self {
            admin,
            ddl,
            fresh,
            state,
            owner,
            actor,
            org,
            other_org,
            ws,
            module,
            item: items[0],
            other: items[1],
            tasks: [tasks[0], tasks[1]],
            roots: [roots[0], roots[1]],
            group,
        })
    }

    pub async fn finish(mut self) -> Result<()> {
        self.state.db.close().await;
        self.ddl.close().await?;
        sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", self.fresh.name))
            .execute(&mut self.admin)
            .await?;
        Ok(())
    }
}
