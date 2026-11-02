use super::{Result, fixture::Fixture};
use serde_json::{Value, json};

struct Expected {
    item: bool,
    task: bool,
    root: bool,
    grade: &'static str,
    archived: bool,
}

pub(super) const OUTCOME: &str = r"
CREATE FUNCTION fvoci.screen_outcome(kind text, target uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY INVOKER SET search_path=fvoci,pg_temp AS $$
BEGIN
  CASE kind
    WHEN 'bundle' THEN RETURN app_task_bundle_read(target);
    WHEN 'context' THEN RETURN app_task_context_read(target);
    WHEN 'board' THEN RETURN app_task_board_read(target);
    WHEN 'detail' THEN RETURN app_task_detail_read(target);
    WHEN 'options' THEN RETURN app_task_detail_options_read(target);
    WHEN 'time' THEN RETURN to_jsonb(app_task_time(target));
  END CASE;
EXCEPTION WHEN OTHERS THEN RETURN jsonb_build_object('code', SQLSTATE);
END $$;
REVOKE ALL ON FUNCTION fvoci.screen_outcome(text,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.screen_outcome(text,uuid) TO fvoci_app;
";

#[tokio::test]
async fn every_role_crosses_visibility_item_relation_archive_and_deletion() -> Result<()> {
    let mut f = Fixture::new().await?;
    sqlx::raw_sql(OUTCOME).execute(&mut f.ddl).await?;
    let mut count = 0usize;
    for organization in ["creator", "admin", "member", "none", "outside"] {
        for workspace in ["owner", "admin", "member", "readonly", "none"] {
            for server_admin in [false, true] {
                set_role(&mut f, organization, workspace, server_admin).await?;
                for visibility in ["workspace", "private"] {
                    for relation in [
                        "creator",
                        "direct",
                        "group",
                        "none",
                        "creator-direct",
                        "creator-group",
                        "direct-group",
                        "all",
                    ] {
                        set_relation(&mut f, visibility, relation).await?;
                        for archive in ["none", "item", "module", "both"] {
                            for deleted in [
                                "none",
                                "task",
                                "root",
                                "tasks",
                                "item",
                                "module",
                                "workspace",
                            ] {
                                set_state(&mut f, archive, deleted).await?;
                                let item_visible = !matches!(organization, "none" | "outside")
                                    && workspace != "none"
                                    && (matches!(workspace, "owner" | "admin")
                                        || visibility == "workspace"
                                        || relation != "none")
                                    && !matches!(deleted, "item" | "module" | "workspace");
                                let task_visible =
                                    item_visible && !matches!(deleted, "task" | "tasks");
                                let case = format!(
                                    "{organization}/{workspace}/{server_admin}/{visibility}/{relation}/{archive}/{deleted}"
                                );
                                let root_visible =
                                    item_visible && !matches!(deleted, "root" | "tasks");
                                let grade = if matches!(workspace, "owner" | "admin") {
                                    "owner"
                                } else if workspace == "readonly" {
                                    "read"
                                } else if relation.contains("creator") || relation == "all" {
                                    "owner"
                                } else {
                                    "write"
                                };
                                compare(
                                    &f,
                                    &Expected {
                                        item: item_visible,
                                        task: task_visible,
                                        root: root_visible,
                                        grade,
                                        archived: archive != "none",
                                    },
                                    &case,
                                )
                                .await?;
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        count, 22400,
        "the security axes are a complete Cartesian product"
    );
    f.finish().await
}

pub(super) async fn set_role(f: &mut Fixture, org: &str, ws: &str, server: bool) -> Result<()> {
    let actor = f.actor.id.0;
    sqlx::query("DELETE FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2")
        .bind(f.org)
        .bind(actor)
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("DELETE FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2")
        .bind(f.other_org)
        .bind(actor)
        .execute(&mut f.ddl)
        .await?;
    if org == "outside" {
        sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,'member')").bind(f.other_org).bind(actor).execute(&mut f.ddl).await?;
    } else if org != "none" {
        sqlx::query(
            "INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,$3)",
        )
        .bind(f.org)
        .bind(actor)
        .bind(if org == "member" { "member" } else { "admin" })
        .execute(&mut f.ddl)
        .await?;
    }
    sqlx::query("UPDATE fvoci.organizations SET created_by=$2 WHERE id=$1")
        .bind(f.org)
        .bind(if org == "creator" {
            actor
        } else {
            f.owner.id.0
        })
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("UPDATE fvoci.workspaces SET owner_id=$2 WHERE id=$1")
        .bind(f.ws)
        .bind(if ws == "owner" { actor } else { f.owner.id.0 })
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("DELETE FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2")
        .bind(f.ws)
        .bind(actor)
        .execute(&mut f.ddl)
        .await?;
    if ws != "none" {
        sqlx::query(
            "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,$3)",
        )
        .bind(f.ws)
        .bind(actor)
        .bind(if ws == "owner" { "admin" } else { ws })
        .execute(&mut f.ddl)
        .await?;
    }
    sqlx::query("DELETE FROM fvoci.server_admins WHERE user_id=$1")
        .bind(actor)
        .execute(&mut f.ddl)
        .await?;
    if server {
        sqlx::query("INSERT INTO fvoci.server_admins(user_id) VALUES($1)")
            .bind(actor)
            .execute(&mut f.ddl)
            .await?;
    }
    Ok(())
}

async fn set_relation(f: &mut Fixture, visibility: &str, relation: &str) -> Result<()> {
    sqlx::query("UPDATE fvoci.items SET visibility=$2,created_by=$3 WHERE id=$1")
        .bind(f.item)
        .bind(visibility)
        .bind(if relation.contains("creator") || relation == "all" {
            f.actor.id.0
        } else {
            f.owner.id.0
        })
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("DELETE FROM fvoci.item_members WHERE item_id=$1")
        .bind(f.item)
        .execute(&mut f.ddl)
        .await?;
    if relation.contains("direct") || relation == "all" {
        sqlx::query("INSERT INTO fvoci.item_members(item_id,user_id,added_by) VALUES($1,$2,$3)")
            .bind(f.item)
            .bind(f.actor.id.0)
            .bind(f.owner.id.0)
            .execute(&mut f.ddl)
            .await?;
    }
    if relation.contains("group") || relation == "all" {
        sqlx::query("INSERT INTO fvoci.item_members(item_id,group_id,added_by) VALUES($1,$2,$3)")
            .bind(f.item)
            .bind(f.group)
            .bind(f.owner.id.0)
            .execute(&mut f.ddl)
            .await?;
        sqlx::query("INSERT INTO fvoci.group_members(group_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(f.group).bind(f.actor.id.0).execute(&mut f.ddl).await?;
    }
    Ok(())
}

async fn set_state(f: &mut Fixture, archive: &str, deleted: &str) -> Result<()> {
    sqlx::query("UPDATE fvoci.items SET archived_at=CASE WHEN $2 THEN now() END,deleted_at=CASE WHEN $3 THEN now() END WHERE id=$1").bind(f.item).bind(matches!(archive,"item"|"both")).bind(deleted=="item").execute(&mut f.ddl).await?;
    sqlx::query("UPDATE fvoci.modules SET archived_at=CASE WHEN $2 THEN now() END,deleted_at=CASE WHEN $3 THEN now() END WHERE id=$1").bind(f.module).bind(matches!(archive,"module"|"both")).bind(deleted=="module").execute(&mut f.ddl).await?;
    sqlx::query("UPDATE fvoci.workspaces SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1")
        .bind(f.ws)
        .bind(deleted == "workspace")
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("UPDATE fvoci.tasks SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1")
        .bind(f.tasks[0])
        .bind(matches!(deleted, "task" | "tasks"))
        .execute(&mut f.ddl)
        .await?;
    sqlx::query("UPDATE fvoci.tasks SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1")
        .bind(f.roots[0])
        .bind(matches!(deleted, "root" | "tasks"))
        .execute(&mut f.ddl)
        .await?;
    Ok(())
}

async fn compare(f: &Fixture, expected_scope: &Expected, case: &str) -> Result<()> {
    let Expected {
        item: item_visible,
        task: task_visible,
        root: root_visible,
        grade,
        archived,
    } = *expected_scope;
    let mut tx = f.state.db.begin_as(f.actor.id).await?;
    let (single, listed, context, board, detail, options): (bool, Vec<uuid::Uuid>, Value, Value, Value, Value) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM fvoci.tasks WHERE id=$2),
        ARRAY(SELECT id FROM fvoci.tasks WHERE item_id=$1 ORDER BY number),
        fvoci.screen_outcome('context',$1),fvoci.screen_outcome('board',$1),fvoci.screen_outcome('detail',$2),fvoci.screen_outcome('options',$1)",
    ).bind(f.item).bind(f.tasks[0]).fetch_one(tx.conn()).await?;
    assert_eq!(single, task_visible, "single RLS: {case}");
    let expected: Vec<_> = [(f.tasks[0], task_visible), (f.roots[0], root_visible)]
        .into_iter()
        .filter_map(|(id, visible)| visible.then_some(id))
        .collect();
    assert_eq!(listed, expected, "list RLS: {case}");
    if item_visible {
        let (statuses,assignable):(Value,Value) = sqlx::query_as("SELECT (SELECT coalesce(jsonb_agg(to_jsonb(s)),'[]'::jsonb) FROM fvoci.app_task_statuses($1) s),(SELECT coalesce(jsonb_agg(to_jsonb(a)),'[]'::jsonb) FROM fvoci.app_item_assignable($1) a)")
            .bind(f.item).fetch_one(tx.conn()).await?;
        assert_eq!(
            options["viewer_id"],
            json!(f.actor.id.0),
            "options viewer: {case}"
        );
        assert_eq!(options["item_id"], json!(f.item), "options scope: {case}");
        assert_eq!(
            options["statuses"],
            json!({"state":"ready","value":statuses}),
            "options statuses: {case}"
        );
        assert_eq!(
            options["assignable"],
            json!({"state":"ready","value":assignable}),
            "options assignable: {case}"
        );
    } else {
        assert_eq!(options["code"], json!("P0002"), "options denial: {case}");
    }
    for (name, value, visible) in [
        ("context", context, item_visible),
        ("board", board, item_visible),
        ("detail", detail, task_visible),
    ] {
        if visible {
            assert!(value.get("code").is_none(), "{name}: {case}: {value}");
            assert_eq!(value["viewer_id"], json!(f.actor.id.0), "{case}");
            if name != "context" {
                assert_eq!(value["archived"], json!(archived), "{name}: {case}");
                let access = if name == "board" {
                    &value["item_access"]
                } else {
                    &value["item"]["access"]
                };
                assert_eq!(*access, json!(grade), "{name}: {case}");
                let rows = value["tasks"].as_array().ok_or("missing tasks")?;
                let ids: Vec<Value> = rows.iter().map(|row| row["id"].clone()).collect();
                assert_eq!(
                    ids,
                    expected.iter().map(|id| json!(id)).collect::<Vec<_>>(),
                    "{name}: {case}"
                );
                if name == "board" {
                    let count: i64 = value["statuses"]
                        .as_array()
                        .ok_or("statuses")?
                        .iter()
                        .filter_map(|row| row["task_count"].as_i64())
                        .sum();
                    assert_eq!(
                        count,
                        i64::from(task_visible) + i64::from(root_visible),
                        "status count: {case}"
                    );
                }
                assert_eq!(
                    value["assignees"].as_array().ok_or("assignees")?.len(),
                    usize::from(task_visible),
                    "{name}: {case}"
                );
                assert_eq!(
                    value["checklist"].as_array().ok_or("checklist")?.len(),
                    usize::from(task_visible),
                    "{name}: {case}"
                );
                assert_eq!(
                    value["dependencies"]
                        .as_array()
                        .ok_or("dependencies")?
                        .len(),
                    usize::from(task_visible && root_visible),
                    "{name}: {case}"
                );
                assert_eq!(
                    value["series"].as_array().ok_or("series")?.len(),
                    usize::from(root_visible),
                    "{name}: {case}"
                );
                if root_visible {
                    assert_eq!(
                        value["series"][0]["root_task_id"],
                        json!(f.roots[0]),
                        "{case}"
                    );
                }
                if task_visible {
                    assert_eq!(
                        value["assignees"][0]["task_id"],
                        json!(f.tasks[0]),
                        "{case}"
                    );
                    assert_eq!(
                        value["checklist"][0]["task_id"],
                        json!(f.tasks[0]),
                        "{case}"
                    );
                }
            }
        } else {
            assert_eq!(value["code"], json!("P0002"), "{name}: {case}: {value}");
        }
    }
    tx.commit().await?;
    Ok(())
}
