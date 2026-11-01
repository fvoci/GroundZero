//! Compare every public resource field against the still-supported granular HTTP reads.
use super::{Result, fixture::Fixture};
use crate::common;
use axum::http::{Method, StatusCode};
use serde_json::Value;
use uuid::Uuid;

async fn get(f: &Fixture, path: &str) -> Result<Value> {
    let reply = common::send(
        &common::app(&f.state),
        Method::GET,
        path,
        Some(&f.owner),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.json);
    Ok(normalize(reply.json))
}

fn normalize(mut value: Value) -> Value {
    match &mut value {
        Value::Object(fields) => {
            for (name, child) in fields {
                if matches!(name.as_str(), "updated_at" | "completed_at") {
                    if let Some(text) = child.as_str()
                        && let Ok(date) = time::OffsetDateTime::parse(
                            text,
                            &time::format_description::well_known::Rfc3339,
                        )
                        && let Ok(formatted) =
                            date.format(&time::format_description::well_known::Rfc3339)
                    {
                        *child = Value::String(formatted);
                    }
                } else {
                    *child = normalize(child.take());
                }
            }
        }
        Value::Array(rows) => {
            for row in rows {
                *row = normalize(row.take());
            }
        }
        _ => {}
    }
    value
}

pub(super) async fn compare(f: &Fixture, item: Uuid, task: Uuid) -> Result<()> {
    let source_item = get(f, &format!("/api/items/{item}")).await?;
    let source_module = get(
        f,
        &format!(
            "/api/modules/{}",
            source_item["module_id"].as_str().ok_or("module id")?
        ),
    )
    .await?;
    let context = get(f, &format!("/api/items/{item}/task-context")).await?;
    assert_eq!(context["item"], source_item);
    assert_eq!(context["module"], source_module);
    assert_eq!(
        context["documents"],
        get(f, &format!("/api/items/{item}/documents")).await?
    );
    let board = get(f, &format!("/api/items/{item}/task-board")).await?;
    let detail = get(f, &format!("/api/tasks/{task}/detail-read")).await?;
    let options = get(f, &format!("/api/items/{item}/task-detail-options")).await?;
    assert_eq!(detail["task"], get(f, &format!("/api/tasks/{task}")).await?);
    assert_eq!(detail["item"], source_item);
    let tasks = get(f, &format!("/api/items/{item}/tasks")).await?;
    assert_eq!(board["tasks"], tasks);
    assert_eq!(detail["tasks"], tasks);
    for (field, path) in [
        ("assignees", "task-assignees"),
        ("dependencies", "task-dependencies"),
        ("series", "task-series"),
        ("checklist", "checklist-items"),
    ] {
        let original = get(f, &format!("/api/{path}?item={item}")).await?;
        assert_eq!(board[field], original, "board {field}");
        assert_eq!(detail[field], original, "detail {field}");
    }
    let statuses = get(f, &format!("/api/items/{item}/statuses")).await?;
    assert_eq!(board["statuses"], statuses);
    assert_eq!(options["statuses"]["value"], statuses);
    assert_eq!(
        options["assignable"]["value"],
        get(f, &format!("/api/items/{item}/assignable")).await?
    );
    Ok(())
}
