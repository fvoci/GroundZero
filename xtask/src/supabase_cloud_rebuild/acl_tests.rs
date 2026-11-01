//! A catalog reader must reject dormant direct ACLs as well as reachable privileges.

use super::catalog;
use anyhow::Result;
use sqlx::PgConnection;

async fn sql(conn: &mut PgConnection, text: &str) -> Result<()> {
    sqlx::raw_sql(text).execute(conn).await?;
    Ok(())
}

pub(super) async fn run(
    admin: &mut PgConnection,
    reader: &mut PgConnection,
) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for grantee in ["cloud_fixture_reader", "PUBLIC"] {
        for (kind, name, privileges) in [
            (
                "TABLE",
                "fvoci.legacy",
                &[
                    "SELECT",
                    "INSERT",
                    "UPDATE",
                    "DELETE",
                    "TRUNCATE",
                    "REFERENCES",
                    "TRIGGER",
                    "MAINTAIN",
                ][..],
            ),
            (
                "SEQUENCE",
                "fvoci.legacy_sequence",
                &["USAGE", "SELECT", "UPDATE"][..],
            ),
            ("FUNCTION", "fvoci.legacy_function()", &["EXECUTE"][..]),
        ] {
            for privilege in privileges {
                sql(
                    admin,
                    &format!("GRANT {privilege} ON {kind} {name} TO {grantee}"),
                )
                .await?;
                if catalog::inspect_authority(reader).await.is_ok() {
                    failures.push(format!("reader accepted {grantee} {kind} {privilege}"));
                }
                sql(
                    admin,
                    &format!("REVOKE {privilege} ON {kind} {name} FROM {grantee}"),
                )
                .await?;
            }
        }
        sql(
            admin,
            &format!("GRANT UPDATE(secret) ON fvoci.legacy TO {grantee}"),
        )
        .await?;
        if catalog::inspect_authority(reader).await.is_ok() {
            failures.push(format!("reader accepted {grantee} column UPDATE"));
        }
        sql(
            admin,
            &format!("REVOKE UPDATE(secret) ON fvoci.legacy FROM {grantee}"),
        )
        .await?;
    }
    for (kind, name, owner) in [
        ("TABLE", "fvoci.legacy", "cloud_fixture_admin"),
        ("TYPE", "fvoci.legacy_state", "fvoci_migrator"),
        ("FUNCTION", "fvoci.legacy_function()", "fvoci_migrator"),
    ] {
        sql(
            admin,
            &format!("ALTER {kind} {name} OWNER TO cloud_fixture_reader"),
        )
        .await?;
        if catalog::inspect_authority(reader).await.is_ok() {
            failures.push(format!("reader accepted {kind} ownership"));
        }
        sql(admin, &format!("ALTER {kind} {name} OWNER TO {owner}")).await?;
    }
    sql(admin,"CREATE FUNCTION fvoci.app_normalize_key(p_key text) RETURNS text LANGUAGE sql AS 'SELECT p_key'").await?;
    if catalog::inspect_authority(reader).await.is_err() {
        failures.push("PUBLIC invoker helper without schema USAGE was rejected".to_owned());
    }
    sql(
        admin,
        "GRANT EXECUTE ON FUNCTION fvoci.app_normalize_key(text) TO cloud_fixture_reader",
    )
    .await?;
    if catalog::inspect_authority(reader).await.is_ok() {
        failures.push("direct helper EXECUTE was accepted".to_owned());
    }
    sql(admin,"REVOKE EXECUTE ON FUNCTION fvoci.app_normalize_key(text) FROM cloud_fixture_reader; ALTER FUNCTION fvoci.app_normalize_key(text) SECURITY DEFINER").await?;
    if catalog::inspect_authority(reader).await.is_ok() {
        failures.push("PUBLIC definer helper was accepted".to_owned());
    }
    sql(
        admin,
        "DROP FUNCTION fvoci.app_normalize_key(text) RESTRICT",
    )
    .await?;
    if catalog::inspect_authority(reader).await.is_err() {
        failures.push("clean catalog reader rejected after ACL cleanup".to_owned());
    }
    Ok(failures)
}
