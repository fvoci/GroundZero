//! Event detail ownership and archive metadata keep the existing event read boundary (#180, D-96).
use crate::{common, sql_support};
use axum::http::{Method, StatusCode};
use serde_json::Value;
use sql_support::World;
use uuid::Uuid;

#[tokio::test]
async fn detail_metadata_matches_visible_roles_archive_and_deletion()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await;
    let app = common::app(&w.state);
    let admin = w.member("admin").await;
    let writer = w.member("member").await;
    let reader = w.member("readonly").await;
    let org_admin = common::login(&w.state, "event org admin").await;
    common::join_organization(w.org, &org_admin, "admin").await;
    let outsider = common::login(&w.state, "event outsider").await;
    let server_admin = common::login(&w.state, "event server admin").await;
    let mut ddl = common::fixture_conn().await;
    sqlx::query("INSERT INTO fvoci.server_admins(user_id) VALUES($1)")
        .bind(server_admin.id.0)
        .execute(&mut ddl)
        .await?;
    let event = sql_support::made(&w.state, &w.owner,
        "SELECT fvoci.app_create_event($1,$2,'meeting','EVENT_DETAIL_PRIVATE_SENTINEL','',true,'2026-11-02','2026-11-02',NULL,NULL,'UTC',@ids) AS id",
        &[w.item, Uuid::now_v7()]).await;
    let actors = [
        (&w.owner, true, true),
        (&admin, true, true),
        (&writer, true, false),
        (&reader, true, false),
        (&org_admin, false, false),
        (&outsider, false, false),
        (&server_admin, false, false),
    ];
    let path = format!("/api/events/{event}");
    let mut cases = 0;
    for private in [false, true] {
        sqlx::query("UPDATE fvoci.items SET visibility=$2 WHERE id=$1")
            .bind(w.item)
            .bind(if private { "private" } else { "workspace" })
            .execute(&mut ddl)
            .await?;
        for archive in ["none", "item", "module"] {
            sqlx::query(
                "UPDATE fvoci.items SET archived_at=CASE WHEN $2 THEN now() END WHERE id=$1",
            )
            .bind(w.item)
            .bind(archive == "item")
            .execute(&mut ddl)
            .await?;
            sqlx::query(
                "UPDATE fvoci.modules SET archived_at=CASE WHEN $2 THEN now() END WHERE id=$1",
            )
            .bind(w.module)
            .bind(archive == "module")
            .execute(&mut ddl)
            .await?;
            for deleted in [false, true] {
                sqlx::query(
                    "UPDATE fvoci.events SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1",
                )
                .bind(event)
                .bind(deleted)
                .execute(&mut ddl)
                .await?;
                for (actor, member, manager) in actors {
                    let (status, headers, bytes) =
                        common::send_raw(&app, Method::GET, &path, Some(actor), &[], Vec::new())
                            .await;
                    let body: Value = serde_json::from_slice(&bytes)?;
                    let visible = member && (!private || manager) && !deleted;
                    assert_eq!(
                        status,
                        if visible {
                            StatusCode::OK
                        } else {
                            StatusCode::NOT_FOUND
                        }
                    );
                    if visible {
                        assert_eq!(
                            headers
                                .get("x-fvoci-viewer-id")
                                .and_then(|v| v.to_str().ok()),
                            Some(actor.id.0.to_string().as_str())
                        );
                        assert_eq!(body["archived"], archive != "none");
                        assert_eq!(body["id"], event.to_string());
                    } else {
                        assert!(body.get("archived").is_none());
                        assert!(!body.to_string().contains("EVENT_DETAIL_PRIVATE_SENTINEL"));
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 84);
    let (status, _, bytes) =
        common::send_raw(&app, Method::GET, &path, None, &[], Vec::new()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!String::from_utf8(bytes)?.contains("EVENT_DETAIL_PRIVATE_SENTINEL"));
    Ok(())
}
