//! #303: omitted assignees survive permission cleanup; explicit lists keep replacement semantics.
use super::{Result, fixture::Fixture, matrix};
use crate::{common, sql_support};
use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use std::time::Duration;
use uuid::Uuid;

async fn assignees(f: &mut Fixture) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM fvoci.task_assignees WHERE task_id=$1 ORDER BY user_id",
    )
    .bind(f.tasks[0])
    .fetch_all(&mut f.ddl)
    .await?)
}

async fn patch(f: &mut Fixture, ids: Option<Vec<Uuid>>) -> Result<common::Reply> {
    let version: i32 = sqlx::query_scalar("SELECT version FROM fvoci.tasks WHERE id=$1")
        .bind(f.tasks[0])
        .fetch_one(&mut f.ddl)
        .await?;
    let mut body = json!({"base_version": version, "description": "saved by patch"});
    if let Some(ids) = ids {
        body["assignee_ids"] = json!(ids);
    }
    Ok(common::send(
        &common::app(&f.state),
        Method::PATCH,
        &format!("/api/tasks/{}", f.tasks[0]),
        Some(&f.owner),
        Some(body),
    )
    .await)
}

#[tokio::test]
async fn omitted_assignees_do_not_restore_a_member_removed_during_patch() -> Result<()> {
    cleanup_during_patch(Editor::Owner).await
}

#[tokio::test]
async fn removing_the_editor_during_patch_still_denies_the_write() -> Result<()> {
    cleanup_during_patch(Editor::RemovedMember).await
}

#[derive(Debug, Clone, Copy)]
enum Editor {
    Owner,
    RemovedMember,
}

async fn cleanup_during_patch(editor: Editor) -> Result<()> {
    let mut f = Fixture::new().await?;
    matrix::set_role(&mut f, "member", "member", false).await?;
    let ids = vec![f.owner.id.0, f.actor.id.0];
    let assigned = patch(&mut f, Some(ids)).await?;
    assert_eq!(assigned.status, StatusCode::OK, "{}", assigned.json);
    let mut locker =
        PgConnection::connect(f.fresh.db.admin_url.as_ref().ok_or("admin URL")?.expose()).await?;
    let mut hold = locker.begin().await?;
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await?;
    // Old PATCH waits on candidate member rows; fixed PATCH waits on the lane.
    // Both waits happen inside app_update_task, after the HTTP handler read its snapshot.
    sqlx::query(
        "SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id IN ($2,$3) ORDER BY user_id FOR UPDATE",
    )
    .bind(f.ws)
    .bind(f.actor.id.0)
    .bind(f.owner.id.0)
    .execute(&mut *hold)
    .await?;
    sqlx::query("SELECT fvoci.app_lock_lane(status_id) FROM fvoci.tasks WHERE id=$1")
        .bind(f.tasks[0])
        .execute(&mut *hold)
        .await?;
    // Fixture locks need elevated access; the real cleanup runs as the application role.
    sqlx::query("SET LOCAL ROLE fvoci_app")
        .execute(&mut *hold)
        .await?;
    sqlx::query("SELECT set_config('app.user_id',$1,true)")
        .bind(f.owner.id.0.to_string())
        .execute(&mut *hold)
        .await?;
    let request = tokio::spawn({
        let app = common::app(&f.state);
        let user = match editor {
            Editor::Owner => f.owner.clone(),
            Editor::RemovedMember => f.actor.clone(),
        };
        let path = format!("/api/tasks/{}", f.tasks[0]);
        let body = json!({"base_version": assigned.json["version"], "title": "cleanup race saved"});
        async move { common::send(&app, Method::PATCH, &path, Some(&user), Some(body)).await }
    });
    wait_for_blocked_patch(&mut f.admin, holder).await?;
    sqlx::query("SELECT fvoci.app_remove_member($1,$2,$3)")
        .bind(f.ws)
        .bind(f.actor.id.0)
        .bind(sql_support::ids())
        .execute(&mut *hold)
        .await?;
    hold.commit().await?;
    locker.close().await?;
    let reply = tokio::time::timeout(Duration::from_secs(15), request).await??;
    // Check cleanup even on the old implementation's 400 failure.
    assert_eq!(assignees(&mut f).await?, vec![f.owner.id.0]);
    let writable = matches!(editor, Editor::Owner);
    assert_eq!(
        reply.status,
        if writable {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        },
        "{}",
        reply.json
    );
    let (title, version): (String, i32) =
        sqlx::query_as("SELECT title,version FROM fvoci.tasks WHERE id=$1")
            .bind(f.tasks[0])
            .fetch_one(&mut f.ddl)
            .await?;
    assert_eq!(
        title,
        if writable {
            "cleanup race saved"
        } else {
            assigned.json["title"].as_str().ok_or("title")?
        }
    );
    assert_eq!(
        i64::from(version),
        assigned.json["version"].as_i64().ok_or("version")? + i64::from(writable)
    );
    if !writable {
        assert!(reply.json.get("current").is_none(), "{}", reply.json);
    }
    let reason: String = sqlx::query_scalar(
        "SELECT detail->>'reason' FROM fvoci.audit_log WHERE target_id=$1 AND action='unassign'",
    )
    .bind(f.tasks[0])
    .fetch_one(&mut f.ddl)
    .await?;
    assert_eq!(reason, "remove");
    f.finish().await
}

async fn wait_for_blocked_patch(admin: &mut PgConnection, holder: i32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder)
            .fetch_one(&mut *admin)
            .await?;
            if blocked {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    Ok(())
}

async fn assignment_events(f: &mut Fixture) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT (SELECT count(*) FROM fvoci.audit_log WHERE target_id=$1 AND action IN ('assign','unassign')),
                (SELECT count(*) FROM fvoci.activity WHERE target_id=$1 AND kind IN ('assign','unassign'))",
    ).bind(f.tasks[0]).fetch_one(&mut f.ddl).await?)
}

#[tokio::test]
async fn explicit_lists_replace_deduplicate_preserve_and_clear_with_the_same_events() -> Result<()>
{
    let mut f = Fixture::new().await?;
    matrix::set_role(&mut f, "member", "member", false).await?;
    let owner = f.owner.id.0;
    let actor = f.actor.id.0;
    // Existing owner only -> add -> same (unordered duplicates) -> partial -> omitted -> empty.
    for (ids, expected, event_delta) in [
        (Some(vec![owner, actor]), vec![owner, actor], 1),
        (Some(vec![actor, owner, actor]), vec![owner, actor], 0),
        (Some(vec![actor]), vec![actor], 1),
        (None, vec![actor], 0),
        (Some(vec![]), vec![], 1),
    ] {
        let before = assignment_events(&mut f).await?;
        let reply = patch(&mut f, ids).await?;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.json);
        let mut expected = expected;
        expected.sort();
        assert_eq!(assignees(&mut f).await?, expected);
        assert_eq!(
            assignment_events(&mut f).await?,
            (before.0 + event_delta, before.1 + event_delta)
        );
    }
    f.finish().await
}

#[tokio::test]
async fn resaving_after_conflict_preserves_omitted_and_replaces_explicit_assignees() -> Result<()> {
    let mut f = Fixture::new().await?;
    matrix::set_role(&mut f, "member", "member", false).await?;
    let owner = f.owner.id.0;
    let actor = f.actor.id.0;
    for ids in [None, Some(vec![owner]), Some(vec![])] {
        let winner = patch(&mut f, Some(vec![actor])).await?;
        assert_eq!(winner.status, StatusCode::OK, "{}", winner.json);
        let mut body = json!({"base_version": 0, "title": "conflict retry"});
        if let Some(ids) = &ids {
            body["assignee_ids"] = json!(ids);
        }
        let app = common::app(&f.state);
        let path = format!("/api/tasks/{}", f.tasks[0]);
        let conflict = common::send(
            &app,
            Method::PATCH,
            &path,
            Some(&f.owner),
            Some(body.clone()),
        )
        .await;
        assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.json);
        assert_eq!(conflict.json["current"]["assignee_ids"], json!([actor]));
        assert_eq!(assignees(&mut f).await?, vec![actor]);
        body["base_version"] = conflict.json["current"]["version"].clone();
        let retry = common::send(&app, Method::PATCH, &path, Some(&f.owner), Some(body)).await;
        assert_eq!(retry.status, StatusCode::OK, "{}", retry.json);
        assert_eq!(assignees(&mut f).await?, ids.unwrap_or_else(|| vec![actor]));
    }
    f.finish().await
}

#[tokio::test]
async fn assignee_update_modes_keep_role_archive_and_deletion_permissions() -> Result<()> {
    let mut f = Fixture::new().await?;
    let mut cases = 0;
    for org in ["creator", "admin", "member", "none", "outside"] {
        for ws in ["owner", "admin", "member", "readonly", "none"] {
            for server in [false, true] {
                matrix::set_role(&mut f, org, ws, server).await?;
                for private in [false, true] {
                    for status in ["live", "archived", "deleted"] {
                        check_permissions(&mut f, org, ws, private, status).await?;
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 300);
    let reply = common::send(
        &common::app(&f.state),
        Method::PATCH,
        &format!("/api/tasks/{}?token=public-link", f.tasks[0]),
        None,
        Some(json!({"base_version": 1, "assignee_ids": []})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.json.get("current").is_none());
    f.finish().await
}

async fn check_permissions(
    f: &mut Fixture,
    org: &str,
    ws: &str,
    private: bool,
    status: &str,
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
        StatusCode::OK
    };
    for ids in [
        None,
        Some(vec![f.owner.id.0]),
        Some(vec![f.actor.id.0]),
        Some(vec![]),
    ] {
        let before = assignees(f).await?;
        let version: i32 = sqlx::query_scalar("SELECT version FROM fvoci.tasks WHERE id=$1")
            .bind(f.tasks[0])
            .fetch_one(&mut f.ddl)
            .await?;
        let mut body = json!({"base_version": version, "description": "role matrix"});
        if let Some(ids) = &ids {
            body["assignee_ids"] = json!(ids);
        }
        let reply = common::send(
            &common::app(&f.state),
            Method::PATCH,
            &format!("/api/tasks/{}", f.tasks[0]),
            Some(&f.actor),
            Some(body),
        )
        .await;
        let case = format!("{org}/{ws}/{private}/{status}/{ids:?}");
        assert_eq!(reply.status, expected, "{case}: {}", reply.json);
        assert_eq!(
            assignees(f).await?,
            if expected == StatusCode::OK {
                ids.unwrap_or(before)
            } else {
                before
            },
            "{case}"
        );
        let after: i32 = sqlx::query_scalar("SELECT version FROM fvoci.tasks WHERE id=$1")
            .bind(f.tasks[0])
            .fetch_one(&mut f.ddl)
            .await?;
        assert_eq!(
            after,
            version + i32::from(expected == StatusCode::OK),
            "{case}"
        );
        if expected != StatusCode::OK {
            assert!(reply.json.get("current").is_none(), "{case}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_assignee_must_see_the_item_even_for_an_administrator_patch() -> Result<()> {
    let mut f = Fixture::new().await?;
    let mut cases = 0;
    for org in ["creator", "admin", "member", "none", "outside"] {
        for ws in ["owner", "admin", "member", "readonly", "none"] {
            matrix::set_role(&mut f, org, ws, false).await?;
            for private in [false, true] {
                sqlx::query("UPDATE fvoci.items SET visibility=$2 WHERE id=$1")
                    .bind(f.item)
                    .bind(if private { "private" } else { "workspace" })
                    .execute(&mut f.ddl)
                    .await?;
                let cleared = patch(&mut f, Some(vec![])).await?;
                assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.json);
                let allowed = !matches!(org, "none" | "outside")
                    && ws != "none"
                    && (!private || matches!(ws, "owner" | "admin"));
                let actor = f.actor.id.0;
                let reply = patch(&mut f, Some(vec![actor])).await?;
                assert_eq!(
                    reply.status,
                    if allowed {
                        StatusCode::OK
                    } else {
                        StatusCode::BAD_REQUEST
                    },
                    "{org}/{ws}/{private}: {}",
                    reply.json
                );
                assert_eq!(
                    assignees(&mut f).await?,
                    if allowed { vec![actor] } else { vec![] }
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 50);
    f.finish().await
}
