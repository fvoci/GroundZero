//! #245 / #210: freeze the complete home JSON, including errors and statement snapshots.
use crate::common::{self, TestUser};
use crate::sql_support::{ids, made};
use fvoci_server::http::AppState;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

mod aggregate_roles;
mod recent_documents;

const COMPARE: &str = "SELECT fvoci.home_outcome($1,$2,true), fvoci.home_outcome($1,$2,false)";
const OUTCOME: &str = r"
CREATE FUNCTION fvoci.home_outcome(org uuid,tz text,reference boolean) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY INVOKER SET search_path=fvoci,pg_temp AS $$
BEGIN
  IF reference THEN RETURN jsonb_build_object('value',home_reference(org,tz)); END IF;
  RETURN jsonb_build_object('value',app_home(org,tz));
EXCEPTION WHEN OTHERS THEN RETURN jsonb_build_object('code',SQLSTATE,'message',SQLERRM);
END $$;
REVOKE ALL ON FUNCTION fvoci.home_outcome(uuid,text,boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.home_outcome(uuid,text,boolean) TO fvoci_app;
";

struct Fixture {
    admin: PgConnection,
    ddl: PgConnection,
    read: PgConnection,
    today: time::Date,
    fresh: common::FreshDb,
    state: AppState,
    owner: TestUser,
    reader: TestUser,
    member: TestUser,
    organization_only: TestUser,
    organizations: [Uuid; 2],
    workspace: Uuid,
    item: Uuid,
    unshared: Uuid,
}

impl Fixture {
    async fn new() -> Result<Self> {
        let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
        let fresh = common::fresh_database(&mut admin, "home_equivalence").await;
        let state = fvoci_server::app::build_state(common::config_with(&[(
            "DATABASE_URL",
            &fresh.app_url,
        )]))?;
        let owner = common::login(&state, "home-owner").await;
        let reader = common::login(&state, "home-reader").await;
        let member = common::login(&state, "home-member").await;
        let organization_only = common::login(&state, "home-organization-only").await;
        let mut ddl = PgConnection::connect(
            fresh
                .db
                .migration_url
                .as_ref()
                .ok_or("migration URL")?
                .expose(),
        )
        .await?;
        let organizations = [Uuid::now_v7(), Uuid::now_v7()];
        let mut workspaces = Vec::new();
        let mut items = Vec::new();
        for org in organizations {
            sqlx::query("INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($1,'home',$2,$3)")
                .bind(org).bind(owner.id.0).bind(Uuid::now_v7()).execute(&mut ddl).await?;
            for (user, role) in [
                (&owner, "admin"),
                (&reader, "member"),
                (&member, "member"),
                (&organization_only, "member"),
            ] {
                sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,$3)")
                    .bind(org).bind(user.id.0).bind(role).execute(&mut ddl).await?;
            }
            for _ in 0..2 {
                let ws = made(
                    &state,
                    &owner,
                    "SELECT fvoci.app_create_workspace($1,$2,'home',@ids) AS id",
                    &[Uuid::now_v7(), org],
                )
                .await;
                for (user, role) in [(&reader, "readonly"), (&member, "member")] {
                    sqlx::query("INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,$3)")
                        .bind(ws).bind(user.id.0).bind(role).execute(&mut ddl).await?;
                }
                for (kind, key) in [
                    ("project", "PRO"),
                    ("research", "RES"),
                    ("course", "COU"),
                    ("study", "STU"),
                    ("hobby", "HOB"),
                ] {
                    let module = made(&state,&owner,&format!("SELECT fvoci.app_create_module($1,$2,'{kind}','home','{key}',@ids) AS id"),&[ws,Uuid::now_v7()]).await;
                    let item = made(
                        &state,
                        &owner,
                        "SELECT fvoci.app_create_item($1,$2,'home','',@ids) AS id",
                        &[module, Uuid::now_v7()],
                    )
                    .await;
                    sqlx::query("UPDATE fvoci.items SET visibility='workspace' WHERE id=$1")
                        .bind(item)
                        .execute(&mut ddl)
                        .await?;
                    items.push(item);
                }
                workspaces.push(ws);
            }
        }
        let workspace = workspaces[0];
        let item = items[0];
        // Personal references are NULL. A stale mark must remain invisible until shared.
        let unshared = made(&state,&owner,"SELECT fvoci.app_create_personal_note($1,'private note',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",&[Uuid::now_v7()]).await;
        let mut marks = state.db.begin_as(owner.id).await?;
        for mark in ["pin", "favorite"] {
            sqlx::query("SELECT fvoci.app_set_mark($1,'document',$2,true)")
                .bind(mark)
                .bind(unshared)
                .execute(marks.conn())
                .await?;
        }
        marks.commit().await?;
        sqlx::query("UPDATE fvoci.users SET email='home-reader@example.test',email_verified=true WHERE id=$1")
            .bind(reader.id.0)
            .execute(&mut ddl)
            .await?;
        for mark in ["pin", "favorite"] {
            sqlx::query("INSERT INTO fvoci.user_marks(user_id,mark,target_kind,target_id) VALUES($1,$2,'document',$3)")
                .bind(reader.id.0).bind(mark).bind(unshared).execute(&mut ddl).await?;
        }
        // Private/archived/deleted parents and another organization exercise RLS.
        for (id, column, value) in [
            (items[1], "visibility", "'private'"),
            (items[2], "archived_at", "now()"),
            (items[3], "deleted_at", "now()"),
        ] {
            sqlx::query(&format!(
                "UPDATE fvoci.items SET {column}={value} WHERE id=$1"
            ))
            .bind(id)
            .execute(&mut ddl)
            .await?;
        }
        sqlx::query("UPDATE fvoci.items SET updated_at='2026-01-01T00:00:00Z' WHERE id=ANY($1)")
            .bind(&items)
            .execute(&mut ddl)
            .await?;
        sqlx::raw_sql(include_str!("../../fixtures/home/app_home_reference.sql"))
            .execute(&mut ddl)
            .await?;
        sqlx::raw_sql(OUTCOME).execute(&mut ddl).await?;
        // Keep now() fixed while READ COMMITTED still sees each fixture update.
        let mut read = PgConnection::connect(&fresh.app_url).await?;
        sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&mut read)
            .await?;
        let today = sqlx::query_scalar("SELECT (now() AT TIME ZONE 'UTC')::date")
            .fetch_one(&mut read)
            .await?;
        Ok(Self {
            admin,
            ddl,
            read,
            today,
            fresh,
            state,
            owner,
            reader,
            member,
            organization_only,
            organizations,
            workspace,
            item,
            unshared,
        })
    }

    async fn populate(&self, from: usize, to: usize) -> Result<()> {
        // One transaction makes updated_at ties; the original UUID/number keys decide order.
        let mut tx = self.state.db.begin_as(self.owner.id).await?;
        for n in from..to {
            for offset in [-1, 0, 1] {
                sqlx::query("SELECT fvoci.app_create_task($1,$2,$3,NULL,'',NULL,'none',NULL,$4::date+$5,NULL,'{}',ARRAY[$6,$7],false,$8)")
                    .bind(self.item).bind(Uuid::now_v7()).bind(format!("home {n} {offset}"))
                    .bind(self.today).bind(offset).bind(self.owner.id.0).bind(self.reader.id.0).bind(ids()).execute(tx.conn()).await?;
            }
            sqlx::query("SELECT fvoci.app_create_wiki_document($1,$2,$3,1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,$4)")
                .bind(self.workspace).bind(Uuid::now_v7()).bind(format!("home {n}")).bind(ids()).execute(tx.conn()).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn compare(
        &mut self,
        actor: Option<Uuid>,
        org: Option<Uuid>,
        tz: Option<&str>,
    ) -> Result<Value> {
        sqlx::query("SELECT set_config('app.user_id',$1,false)")
            .bind(actor.map(|v| v.to_string()).unwrap_or_default())
            .execute(&mut self.read)
            .await?;
        let (expected, actual): (Value, Value) = sqlx::query_as(COMPARE)
            .bind(org)
            .bind(tz)
            .fetch_one(&mut self.read)
            .await?;
        assert_eq!(actual, expected, "actor={actor:?},org={org:?},tz={tz:?}");
        Ok(actual)
    }

    async fn add_edges(&mut self) -> Result<()> {
        let mut tx = self.state.db.begin_as(self.member.id).await?;
        for status in ["할 일", "완료"] {
            let task: Uuid = sqlx::query_scalar("SELECT fvoci.app_create_task($1,$2,'undated',NULL,'',$3,'none',NULL,NULL,NULL,'{}',ARRAY[$4,$5],false,$6)")
                .bind(self.item).bind(Uuid::now_v7()).bind(status).bind(self.owner.id.0).bind(self.reader.id.0).bind(ids()).fetch_one(tx.conn()).await?;
            sqlx::query("SELECT fvoci.app_set_mark('pin','task',$1,true)")
                .bind(task)
                .execute(tx.conn())
                .await?;
        }
        sqlx::query("SELECT fvoci.app_create_wiki_document($1,$2,'member document',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,$3)")
            .bind(self.workspace).bind(Uuid::now_v7()).bind(ids()).execute(tx.conn()).await?;
        tx.commit().await?;
        for user in [&self.owner, &self.reader] {
            let mut tx = self.state.db.begin_as(user.id).await?;
            for mark in ["pin", "favorite"] {
                sqlx::query("SELECT fvoci.app_set_mark($1,'item',$2,true)")
                    .bind(mark)
                    .bind(self.item)
                    .execute(tx.conn())
                    .await?;
            }
            tx.commit().await?;
        }
        sqlx::query("UPDATE fvoci.user_marks SET created_at='2026-01-01T00:00:00Z'")
            .execute(&mut self.ddl)
            .await?;
        Ok(())
    }

    async fn finish(mut self) -> Result<()> {
        sqlx::query("ROLLBACK").execute(&mut self.read).await?;
        self.read.close().await?;
        self.state.db.close().await;
        self.ddl.close().await?;
        sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", self.fresh.name))
            .execute(&mut self.admin)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn home_matches_frozen_json_for_scopes_timezones_and_fixed_limit_boundaries() -> Result<()> {
    let mut f = Fixture::new().await?;
    let mut from = 0;
    for to in [0usize, 1, 7, 8, 9, 49, 50, 51] {
        f.populate(from, to).await?;
        let value = f
            .compare(Some(f.owner.id.0), Some(f.organizations[0]), Some("UTC"))
            .await?;
        assert_eq!(
            value["value"]["recent_tasks"]
                .as_array()
                .ok_or("tasks")?
                .len(),
            (to * 3).min(8)
        );
        assert_eq!(
            value["value"]["recent_documents"]
                .as_array()
                .ok_or("documents")?
                .len(),
            (to + 1).min(8)
        );
        assert_eq!(
            value["value"]["up_next"].as_array().ok_or("up next")?.len(),
            to.min(50) * 3
        );
        assert_eq!(
            value["value"]["up_next_counts"],
            json!({"overdue":to,"today":to,"upcoming":to})
        );
        from = to;
    }
    f.add_edges().await?;
    for setting in [None, Some("America/New_York")] {
        sqlx::query("DELETE FROM fvoci.user_settings WHERE user_id=$1")
            .bind(f.owner.id.0)
            .execute(&mut f.ddl)
            .await?;
        if let Some(setting) = setting {
            sqlx::query("INSERT INTO fvoci.user_settings(user_id,time_zone) VALUES($1,$2)")
                .bind(f.owner.id.0)
                .bind(setting)
                .execute(&mut f.ddl)
                .await?;
        }
        for actor in [
            Some(f.owner.id.0),
            Some(f.reader.id.0),
            Some(f.member.id.0),
            Some(f.organization_only.id.0),
            Some(Uuid::now_v7()),
            None,
        ] {
            for org in [
                Some(f.organizations[0]),
                Some(f.organizations[1]),
                Some(Uuid::now_v7()),
                None,
            ] {
                for tz in [
                    None,
                    Some(""),
                    Some("  "),
                    Some(" UTC "),
                    Some("Asia/Seoul"),
                    Some("America/New_York"),
                    Some("Pacific/Kiritimati"),
                    Some("Nowhere/Land"),
                ] {
                    let value = f.compare(actor, org, tz).await?;
                    if actor == Some(f.organization_only.id.0)
                        && org == Some(f.organizations[0])
                        && tz == Some("Asia/Seoul")
                    {
                        assert_eq!(value["value"]["pins"], json!([]));
                        assert_eq!(value["value"]["up_next"], json!([]));
                    }
                    if actor.is_none() || org.is_none() {
                        assert_eq!(
                            value["code"], "P0002",
                            "organization rejection precedes timezone validation"
                        );
                    }
                }
            }
        }
    }
    f.finish().await
}

#[tokio::test]
async fn home_keeps_reference_snapshot_during_revocation_and_share_addition() -> Result<()> {
    let mut f = Fixture::new().await?;
    f.populate(0, 1).await?;
    let mut read = PgConnection::connect(&f.fresh.app_url).await?;
    sqlx::query("SELECT set_config('app.user_id',$1,false)")
        .bind(f.reader.id.0.to_string())
        .execute(&mut read)
        .await?;
    let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut read)
        .await?;
    // Keep now()/today constant across the three statements, including a midnight run.
    sqlx::query("BEGIN").execute(&mut read).await?;
    let org = f.organizations[0];
    let before: (Value, Value) = sqlx::query_as(COMPARE)
        .bind(org)
        .bind("UTC")
        .fetch_one(&mut read)
        .await?;
    assert_eq!(before.0, before.1);
    assert_eq!(
        before.0["value"]["up_next"]
            .as_array()
            .ok_or("up next")?
            .len(),
        3
    );
    let barrier: i64 = sqlx::query_scalar("SELECT hashtextextended($1,0)")
        .bind(Uuid::now_v7().to_string())
        .fetch_one(&mut f.ddl)
        .await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(barrier)
        .execute(&mut f.ddl)
        .await?;
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut f.ddl)
        .await?;
    // Gate the first workspace enumeration in this disposable DB. Preserve the
    // original implementation, ownership and effective role in the wrapper.
    sqlx::raw_sql(&format!(r"
      ALTER FUNCTION fvoci.app_my_workspace_ids(uuid) RENAME TO home_workspace_reference;
      CREATE FUNCTION fvoci.app_my_workspace_ids(org uuid) RETURNS SETOF uuid
      LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=fvoci,pg_temp AS $$ BEGIN
        IF pg_backend_pid()={reader_pid} THEN PERFORM pg_advisory_xact_lock({barrier}::bigint); END IF;
        RETURN QUERY SELECT * FROM home_workspace_reference(org);
      END $$;
      ALTER FUNCTION fvoci.app_my_workspace_ids(uuid) OWNER TO fvoci_migrator;
      REVOKE ALL ON FUNCTION fvoci.app_my_workspace_ids(uuid) FROM PUBLIC;
      GRANT EXECUTE ON FUNCTION fvoci.app_my_workspace_ids(uuid) TO fvoci_app;
    ")).execute(&mut f.ddl).await?;
    let pending = tokio::spawn(async move {
        let during: (Value, Value) = sqlx::query_as(COMPARE)
            .bind(org)
            .bind("UTC")
            .fetch_one(&mut read)
            .await?;
        Result::Ok((read, during))
    });
    crate::sql_support::blocked(holder, reader_pid).await;
    let mut write = f.state.db.begin_as(f.owner.id).await?;
    sqlx::query("SELECT fvoci.app_remove_member($1,$2,$3)")
        .bind(f.workspace)
        .bind(f.reader.id.0)
        .bind(ids())
        .execute(write.conn())
        .await?;
    sqlx::query("SELECT fvoci.app_share_document($1,'home-reader@example.test','read',$2,$3)")
        .bind(f.unshared)
        .bind(org)
        .bind(ids())
        .execute(write.conn())
        .await?;
    write.commit().await?;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(barrier)
        .execute(&mut f.ddl)
        .await?;
    let (mut read, during) =
        tokio::time::timeout(std::time::Duration::from_secs(10), pending).await???;
    assert_eq!(
        during.0, before.0,
        "reference retains the statement snapshot"
    );
    assert_eq!(during.1, during.0, "all lists retain the same snapshot");
    let after: (Value, Value) = sqlx::query_as(COMPARE)
        .bind(org)
        .bind("UTC")
        .fetch_one(&mut read)
        .await?;
    assert_eq!(after.0, after.1);
    assert_eq!(after.1["value"]["up_next"], json!([]));
    assert_eq!(after.1["value"]["pins"][0]["id"], f.unshared.to_string());
    assert_eq!(after.1["value"]["pins"][0]["item_id"], Value::Null);
    sqlx::query("ROLLBACK").execute(&mut read).await?;
    read.close().await?;
    f.finish().await
}
