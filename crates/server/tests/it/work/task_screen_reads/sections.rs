use super::{Result, fixture::Fixture};
use crate::common;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

#[tokio::test]
async fn read_sections_propagate_privilege_not_found_and_authentication_errors() -> Result<()> {
    let mut f = Fixture::new().await?;
    for function in ["app_task_statuses", "app_item_assignable"] {
        let definition: String =
            sqlx::query_scalar("SELECT pg_get_functiondef(to_regprocedure($1))")
                .bind(format!("fvoci.{function}(uuid)"))
                .fetch_one(&mut f.ddl)
                .await?;
        for code in ["42501", "P0002", "28000", "28P01", "22012"] {
            let mutated = definition
                .find("AS $function$")
                .ok_or("function delimiter")?;
            let replacement = format!(
                "{}AS $function$ BEGIN RAISE EXCEPTION 'injected section failure' USING ERRCODE='{code}'; END $function$",
                &definition[..mutated]
            );
            sqlx::raw_sql(&replacement).execute(&mut f.ddl).await?;
            let mut tx = f.state.db.begin_as(f.owner.id).await?;
            let result =
                sqlx::query_scalar::<_, Value>("SELECT fvoci.app_task_detail_options_read($1)")
                    .bind(f.item)
                    .fetch_one(tx.conn())
                    .await;
            if code == "22012" {
                let value = result?;
                let section = if function == "app_task_statuses" {
                    "statuses"
                } else {
                    "assignable"
                };
                assert_eq!(
                    value[section],
                    json!({"state":"failed","error":"unavailable"})
                );
            } else {
                let error = result
                    .err()
                    .ok_or("a permission/authentication error must escape the section")?;
                assert_eq!(
                    error
                        .as_database_error()
                        .and_then(|error| error.code())
                        .as_deref(),
                    Some(code)
                );
            }
            drop(tx); // UserTx rolls back on drop, including an aborted section statement.
            let reply = common::send(
                &common::app(&f.state),
                Method::GET,
                &format!("/api/items/{}/task-detail-options", f.item),
                Some(&f.owner),
                None,
            )
            .await;
            assert_eq!(
                reply.status,
                match code {
                    "42501" => StatusCode::FORBIDDEN,
                    "P0002" => StatusCode::NOT_FOUND,
                    "28000" | "28P01" => StatusCode::UNAUTHORIZED,
                    _ => StatusCode::OK,
                },
                "HTTP section propagation: {function}/{code}"
            );
        }
        sqlx::raw_sql(&definition).execute(&mut f.ddl).await?;
    }
    f.finish().await
}
