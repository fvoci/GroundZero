//! #180: a stale PATCH must not expose more than the caller's ordinary task read.
use super::{Result, fixture::Fixture, matrix};
use crate::common;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn conflict_assignees_cross_roles_visibility_archive_and_deletion() -> Result<()> {
    let mut f = Fixture::new().await?;
    let app = common::app(&f.state);
    // A different task has a second assignee: even visible relations must not enter this snapshot.
    sqlx::query("INSERT INTO fvoci.task_assignees(task_id,user_id,assigned_by) VALUES($1,$2,$3)")
        .bind(f.tasks[1])
        .bind(f.actor.id.0)
        .bind(f.owner.id.0)
        .execute(&mut f.ddl)
        .await?;
    let mut cases = 0;
    for org in ["creator", "admin", "member", "none", "outside"] {
        for ws in ["owner", "admin", "member", "readonly", "none"] {
            for server_admin in [false, true] {
                matrix::set_role(&mut f, org, ws, server_admin).await?;
                for private in [false, true] {
                    for status in ["live", "archived", "deleted"] {
                        check(
                            &mut f,
                            &app,
                            org,
                            ws,
                            private,
                            status,
                            &format!("{org}/{ws}/{server_admin}/{private}/{status}"),
                        )
                        .await?;
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        cases, 300,
        "every role, visibility, archive and deletion case"
    );
    let anonymous = common::send(
        &app,
        Method::PATCH,
        &format!("/api/tasks/{}?token=public-link", f.tasks[0]),
        None,
        Some(json!({"base_version": -1})),
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(anonymous.json.get("current").is_none());
    f.finish().await
}

async fn check(
    f: &mut Fixture,
    app: &axum::Router,
    org: &str,
    ws: &str,
    private: bool,
    status: &str,
    case: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE fvoci.items SET visibility=$2,archived_at=CASE WHEN $3 THEN now() END WHERE id=$1",
    )
    .bind(f.item)
    .bind(if private { "private" } else { "workspace" })
    .bind(status == "archived")
    .execute(&mut f.ddl)
    .await?;
    sqlx::query("UPDATE fvoci.tasks SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1")
        .bind(f.tasks[0])
        .bind(status == "deleted")
        .execute(&mut f.ddl)
        .await?;
    let path = format!("/api/tasks/{}", f.tasks[0]);
    let read = common::send(app, Method::GET, &path, Some(&f.actor), None).await;
    let visible = !matches!(org, "none" | "outside")
        && ws != "none"
        && (!private || matches!(ws, "owner" | "admin"))
        && status != "deleted";
    let expected = if !visible {
        StatusCode::NOT_FOUND
    } else if status == "archived" {
        StatusCode::CONFLICT
    } else if ws == "readonly" {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::CONFLICT
    };
    let reply = common::send(
        app,
        Method::PATCH,
        &path,
        Some(&f.actor),
        Some(json!({"base_version": -1})),
    )
    .await;

    assert_eq!(
        read.status,
        if visible {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        },
        "read: {case}"
    );
    assert_eq!(reply.status, expected, "{case}: {}", reply.json);
    if visible && ws != "readonly" && status == "live" {
        assert_eq!(
            reply.json["current"]["assignee_ids"],
            json!([f.owner.id.0]),
            "{case}"
        );
        assert_eq!(
            reply.json["current"]["version"], read.json["version"],
            "{case}"
        );
        assert_eq!(
            reply.json["changed"],
            json!([]),
            "only the version is stale: {case}"
        );
    } else {
        assert!(reply.json.get("current").is_none(), "no snapshot: {case}");
        assert!(
            !reply.json.to_string().contains(&f.owner.id.0.to_string()),
            "no assignee leak: {case}"
        );
    }
    Ok(())
}
