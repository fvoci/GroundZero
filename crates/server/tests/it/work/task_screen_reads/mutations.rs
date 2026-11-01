use super::{Result, fixture::Fixture, matrix, reference};
use serde_json::{Value, json};

/// The same assertions run normally and with exactly one injected aggregate defect.
/// The local mutation runner requires a failing cargo exit for every named mutation.
#[tokio::test]
async fn aggregate_scope_equivalence() -> Result<()> {
    let mut f = Fixture::new().await?;
    sqlx::raw_sql(matrix::OUTCOME).execute(&mut f.ddl).await?;
    if let Ok(mutation) = std::env::var("FVOCI_TASK_SCREEN_MUTATION") {
        install(&mut f, &mutation).await?;
    }
    let mut tx = f.state.db.begin_as(f.actor.id).await?;
    let denied: Value = sqlx::query_scalar("SELECT fvoci.screen_outcome('bundle',$1)")
        .bind(f.item)
        .fetch_one(tx.conn())
        .await?;
    assert_eq!(
        denied["code"],
        json!("P0002"),
        "an invisible scope is 404, including an empty RLS result"
    );
    tx.commit().await?;
    reference::compare(&f, f.item, f.tasks[0]).await?;
    reference::compare(&f, f.other, f.tasks[1]).await?;
    f.finish().await
}

async fn install(f: &mut Fixture, name: &str) -> Result<()> {
    let (function, before, after) = match name {
        "entry" => (
            "app_task_bundle_read",
            "IF v_grade = 'none' THEN",
            "IF false THEN",
        ),
        "tasks" => (
            "app_task_bundle_read",
            "WHERE t.item_id=p_item ORDER BY s.position",
            "ORDER BY s.position",
        ),
        "assignees" => (
            "app_task_bundle_read",
            "WHERE t.item_id=p_item ORDER BY a.task_id",
            "ORDER BY a.task_id",
        ),
        "dependencies" => (
            "app_task_bundle_read",
            "WHERE item_id=p_item ORDER BY created_at",
            "ORDER BY created_at",
        ),
        "series" => (
            "app_task_bundle_read",
            "WHERE s.item_id=p_item ORDER BY s.id",
            "ORDER BY s.id",
        ),
        "checklist" => (
            "app_task_bundle_read",
            "WHERE t.item_id=p_item ORDER BY c.task_id",
            "ORDER BY c.task_id",
        ),
        "documents" => (
            "app_task_context_read",
            "FROM documents WHERE item_id=p_item",
            "FROM documents",
        ),
        "previous-keys" => ("app_task_context_read", "o.module_id=m.id AND ", ""),
        "document-parent" => ("app_task_context_read", "ON p.id=d.parent_id", "ON true"),
        "item" => (
            "app_task_context_read",
            "FROM items i WHERE i.id=p_item;",
            "FROM items i;",
        ),
        "module" => ("app_task_context_read", "WHERE m.id=v_module_id;", ";"),
        "document-reference" => ("app_task_context_read", "WHERE i.id=p_item)||", " )||"),
        "task" => ("app_task_detail_read", "WHERE t.id=p_task", ""),
        "detail-item" => (
            "app_task_detail_read",
            "FROM items i WHERE i.id=v_item;",
            "FROM items i;",
        ),
        _ => return Err(format!("unknown scope mutation {name}").into()),
    };
    let definition: String = sqlx::query_scalar("SELECT pg_get_functiondef(to_regprocedure($1))")
        .bind(format!("fvoci.{function}(uuid)"))
        .fetch_one(&mut f.ddl)
        .await?;
    assert_eq!(
        definition.matches(before).count(),
        1,
        "one predicate per mutation: {name}"
    );
    sqlx::raw_sql(&definition.replacen(before, after, 1))
        .execute(&mut f.ddl)
        .await?;
    eprintln!("MUTATION_INSTALLED {name}");
    Ok(())
}
