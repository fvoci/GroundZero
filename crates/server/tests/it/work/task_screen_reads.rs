//! #291: first fail without the aggregate functions; implementation and matrices follow here.
use crate::common;
use sqlx::Connection;

mod fixture;
mod matrix;
mod mutations;
mod patch_conflict;
mod privacy;
mod reference;
mod sections;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn task_screen_functions_keep_the_application_role_and_rls() -> Result<()> {
    let mut admin = sqlx::PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
    let fresh = common::fresh_database(&mut admin, "task_screen_permissions").await;
    let mut read = sqlx::PgConnection::connect(&fresh.app_url).await?;
    for name in [
        "app_task_bundle_read",
        "app_task_context_read",
        "app_task_board_read",
        "app_task_detail_read",
        "app_task_detail_options_read",
    ] {
        let invoker: bool =
            sqlx::query_scalar("SELECT NOT prosecdef FROM pg_proc WHERE oid = to_regprocedure($1)")
                .bind(format!("fvoci.{name}(uuid)"))
                .fetch_one(&mut read)
                .await?;
        assert!(invoker, "{name} must preserve RLS");
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut read)
    .await?;
    assert!(!privileged);
    read.close().await?;
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fresh.name))
        .execute(&mut admin)
        .await?;
    Ok(())
}
