//! #291 C1 is equivalent only while the caller's items RLS actually applies.
use crate::common;

#[tokio::test]
async fn calendar_runs_as_non_owner_with_rls_enabled() -> Result<(), fvoci_core::error::AppError> {
    let state = common::state();
    let user = common::login(&state, "calendar invoker witness").await;
    let mut tx = state.db.begin_as(user.id).await?;
    let invoker: bool = sqlx::query_scalar(
        "SELECT NOT prosecdef FROM pg_proc
         WHERE oid='fvoci.app_calendar(uuid,date,date,text,uuid,uuid)'::regprocedure",
    )
    .fetch_one(tx.conn())
    .await?;
    assert!(invoker, "C1 requires app_calendar SECURITY INVOKER");
    let protected: bool = sqlx::query_scalar(
        "SELECT current_user='fvoci_app' AND NOT r.rolsuper AND NOT r.rolbypassrls
           AND current_setting('row_security')='on'
           AND bool_and(c.relrowsecurity AND row_security_active(c.oid) AND c.relowner<>r.oid
                        AND NOT pg_has_role(r.oid,c.relowner,'USAGE'))
         FROM pg_roles r CROSS JOIN pg_class c
         WHERE r.rolname=current_user AND c.oid IN
           ('fvoci.items'::regclass,'fvoci.tasks'::regclass,'fvoci.events'::regclass)
         GROUP BY r.oid,r.rolsuper,r.rolbypassrls",
    )
    .fetch_one(tx.conn())
    .await?;
    assert!(
        protected,
        "calendar comparisons must execute under caller RLS"
    );
    tx.commit().await
}
