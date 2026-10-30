//! #240: the cursor must preserve the original ordered, permission-filtered result.
use crate::common;
use fvoci_core::ids::UserId;
use fvoci_server::http::AppState;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Frozen SQL body from 887f360a, not derived from the implementation under test.
const REFERENCE: &str = r"
CREATE FUNCTION fvoci.recent_activity_reference(p_organization uuid, p_limit integer)
RETURNS TABLE(id uuid, kind text, actor_id uuid, actor_name text, workspace_id uuid,
 target_kind text, target_id uuid, item_id uuid, detail jsonb, created_at timestamptz)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = fvoci, pg_temp AS $$
 SELECT a.id, a.kind, a.actor,
   CASE WHEN app_shares_organization(a.actor) THEN u.display_name END,
   a.workspace_id, a.target_kind, a.target_id, a.item_id, a.detail, a.created_at
 FROM activity a LEFT JOIN users u ON u.id = a.actor
 WHERE a.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
   AND app_access(a.target_kind, a.target_id) <> 'none'
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT greatest(1, least(coalesce(p_limit, 20), 100))
$$;
ALTER FUNCTION fvoci.recent_activity_reference(uuid,integer) OWNER TO fvoci_migrator;
REVOKE ALL ON FUNCTION fvoci.recent_activity_reference(uuid,integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.recent_activity_reference(uuid,integer) TO fvoci_app;
";

const COMPARE: &str = r"
SELECT
 (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY n), '[]')
  FROM fvoci.recent_activity_reference($1,$2) WITH ORDINALITY
  r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n)),
 (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY n), '[]')
  FROM fvoci.app_recent_activity($1,$2) WITH ORDINALITY
  r(id,kind,actor_id,actor_name,workspace_id,target_kind,target_id,item_id,detail,created_at,n))
";

struct Fixture {
    admin: PgConnection,
    ddl: PgConnection,
    fresh: common::FreshDb,
    state: AppState,
    owner: UserId,
    reader: UserId,
    organization_only: UserId,
    organization: Uuid,
    workspace: Uuid,
}

impl Fixture {
    async fn new() -> Result<Self> {
        let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
        let fresh = common::fresh_database(&mut admin, "recent_activity").await;
        let state = fvoci_server::app::build_state(common::config_with(&[(
            "DATABASE_URL",
            &fresh.app_url,
        )]))?;
        let owner = common::login(&state, "recent-owner").await.id;
        let reader = common::login(&state, "recent-readonly").await.id;
        let organization_only = common::login(&state, "recent-organization-only").await.id;
        let mut ddl = PgConnection::connect(
            fresh
                .db
                .migration_url
                .as_ref()
                .ok_or("missing migration URL")?
                .expose(),
        )
        .await?;
        // Organization membership is fixture setup, as in common::organization_for.
        let organization = Uuid::now_v7();
        sqlx::query("INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($1,'recent',$2,$3)")
            .bind(organization).bind(owner.0).bind(Uuid::now_v7()).execute(&mut ddl).await?;
        for (user, role) in [
            (owner, "admin"),
            (reader, "member"),
            (organization_only, "member"),
        ] {
            sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,$3)")
                .bind(organization).bind(user.0).bind(role).execute(&mut ddl).await?;
        }
        let mut workspaces = Vec::new();
        for n in 0..3 {
            let mut tx = state.db.begin_as(owner).await?;
            let workspace: Uuid =
                sqlx::query_scalar("SELECT fvoci.app_create_workspace($1,$2,$3,$4)")
                    .bind(Uuid::now_v7())
                    .bind(organization)
                    .bind(format!("recent {n}"))
                    .bind(vec![Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7()])
                    .fetch_one(tx.conn())
                    .await?;
            tx.commit().await?;
            workspaces.push(workspace);
        }
        let workspace = workspaces[0];
        sqlx::query("INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'readonly')")
            .bind(workspace).bind(reader.0).execute(&mut ddl).await?;
        let mut documents = Vec::new();
        for ws in workspaces {
            // One transaction produces tied timestamps; IDs establish the second key.
            let mut tx = state.db.begin_as(owner).await?;
            for n in 0..24 {
                let doc: Uuid = sqlx::query_scalar("SELECT fvoci.app_create_wiki_document($1,$2,$3,1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,$4)")
                    .bind(ws).bind(Uuid::now_v7()).bind(format!("recent {n}"))
                    .bind(vec![Uuid::now_v7(),Uuid::now_v7(),Uuid::now_v7()])
                    .fetch_one(tx.conn()).await?;
                if ws == workspace {
                    documents.push(doc);
                }
            }
            tx.commit().await?;
        }
        // Denied targets remain in activity. LIMIT must be filled past them.
        sqlx::query("UPDATE fvoci.documents SET deleted_at=now() WHERE id=ANY($1)")
            .bind(&documents[20..])
            .execute(&mut ddl)
            .await?;
        sqlx::raw_sql(REFERENCE).execute(&mut ddl).await?;
        Ok(Self {
            admin,
            ddl,
            fresh,
            state,
            owner,
            reader,
            organization_only,
            organization,
            workspace,
        })
    }

    async fn finish(mut self) -> Result<()> {
        sqlx::query("DROP FUNCTION fvoci.recent_activity_reference(uuid,integer)")
            .execute(&mut self.ddl)
            .await?;
        self.state.db.close().await;
        self.ddl.close().await?;
        sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", self.fresh.name))
            .execute(&mut self.admin)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn cursor_matches_reference_for_105_scopes_limits_and_ties() -> Result<()> {
    let fixture = Fixture::new().await?;
    let mut conn = PgConnection::connect(&fixture.fresh.app_url).await?;
    let mut cases = 0;
    for who in [
        Some(fixture.owner.0),
        Some(fixture.reader.0),
        Some(fixture.organization_only.0),
        Some(Uuid::now_v7()),
        None,
    ] {
        sqlx::query("SELECT set_config('app.user_id',$1,false)")
            .bind(who.map(|id| id.to_string()).unwrap_or_default())
            .execute(&mut conn)
            .await?;
        for org in [Some(fixture.organization), None, Some(Uuid::now_v7())] {
            for limit in [
                None,
                Some(-1),
                Some(0),
                Some(1),
                Some(20),
                Some(100),
                Some(101),
            ] {
                let (expected, actual): (Value, Value) = sqlx::query_as(COMPARE)
                    .bind(org)
                    .bind(limit)
                    .fetch_one(&mut conn)
                    .await?;
                assert_eq!(
                    actual, expected,
                    "actor={who:?}, organization={org:?}, limit={limit:?}"
                );
                if who == Some(fixture.reader.0)
                    && org == Some(fixture.organization)
                    && limit == Some(20)
                {
                    let rows = actual.as_array().ok_or("expected activity array")?;
                    assert_eq!(rows.len(), 20, "denied rows must not consume the limit");
                    assert!(
                        rows.iter()
                            .all(|r| r["workspace_id"] == fixture.workspace.to_string())
                    );
                    assert!(
                        rows.iter()
                            .all(|r| r["created_at"] == rows[0]["created_at"])
                    );
                    assert!(
                        rows.windows(2)
                            .all(|pair| pair[0]["id"].as_str() > pair[1]["id"].as_str())
                    );
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 105);
    for who in [fixture.owner, fixture.reader] {
        sqlx::query("SELECT set_config('app.user_id',$1,false)")
            .bind(who.0.to_string())
            .execute(&mut conn)
            .await?;
        for limit in [1i32, 2, 20] {
            let (expected, actual): (Value, Value) = sqlx::query_as(COMPARE)
                .bind(fixture.organization)
                .bind(limit)
                .fetch_one(&mut conn)
                .await?;
            assert_eq!(actual, expected, "tie actor={who:?}, limit={limit}");
            let rows = actual.as_array().ok_or("expected tied activity")?;
            assert_eq!(i32::try_from(rows.len())?, limit);
            assert!(
                rows.iter()
                    .all(|r| r["created_at"] == rows[0]["created_at"])
            );
            assert!(
                rows.windows(2)
                    .all(|pair| pair[0]["id"].as_str() > pair[1]["id"].as_str())
            );
        }
    }
    conn.close().await?;
    fixture.finish().await
}

#[tokio::test]
async fn cursor_keeps_the_reference_snapshot_during_permission_revocation() -> Result<()> {
    let mut fixture = Fixture::new().await?;
    let mut read = PgConnection::connect(&fixture.fresh.app_url).await?;
    sqlx::query("SELECT set_config('app.user_id',$1,false)")
        .bind(fixture.reader.0.to_string())
        .execute(&mut read)
        .await?;
    let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut read)
        .await?;
    let before: (Value, Value) = sqlx::query_as(COMPARE)
        .bind(fixture.organization)
        .bind(20i32)
        .fetch_one(&mut read)
        .await?;
    assert_eq!(before.0.as_array().ok_or("expected rows")?.len(), 20);
    assert_eq!(before.0, before.1);
    let barrier: i64 = sqlx::query_scalar("SELECT hashtextextended($1,0)")
        .bind(Uuid::now_v7().to_string())
        .fetch_one(&mut fixture.ddl)
        .await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(barrier)
        .execute(&mut fixture.ddl)
        .await?;
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut fixture.ddl)
        .await?;
    // Only this reader is gated. The original access implementation and its
    // permissions remain intact; the wrapper is confined to this disposable DB.
    sqlx::raw_sql(&format!(
        r"
      ALTER FUNCTION fvoci.app_access(text,uuid) RENAME TO recent_access_reference;
      CREATE FUNCTION fvoci.app_access(p_kind text,p_id uuid) RETURNS text
      LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=fvoci,pg_temp AS $$
      BEGIN
        IF pg_backend_pid()={reader_pid} AND p_kind='document' THEN
          PERFORM pg_advisory_xact_lock({barrier}::bigint);
        END IF;
        RETURN recent_access_reference(p_kind,p_id);
      END $$;
      ALTER FUNCTION fvoci.app_access(text,uuid) OWNER TO fvoci_migrator;
      REVOKE ALL ON FUNCTION fvoci.app_access(text,uuid) FROM PUBLIC;
      GRANT EXECUTE ON FUNCTION fvoci.app_access(text,uuid) TO fvoci_app;
    "
    ))
    .execute(&mut fixture.ddl)
    .await?;
    let org = fixture.organization;
    let pending = tokio::spawn(async move {
        let result: (Value, Value) = sqlx::query_as(COMPARE)
            .bind(org)
            .bind(20i32)
            .fetch_one(&mut read)
            .await?;
        read.close().await?;
        Result::Ok(result)
    });
    crate::sql_support::blocked(holder, reader_pid).await;
    let mut write = fixture.state.db.begin_as(fixture.owner).await?;
    let count: i32 = sqlx::query_scalar("SELECT fvoci.app_remove_member_ids($1,$2)")
        .bind(fixture.workspace)
        .bind(fixture.reader.0)
        .fetch_one(write.conn())
        .await?;
    let ids: Vec<_> = (0..count).map(|_| Uuid::now_v7()).collect();
    sqlx::query("SELECT fvoci.app_remove_member($1,$2,$3)")
        .bind(fixture.workspace)
        .bind(fixture.reader.0)
        .bind(ids)
        .execute(write.conn())
        .await?;
    write.commit().await?;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(barrier)
        .execute(&mut fixture.ddl)
        .await?;
    let during = tokio::time::timeout(std::time::Duration::from_secs(10), pending).await???;
    assert_eq!(
        during.0, before.0,
        "the original STABLE function keeps the statement snapshot"
    );
    assert_eq!(
        during.1, during.0,
        "the cursor must use the same pre-revocation snapshot"
    );
    let mut after = fixture.state.db.begin_as(fixture.reader).await?;
    let result: (Value, Value) = sqlx::query_as(COMPARE)
        .bind(org)
        .bind(20i32)
        .fetch_one(after.conn())
        .await?;
    assert_eq!(result.0, serde_json::json!([]));
    assert_eq!(
        result.1, result.0,
        "the next statement must see the committed revocation"
    );
    after.commit().await?;
    sqlx::raw_sql("DROP FUNCTION fvoci.app_access(text,uuid); ALTER FUNCTION fvoci.recent_access_reference(text,uuid) RENAME TO app_access;")
        .execute(&mut fixture.ddl).await?;
    fixture.finish().await
}
