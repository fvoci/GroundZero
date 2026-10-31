//! #210: item summaries must preserve a81cab4d's ordered, RLS-filtered result.

use crate::common::{self, TestUser};
use crate::sql_support;
use axum::http::{Method, StatusCode};
use fvoci_core::ids::DocumentId;
use fvoci_documents::documents::DocumentSummary;
use fvoci_documents::items::ITEM_DOCUMENTS_SQL;
use fvoci_server::http::AppState;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Frozen from a81cab4d; do not derive this from the product query/constants.
const REFERENCE: &str = "SELECT id, app_document_reference(id) AS reference, title, revision,
 updated_at, CASE WHEN app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id
 FROM documents WHERE item_id = $1 ORDER BY position, number NULLS LAST, created_at, id";

struct Fixture {
    admin: PgConnection,
    ddl: PgConnection,
    fresh: common::FreshDb,
    state: AppState,
    users: Vec<TestUser>,
    workspace: Uuid,
    organization: Uuid,
    modules: Vec<Uuid>,
    items: Vec<Uuid>,
    documents: Vec<Uuid>,
    share: Uuid,
}

impl Fixture {
    async fn new() -> Result<Self> {
        let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
        let fresh = common::fresh_database(&mut admin, "item_list").await;
        let state = fvoci_server::app::build_state(common::config_with(&[(
            "DATABASE_URL",
            &fresh.app_url,
        )]))?;
        let mut users = Vec::new();
        for label in ["owner", "readonly", "shared-member", "outside", "denied"] {
            users.push(
                common::login_with_email(&state, label, &common::unique_email(label), true).await,
            );
        }
        let mut ddl = PgConnection::connect(
            fresh
                .db
                .migration_url
                .as_ref()
                .ok_or("missing migration URL")?
                .expose(),
        )
        .await?;
        let organization = Uuid::now_v7();
        sqlx::query("INSERT INTO fvoci.organizations(id,name,created_by,request_id) VALUES($1,'item list',$2,$3)")
            .bind(organization).bind(users[0].id.0).bind(Uuid::now_v7()).execute(&mut ddl).await?;
        for (n, user) in users.iter().take(4).enumerate() {
            sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,$3)")
                .bind(organization).bind(user.id.0).bind(if n == 0 { "admin" } else { "member" })
                .execute(&mut ddl).await?;
        }
        let workspace = sql_support::made(
            &state,
            &users[0],
            "SELECT app_create_workspace($1,$2,'item list',@ids) AS id",
            &[Uuid::now_v7(), organization],
        )
        .await;
        for (user, role) in [(&users[1], "readonly"), (&users[2], "member")] {
            sqlx::query(
                "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,$3)",
            )
            .bind(workspace)
            .bind(user.id.0)
            .bind(role)
            .execute(&mut ddl)
            .await?;
        }
        let mut modules = Vec::new();
        let mut items = Vec::new();
        let mut documents = Vec::new();
        for key in ["LIST", "NEXT"] {
            let module = sql_support::made(
                &state,
                &users[0],
                &format!("SELECT app_create_module($1,$2,'project','list','{key}',@ids) AS id"),
                &[workspace, Uuid::now_v7()],
            )
            .await;
            modules.push(module);
            let item = sql_support::made(
                &state,
                &users[0],
                "SELECT app_create_item($1,$2,'list','',@ids) AS id",
                &[module, Uuid::now_v7()],
            )
            .await;
            items.push(item);
            sqlx::query("UPDATE fvoci.items SET visibility='workspace' WHERE id=$1")
                .bind(item)
                .execute(&mut ddl)
                .await?;
            let mut parent = None;
            for n in 0..8 {
                let mut tx = state.db.begin_as(users[0].id).await?;
                let doc: Uuid = sqlx::query_scalar("SELECT app_create_module_document($1,$2,$3,1,'{\"type\":\"doc\",\"content\":[]}',$4,20,$5)")
                    .bind(item).bind(Uuid::now_v7()).bind(format!("{key} {n}"))
                    .bind(if (1..4).contains(&n) { parent } else { None })
                    .bind(sql_support::ids()).fetch_one(tx.conn()).await?;
                tx.commit().await?;
                if n == 0 {
                    parent = Some(doc);
                }
                documents.push(doc);
            }
        }
        items.push(
            sql_support::made(
                &state,
                &users[0],
                "SELECT app_create_item($1,$2,'empty list','',@ids) AS id",
                &[modules[0], Uuid::now_v7()],
            )
            .await,
        );
        sqlx::query("UPDATE fvoci.items SET visibility='workspace' WHERE id=$1")
            .bind(items[2])
            .execute(&mut ddl)
            .await?;
        // Same positions and timestamps exercise number ordering. NULL numbers
        // belong to personal notes; module numbers cannot be NULL (table CHECK).
        sqlx::query("UPDATE fvoci.documents SET position=0, created_at='2026-10-01T00:00:00Z', updated_at='2026-10-02T00:00:00.123456Z' WHERE item_id=ANY($1)")
            .bind(&items).execute(&mut ddl).await?;
        sqlx::query("UPDATE fvoci.documents SET title='수정한 문서', revision=revision+1, updated_at='2026-10-03T00:00:00.654321Z' WHERE id=$1")
            .bind(documents[5]).execute(&mut ddl).await?;
        sqlx::query("UPDATE fvoci.documents SET visibility='private' WHERE id=ANY($1)")
            .bind(vec![
                documents[0],
                documents[2],
                documents[8],
                documents[10],
            ])
            .execute(&mut ddl)
            .await?;
        // Duplicate numbers in different scopes are valid and must keep their key.
        // A shared child never grants access to its private parent or private item.
        let mut share = Uuid::nil();
        for user in [&users[2], &users[3]] {
            let email: String = sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
                .bind(user.id.0)
                .fetch_one(&mut ddl)
                .await?;
            let mut tx = state.db.begin_as(users[0].id).await?;
            let id: Uuid = sqlx::query_scalar("SELECT app_share_document($1,$2,'read',$3,$4)")
                .bind(documents[2])
                .bind(email)
                .bind(organization)
                .bind(sql_support::ids())
                .fetch_one(tx.conn())
                .await?;
            tx.commit().await?;
            if user.id == users[2].id {
                share = id;
            }
        }
        Ok(Self {
            admin,
            ddl,
            fresh,
            state,
            users,
            workspace,
            organization,
            modules,
            items,
            documents,
            share,
        })
    }

    async fn reference(&self, user: &TestUser, item: Uuid) -> Result<Value> {
        let mut tx = self.state.db.begin_as(user.id).await?;
        type Row = (
            DocumentId,
            Option<String>,
            String,
            i32,
            OffsetDateTime,
            Option<DocumentId>,
        );
        let rows: Vec<Row> = sqlx::query_as(REFERENCE)
            .bind(item)
            .fetch_all(tx.conn())
            .await?;
        let actual: Vec<Row> = sqlx::query_as(ITEM_DOCUMENTS_SQL)
            .bind(item)
            .fetch_all(tx.conn())
            .await?;
        assert_eq!(
            actual, rows,
            "raw app-role SQL, actor={:?}, item={item}",
            user.id
        );
        tx.commit().await?;
        let summaries: Vec<_> = rows
            .into_iter()
            .map(
                |(id, reference, title, revision, updated_at, parent_id)| DocumentSummary {
                    id,
                    reference,
                    title,
                    revision,
                    updated_at,
                    parent_id,
                },
            )
            .collect();
        Ok(serde_json::to_value(summaries)?)
    }

    async fn compare(
        &self,
        label: &str,
        unavailable: &[Uuid],
        private: Option<Uuid>,
    ) -> Result<()> {
        for (n, user) in self.users.iter().enumerate() {
            for item in self.items.iter().copied().chain([Uuid::now_v7()]) {
                let expected = self.reference(user, item).await?;
                // Expected HTTP visibility comes from the fixture, not app_access.
                let visible = n < 3
                    && self.items.contains(&item)
                    && !unavailable.contains(&item)
                    && (private != Some(item) || n == 0);
                let reply = common::send(
                    &common::app(&self.state),
                    Method::GET,
                    &format!("/api/items/{item}/documents"),
                    Some(user),
                    None,
                )
                .await;
                if visible {
                    assert_eq!(
                        reply.status,
                        StatusCode::OK,
                        "{label}, actor={n}: {}",
                        reply.json
                    );
                    assert_eq!(reply.json, expected, "{label}, actor={n}, item={item}");
                } else {
                    assert_eq!(
                        reply.status,
                        StatusCode::NOT_FOUND,
                        "{label}, actor={n}, item={item}"
                    );
                }
            }
        }
        Ok(())
    }

    async fn finish(mut self) -> Result<()> {
        self.state.db.close().await;
        self.ddl.close().await?;
        sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", self.fresh.name))
            .execute(&mut self.admin)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn item_summaries_match_original_for_visibility_trash_moves_and_keys() -> Result<()> {
    let mut f = Fixture::new().await?;
    f.compare("active", &[], None).await?;
    let rows = f.reference(&f.users[1], f.items[0]).await?;
    assert_eq!(rows.as_array().ok_or("expected summaries")?.len(), 6);
    let child = rows
        .as_array()
        .ok_or("expected summaries")?
        .iter()
        .find(|r| r["id"] == f.documents[1].to_string())
        .ok_or("visible child")?;
    assert_eq!(
        child["parent_id"],
        Value::Null,
        "private parents stay hidden"
    );
    assert!(
        rows.as_array()
            .ok_or("expected summaries")?
            .iter()
            .all(|r| r["reference"]
                .as_str()
                .is_some_and(|s| s.starts_with("LIST-")))
    );
    let shared = f.reference(&f.users[2], f.items[0]).await?;
    assert_eq!(shared.as_array().ok_or("expected summaries")?.len(), 7);
    let outside = f.reference(&f.users[3], f.items[0]).await?;
    assert_eq!(outside.as_array().ok_or("expected summaries")?.len(), 1);
    assert_eq!(
        outside[0]["reference"],
        Value::Null,
        "an unreadable item hides its module key"
    );
    assert_eq!(outside[0]["parent_id"], Value::Null);
    let email: String = sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
        .bind(f.users[2].id.0)
        .fetch_one(&mut f.ddl)
        .await?;
    let mut tx = f.state.db.begin_as(f.users[0].id).await?;
    let parent_share: Uuid = sqlx::query_scalar("SELECT app_share_document($1,$2,'read',$3,$4)")
        .bind(f.documents[0])
        .bind(email)
        .bind(f.organization)
        .bind(sql_support::ids())
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    f.compare("parent visible through an explicit share", &[], None)
        .await?;
    let shared = f.reference(&f.users[2], f.items[0]).await?;
    let child = shared
        .as_array()
        .ok_or("expected summaries")?
        .iter()
        .find(|r| r["id"] == f.documents[2].to_string())
        .ok_or("shared child")?;
    assert_eq!(child["parent_id"], f.documents[0].to_string());
    sql_support::rows(
        &f.state,
        &f.users[0],
        "SELECT app_unshare_document($1,@ids)",
        &[parent_share],
    )
    .await;
    f.compare("parent share revoked; child still shared", &[], None)
        .await?;
    for (table, id) in [
        ("documents", f.documents[0]),
        ("documents", f.documents[1]),
        ("items", f.items[0]),
        ("modules", f.modules[0]),
        ("workspaces", f.workspace),
    ] {
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET deleted_at=now() WHERE id=$1"
        ))
        .bind(id)
        .execute(&mut f.ddl)
        .await?;
        let unavailable = match table {
            "workspaces" => f.items.clone(),
            "items" => vec![f.items[0]],
            "modules" => vec![f.items[0], f.items[2]],
            _ => Vec::new(),
        };
        f.compare(table, &unavailable, None).await?;
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET deleted_at=NULL WHERE id=$1"
        ))
        .bind(id)
        .execute(&mut f.ddl)
        .await?;
        f.compare("restored", &[], None).await?;
    }
    for (table, id) in [("items", f.items[0]), ("modules", f.modules[0])] {
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET archived_at=now() WHERE id=$1"
        ))
        .bind(id)
        .execute(&mut f.ddl)
        .await?;
        f.compare("archived", &[], None).await?;
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET archived_at=NULL WHERE id=$1"
        ))
        .bind(id)
        .execute(&mut f.ddl)
        .await?;
    }
    sqlx::query("UPDATE fvoci.items SET visibility='private' WHERE id=$1")
        .bind(f.items[0])
        .execute(&mut f.ddl)
        .await?;
    f.compare("private item with shared child", &[], Some(f.items[0]))
        .await?;
    sqlx::query("UPDATE fvoci.items SET visibility='workspace' WHERE id=$1")
        .bind(f.items[0])
        .execute(&mut f.ddl)
        .await?;
    sql_support::rows(
        &f.state,
        &f.users[0],
        "SELECT * FROM app_move_document($1,$2,$3,@ids)",
        &[f.documents[0], f.workspace, f.items[1]],
    )
    .await;
    f.compare("parent moved across modules; children detached", &[], None)
        .await?;
    sqlx::query("UPDATE fvoci.module_keys SET retired_at=now() WHERE module_id=$1")
        .bind(f.modules[1])
        .execute(&mut f.ddl)
        .await?;
    f.compare("no current key", &[], None).await?;
    sqlx::query(
        "INSERT INTO fvoci.module_keys(workspace_id,module_id,key) VALUES($1,$2,'RENAMED')",
    )
    .bind(f.workspace)
    .bind(f.modules[1])
    .execute(&mut f.ddl)
    .await?;
    f.compare("renamed key", &[], None).await?;
    sql_support::rows(
        &f.state,
        &f.users[0],
        "SELECT app_unshare_document($1,@ids)",
        &[f.share],
    )
    .await;
    f.compare("share revoked", &[], None).await?;
    // Other SUMMARY_COLUMNS callers keep personal NULL references and wiki references.
    for (uri, expected_prefix) in [
        ("/api/me/notes".to_owned(), None),
        (
            format!("/api/workspaces/{}/wiki/documents", f.workspace),
            Some("WIKI-"),
        ),
    ] {
        let created = common::send(
            &common::app(&f.state),
            Method::POST,
            &uri,
            Some(&f.users[0]),
            Some(json!({"request_id":Uuid::now_v7(),"title":"other list"})),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.json);
        let list = common::send(
            &common::app(&f.state),
            Method::GET,
            &uri,
            Some(&f.users[0]),
            None,
        )
        .await;
        assert_eq!(list.status, StatusCode::OK);
        assert_eq!(list.json[0]["id"], created.json["id"]);
        match expected_prefix {
            Some(prefix) => assert!(
                list.json[0]["reference"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(prefix))
            ),
            None => assert_eq!(list.json[0]["reference"], Value::Null),
        }
        if expected_prefix.is_none() {
            let other = common::send(
                &common::app(&f.state),
                Method::POST,
                &uri,
                Some(&f.users[2]),
                Some(json!({"request_id":Uuid::now_v7(),"title":"다른 사람의 개인 노트"})),
            )
            .await;
            assert_eq!(other.status, StatusCode::CREATED, "{}", other.json);
            let hidden = common::send(
                &common::app(&f.state),
                Method::GET,
                &format!(
                    "/api/documents/{}",
                    other.json["id"].as_str().ok_or("private note id")?
                ),
                Some(&f.users[0]),
                None,
            )
            .await;
            assert_eq!(
                hidden.status,
                StatusCode::NOT_FOUND,
                "workspace ownership never exposes another user's private note"
            );
            f.compare("private notes stay outside item summaries", &[], None)
                .await?;
            let note = Uuid::parse_str(created.json["id"].as_str().ok_or("note id")?)?;
            sql_support::rows(
                &f.state,
                &f.users[0],
                "SELECT * FROM app_move_document($1,$2,$3,@ids)",
                &[note, f.workspace, f.items[0]],
            )
            .await;
            f.compare("personal note moved into item", &[], None)
                .await?;
            let before = f.reference(&f.users[0], f.items[0]).await?;
            let denied = sql_support::attempt(
                &f.state,
                &f.users[0],
                "SELECT * FROM app_move_document($1,NULL,NULL,@ids)",
                &[note],
            )
            .await;
            let error = denied
                .err()
                .ok_or("team-to-personal moves must stay rejected")?;
            sql_support::code(&error, "55000");
            assert_eq!(
                f.reference(&f.users[0], f.items[0]).await?,
                before,
                "a rejected reverse move must preserve every summary"
            );
            f.compare("rejected team-to-personal move", &[], None)
                .await?;
        }
    }
    f.finish().await
}

#[tokio::test]
async fn item_summaries_keep_statement_snapshot_during_share_revocation() -> Result<()> {
    let mut f = Fixture::new().await?;
    let expected = f.reference(&f.users[2], f.items[0]).await?;
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
    // Confined to this fresh database. Gate the real HTTP reader inside its first
    // document RLS check, retaining the original STABLE access implementation.
    sqlx::raw_sql(&format!(
        r"
        CREATE FUNCTION fvoci.item_list_access_reference(p_kind text,p_id uuid) RETURNS text
        LANGUAGE sql STABLE SECURITY DEFINER SET search_path=fvoci,pg_temp
        AS $$ SELECT app_access_as(p_kind,p_id,app_user_id()) $$;
        ALTER FUNCTION fvoci.item_list_access_reference(text,uuid) OWNER TO fvoci_migrator;
        REVOKE ALL ON FUNCTION fvoci.item_list_access_reference(text,uuid) FROM PUBLIC;
        CREATE OR REPLACE FUNCTION fvoci.app_access(p_kind text,p_id uuid) RETURNS text
        LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=fvoci,pg_temp AS $$
        BEGIN
          IF app_user_id()='{}'::uuid AND p_kind='document' THEN
            PERFORM pg_advisory_xact_lock({barrier}::bigint);
          END IF;
          RETURN item_list_access_reference(p_kind,p_id);
        END $$;
        ALTER FUNCTION fvoci.app_access(text,uuid) OWNER TO fvoci_migrator;
        REVOKE ALL ON FUNCTION fvoci.app_access(text,uuid) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION fvoci.app_access(text,uuid) TO fvoci_app;
    ",
        f.users[2].id.0
    ))
    .execute(&mut f.ddl)
    .await?;
    let app = common::app(&f.state);
    let reader = f.users[2].clone();
    let uri = format!("/api/items/{}/documents", f.items[0]);
    let pending =
        tokio::spawn(
            async move { common::send(&app, Method::GET, &uri, Some(&reader), None).await },
        );
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder)
            .fetch_one(&mut f.ddl)
            .await?;
            if waiting {
                return Result::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    sql_support::rows(
        &f.state,
        &f.users[0],
        "SELECT app_unshare_document($1,@ids)",
        &[f.share],
    )
    .await;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(barrier)
        .execute(&mut f.ddl)
        .await?;
    let during = tokio::time::timeout(std::time::Duration::from_secs(10), pending).await??;
    assert_eq!(during.status, StatusCode::OK, "{}", during.json);
    assert_eq!(
        during.json, expected,
        "in-flight list keeps the original statement snapshot"
    );
    sqlx::raw_sql("CREATE OR REPLACE FUNCTION fvoci.app_access(p_kind text,p_id uuid) RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path=fvoci,pg_temp AS $$ SELECT app_access_as(p_kind,p_id,app_user_id()) $$; DROP FUNCTION fvoci.item_list_access_reference(text,uuid);")
        .execute(&mut f.ddl).await?;
    let after = f.reference(&f.users[2], f.items[0]).await?;
    assert_eq!(after.as_array().ok_or("expected summaries")?.len(), 6);
    assert_ne!(
        after, expected,
        "the next statement must observe revocation"
    );
    f.compare("after concurrent revocation", &[], None).await?;
    f.finish().await
}
