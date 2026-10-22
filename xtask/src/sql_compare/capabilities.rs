//! Local capability profile, not a replica of a managed provider's control plane.
use super::*;

const INSTALLER: &str = "sql_compare_installer";
const OTHER_INSTALLER: &str = "sql_compare_other_installer";

fn admin_url(urls: &DbUrls, role: &str) -> Result<String> {
    let (_, host) = urls
        .admin
        .rsplit_once('@')
        .context("invalid fixture admin URL")?;
    Ok(format!(
        "postgres://{role}:{}@{host}",
        compose::TEST_MIGRATOR_PASSWORD
    ))
}

pub(super) async fn prepare(urls: &mut DbUrls) -> Result<()> {
    let mut conn = PgConnection::connect(&urls.admin).await?;
    let mut tx = conn.begin().await?;
    for role in [INSTALLER, OTHER_INSTALLER] {
        let statement: String = sqlx::query_scalar(
            "SELECT format('CREATE ROLE %I LOGIN CREATEROLE NOSUPERUSER NOCREATEDB NOBYPASSRLS NOREPLICATION PASSWORD %L', $1, $2)",
        )
        .bind(role)
        .bind(compose::TEST_MIGRATOR_PASSWORD)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::raw_sql(&statement).execute(&mut *tx).await?;
        sqlx::raw_sql(&format!(
            "ALTER ROLE {role} SET createrole_self_grant = ''; \
             GRANT CREATE ON DATABASE fvoci TO {role}; \
             GRANT CONNECT ON DATABASE fvoci TO {role} WITH GRANT OPTION;"
        ))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    conn.close().await?;
    // No fixture-superuser connection is used after this setup step.
    urls.admin = admin_url(urls, INSTALLER)?;
    Ok(())
}

async fn memberships(url: &str) -> Result<Value> {
    let mut conn = PgConnection::connect(url).await?;
    let rows: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('role',roleid::regrole::text, \
         'member',member::regrole::text,'grantor',grantor::regrole::text, \
         'admin',admin_option,'inherit',inherit_option,'set',set_option) ORDER BY roleid::regrole::text,grantor::regrole::text),'[]'::jsonb) \
         FROM pg_auth_members WHERE member=(SELECT oid FROM pg_roles WHERE rolname=current_user)",
    )
    .fetch_one(&mut conn)
    .await?;
    conn.close().await?;
    Ok(rows)
}

async fn denied(
    conn: &mut PgConnection,
    name: &str,
    statement: &str,
    transactional: bool,
) -> Result<Value> {
    if transactional {
        sqlx::raw_sql("BEGIN").execute(&mut *conn).await?;
    }
    let result = sqlx::raw_sql(statement).execute(&mut *conn).await;
    if transactional {
        sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await?;
    }
    let sqlstate = result
        .as_ref()
        .err()
        .and_then(|error| error.as_database_error())
        .and_then(|error| error.code())
        .map(|code| code.into_owned());
    Ok(
        json!({"probe":name,"expected_sqlstate":"42501","actual_sqlstate":sqlstate,
        "status":if sqlstate.as_deref()==Some("42501") {"EXPECTED_DENIAL"}else{"FAIL"}}),
    )
}

pub(super) async fn verify(
    sh: &Shell,
    urls: &DbUrls,
    candidate: Option<&Path>,
    dir: &Path,
    before: &Snapshot,
) -> Result<()> {
    let initial_memberships = memberships(&urls.admin).await?;
    let initial_ledger = std::fs::read(dir.join("migration-ledger.json"))?;
    let memberships_valid = initial_memberships.as_array().is_some_and(|rows| {
        rows.len() == 2
            && rows.iter().all(|row| {
                row["admin"] == true
                    && row["inherit"] == false
                    && row["set"] == false
                    && matches!(row["role"].as_str(), Some("fvoci_migrator" | "fvoci_app"))
            })
    });
    super::bootstrap(sh, urls)?;
    super::install_source(sh, urls, candidate).await?;
    let repeat_dir = dir.join("repeat");
    std::fs::create_dir(&repeat_dir)?;
    let repeated = super::snapshot(&urls.migration, &repeat_dir).await?;
    let final_memberships = memberships(&urls.admin).await?;
    let repeatability = json!({
        "bootstrap_and_migration_repeated":true,
        "catalog_equal":before==&repeated,
        "sqlx_ledger_equal":initial_ledger==std::fs::read(repeat_dir.join("migration-ledger.json"))?,
        "memberships_before":initial_memberships,"memberships_after_fresh_connection":final_memberships,
        "creator_admin_without_set_or_inherit":memberships_valid,
        "memberships_restored":initial_memberships==final_memberships
    });

    let other = DbUrls {
        admin: admin_url(urls, OTHER_INSTALLER)?,
        migration: urls.migration.clone(),
        app: urls.app.clone(),
    };
    let rejected = {
        let _admin = sh.push_env("DATABASE_ADMIN_URL", &other.admin);
        let _migrator = sh.push_env("FVOCI_DB_MIGRATOR_PASSWORD", compose::TEST_APP_PASSWORD);
        let _app = sh.push_env("FVOCI_DB_APP_PASSWORD", compose::TEST_MIGRATOR_PASSWORD);
        cmd!(
            sh,
            "cargo run --quiet --package fvoci-server --locked -- db bootstrap"
        )
        .ignore_status()
        .output()?
    };
    let rejection_expected = !rejected.status.success()
        && [&rejected.stdout, &rejected.stderr]
            .iter()
            .any(|stream| String::from_utf8_lossy(stream).contains("no ADMIN OPTION"));
    let mut conn = PgConnection::connect(&urls.migration).await?;
    let after_rejection = super::catalog(&mut conn).await?;
    conn.close().await?;
    let rejected_unchanged = before == &after_rejection;
    // The rejected request used different fixed test passwords. Fresh connections with
    // the original credentials prove that no partial password update survived.
    let mut credentials_preserved = true;
    for url in [&urls.migration, &urls.app] {
        match PgConnection::connect(url).await {
            Ok(conn) => conn.close().await?,
            Err(_) => credentials_preserved = false,
        }
    }

    let mut admin = PgConnection::connect(&urls.admin).await?;
    let database: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('owner',datdba::regrole::text,
          'create',has_database_privilege(current_user,oid,'CREATE'),
          'connect',has_database_privilege(current_user,oid,'CONNECT'),
          'create_grant_option',has_database_privilege(current_user,oid,'CREATE WITH GRANT OPTION'),
          'connect_grant_option',has_database_privilege(current_user,oid,'CONNECT WITH GRANT OPTION'),
          'createrole_self_grant',current_setting('createrole_self_grant'))
         FROM pg_database WHERE datname=current_database()",
    ).fetch_one(&mut admin).await?;
    admin.close().await?;
    let database_profile_valid = database["owner"] != INSTALLER
        && database["create"] == true
        && database["connect"] == true
        && database["create_grant_option"] == false
        && database["connect_grant_option"] == true
        && database["createrole_self_grant"] == "";

    let mut probes = Vec::new();
    for (role, url) in [
        (INSTALLER, &urls.admin),
        ("fvoci_migrator", &urls.migration),
        ("fvoci_app", &urls.app),
    ] {
        let mut conn = PgConnection::connect(url).await?;
        let properties: Value = sqlx::query_scalar(
            "SELECT to_jsonb(r)-'oid'-'rolpassword' FROM pg_roles r WHERE rolname=current_user",
        )
        .fetch_one(&mut conn)
        .await?;
        let mut denials = Vec::new();
        denials.push(
            denied(
                &mut conn,
                "set_replication_role",
                "SET LOCAL session_replication_role = replica",
                true,
            )
            .await?,
        );
        denials.push(
            denied(
                &mut conn,
                "create_database",
                &format!(
                    "CREATE DATABASE sql_compare_denied_{}",
                    uuid::Uuid::now_v7().simple()
                ),
                false,
            )
            .await?,
        );
        denials.push(
            denied(
                &mut conn,
                "become_superuser",
                &format!("ALTER ROLE {role} SUPERUSER"),
                true,
            )
            .await?,
        );
        denials.push(
            denied(
                &mut conn,
                "set_fixture_superuser",
                "SET LOCAL ROLE fvoci",
                true,
            )
            .await?,
        );
        if role == INSTALLER {
            denials.push(
                denied(
                    &mut conn,
                    "read_app_table_without_membership",
                    "SELECT count(*) FROM fvoci.documents",
                    true,
                )
                .await?,
            );
        } else {
            denials.push(
                denied(
                    &mut conn,
                    "create_role",
                    "CREATE ROLE sql_compare_denied_role",
                    true,
                )
                .await?,
            );
            denials.push(
                denied(
                    &mut conn,
                    "create_schema",
                    "CREATE SCHEMA sql_compare_denied_schema",
                    true,
                )
                .await?,
            );
        }
        if role == "fvoci_app" {
            denials.push(
                denied(
                    &mut conn,
                    "create_app_table",
                    "CREATE TABLE fvoci.sql_compare_denied_table(id integer)",
                    true,
                )
                .await?,
            );
            denials.push(
                denied(
                    &mut conn,
                    "disable_rls",
                    "ALTER TABLE fvoci.documents DISABLE ROW LEVEL SECURITY",
                    true,
                )
                .await?,
            );
            denials.push(
                denied(
                    &mut conn,
                    "set_migrator",
                    "SET LOCAL ROLE fvoci_migrator",
                    true,
                )
                .await?,
            );
        }
        conn.close().await?;
        let attributes_valid = ["rolsuper", "rolcreatedb", "rolbypassrls", "rolreplication"]
            .iter()
            .all(|name| properties[*name] == false)
            && properties["rolcanlogin"] == true
            && properties["rolcreaterole"] == (role == INSTALLER);
        probes.push(json!({"role":role,"properties":properties,"profile_attributes_valid":attributes_valid,"denials":denials}));
    }
    let all_denied = probes.iter().all(|role| {
        role["denials"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["status"] == "EXPECTED_DENIAL"))
    });
    let evidence = json!({
        "profile":"restricted-installer-v1","provider_replica":false,
        "fixture_setup":"Local superuser created only two installer roles and granted target database CREATE plus CONNECT WITH GRANT OPTION. Product bootstrap and migration use restricted roles.",
        "repeatability":repeatability,
        "database":database,"database_profile_valid":database_profile_valid,
        "unrelated_installer_bootstrap":{"status":if rejection_expected {"EXPECTED_DENIAL"}else{"FAIL"},"expected_error":"no ADMIN OPTION","catalog_unchanged":rejected_unchanged,"different_passwords_requested":true,"original_credentials_preserved":credentials_preserved},
        "roles":probes,
        "restore":{"status":"NOT SUPPORTED by this capability profile","reason":"session_replication_role SET is denied; no grant was added"}
    });
    super::write_json(&dir.join("capabilities.json"), &evidence)?;
    ensure!(
        memberships_valid
            && initial_memberships == final_memberships
            && before == &repeated
            && evidence["repeatability"]["sqlx_ledger_equal"] == true
            && rejection_expected
            && rejected_unchanged
            && all_denied
            && database_profile_valid
            && credentials_preserved
            && probes
                .iter()
                .all(|role| role["profile_attributes_valid"] == true),
        "capability verification failed; see {}/capabilities.json",
        dir.display()
    );
    Ok(())
}
