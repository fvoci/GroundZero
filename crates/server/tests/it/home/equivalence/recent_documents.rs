//! #291 H2: the eight visible rows precede reference construction and retain NULLs.
use super::{Fixture, Result};
use crate::sql_support::made;
use serde_json::Value;
use uuid::Uuid;

async fn prepare() -> Result<(Fixture, Uuid, [Uuid; 3])> {
    let mut f = Fixture::new().await?;
    let hidden = made(&f.state, &f.owner,
        "SELECT fvoci.app_create_item((SELECT module_id FROM fvoci.items WHERE id=$1),$2,'hidden','',@ids) AS id",
        &[f.item, Uuid::now_v7()]).await;
    let shared = made(
        &f.state,
        &f.owner,
        "SELECT fvoci.app_create_module_document($1,$2,'shared child',1,'{}',NULL,20,@ids) AS id",
        &[hidden, Uuid::now_v7()],
    )
    .await;
    let module: Uuid = sqlx::query_scalar("SELECT module_id FROM fvoci.items WHERE id=$1")
        .bind(f.item)
        .fetch_one(&mut f.ddl)
        .await?;
    sqlx::query("INSERT INTO fvoci.document_shares(id,document_id,organization_id,grantee_user_id,permission,granted_by)
        VALUES($1,$2,$3,$4,'read',$5)")
        .bind(Uuid::now_v7()).bind(shared).bind(f.organizations[0]).bind(f.member.id.0)
        .bind(f.owner.id.0).execute(&mut f.ddl).await?;
    let mut visible = vec![shared];
    let normal = made(
        &f.state,
        &f.owner,
        "SELECT fvoci.app_create_module_document($1,$2,'visible child',1,'{}',NULL,20,@ids) AS id",
        &[f.item, Uuid::now_v7()],
    )
    .await;
    visible.push(normal);
    for _ in 0..8 {
        visible.push(
            made(
                &f.state,
                &f.owner,
                "SELECT fvoci.app_create_wiki_document($1,$2,'wiki',1,'{}',NULL,20,@ids) AS id",
                &[f.workspace, Uuid::now_v7()],
            )
            .await,
        );
    }
    let note = made(
        &f.state,
        &f.member,
        "SELECT fvoci.app_create_personal_note($1,'mine',1,'{}',NULL,20,@ids) AS id",
        &[Uuid::now_v7()],
    )
    .await;
    visible.push(note);
    for _ in 0..9 {
        let denied = made(&f.state, &f.owner,
            "SELECT fvoci.app_create_module_document($1,$2,'denied newest',1,'{}',NULL,20,@ids) AS id",
            &[hidden, Uuid::now_v7()]).await;
        sqlx::query("UPDATE fvoci.documents SET updated_by=$2,updated_at='2100-01-01' WHERE id=$1")
            .bind(denied)
            .bind(f.member.id.0)
            .execute(&mut f.ddl)
            .await?;
    }
    sqlx::query(
        "UPDATE fvoci.documents SET updated_by=$2,updated_at='2000-01-01' WHERE id=ANY($1)",
    )
    .bind(&visible)
    .bind(f.member.id.0)
    .execute(&mut f.ddl)
    .await?;
    sqlx::query("UPDATE fvoci.documents SET updated_at='2099-01-01' WHERE id=ANY($1)")
        .bind(vec![shared, normal, note])
        .execute(&mut f.ddl)
        .await?;
    Ok((f, module, [shared, normal, note]))
}

#[tokio::test]
async fn recent_documents_limit_after_rls_and_keep_hidden_missing_retired_keys_null() -> Result<()>
{
    let (mut f, module, [shared, normal, note]) = prepare().await?;
    for state in ["current", "retired", "missing"] {
        if state == "retired" {
            sqlx::query("UPDATE fvoci.module_keys SET retired_at=now() WHERE module_id=$1")
                .bind(module)
                .execute(&mut f.ddl)
                .await?;
        } else if state == "missing" {
            sqlx::query("DELETE FROM fvoci.module_keys WHERE module_id=$1")
                .bind(module)
                .execute(&mut f.ddl)
                .await?;
        }
        let actual = f
            .compare(Some(f.member.id.0), Some(f.organizations[0]), Some("UTC"))
            .await?;
        let rows = actual["value"]["recent_documents"]
            .as_array()
            .ok_or("recent documents")?;
        assert_eq!(rows.len(), 8, "RLS must precede LIMIT: {state}");
        for id in [shared, normal, note] {
            let row = rows
                .iter()
                .find(|r| r["id"] == id.to_string())
                .ok_or("visible row")?;
            if id == normal && state == "current" {
                assert!(
                    row["reference"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("PRO-"))
                );
            } else {
                assert_eq!(row["reference"], Value::Null, "{state}/{id}");
            }
        }
    }
    // A deleted document remains denied even when a stale explicit share exists.
    sqlx::query("UPDATE fvoci.documents SET deleted_at=now() WHERE id=$1")
        .bind(shared)
        .execute(&mut f.ddl)
        .await?;
    f.compare(Some(f.member.id.0), Some(f.organizations[0]), Some("UTC"))
        .await?;
    f.finish().await
}
