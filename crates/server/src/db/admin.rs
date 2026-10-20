//! `db bootstrap`과 `db migrate`. 운영자·Compose·CI가 같은 경로로 역할과 스키마를 준비한다.
//!
//! - bootstrap: 관리자 연결로 `fvoci_migrator`(스키마 소유)와 `fvoci_app`(`NOBYPASSRLS`)을 만들거나
//!   비밀번호를 갱신하고, 스키마 `fvoci`와 기본 `search_path`를 설정한다. 여러 번 실행해도 같다.
//!   관리자는 superuser가 아니어도 된다(`CREATEROLE`과 DB의 `CREATE` 권한이면 된다. 셀프호스트 Supabase의
//!   `postgres`, D-88). 그래서 이미 있는 역할의 속성은 바꾸지 않고(superuser만 `SUPERUSER` 속성을 다시 쓸 수
//!   있다) 확인만 해, 권한 속성이 붙어 있으면 실패한다.
//! - migrate: 마이그레이션 역할 연결로 `migrations/`를 적용한다.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection};

use crate::config::{DbCommandConfig, Secret};
use crate::error::AppError;

/// 마이그레이션 역할 이름. 마이그레이션 SQL이 이 이름을 참조한다.
pub const MIGRATOR_ROLE: &str = "fvoci_migrator";
/// 앱 역할 이름. 마이그레이션 SQL의 GRANT 대상.
pub const APP_ROLE: &str = "fvoci_app";
/// 스키마 이름.
pub const SCHEMA: &str = "fvoci";

fn need<'a>(value: Option<&'a Secret>, key: &str) -> Result<&'a Secret, AppError> {
    value.ok_or_else(|| AppError::Config(format!("{key} is required")))
}

/// 역할 DDL은 바인드 매개변수를 받지 않으므로 format()의 %I·%L로 인용한 문장을 DB에서 받아 실행한다.
async fn execute_formatted(
    conn: &mut PgConnection,
    template: &str,
    args: &[&str],
) -> Result<(), AppError> {
    let mut query = sqlx::query_scalar::<_, String>(template);
    for arg in args {
        query = query.bind(*arg);
    }
    let statement = query.fetch_one(&mut *conn).await?;
    sqlx::raw_sql(&statement).execute(&mut *conn).await?;
    Ok(())
}

/// 역할을 만들거나(속성 포함) 비밀번호만 갱신한다. 이미 있는 역할은 바꾸기 전에 권한 속성이 없는지, 관리자가 그
/// 역할을 다룰 수 있는지(ADMIN OPTION) 먼저 확인한다: 멈출 때는 아무것도 바꾸지 않는다.
async fn ensure_role(
    conn: &mut PgConnection,
    role: &str,
    password: &Secret,
    attributes: &str,
    may_create_db: bool,
) -> Result<(), AppError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(role)
            .fetch_one(&mut *conn)
            .await?;
    if exists {
        check_attributes(conn, role, may_create_db).await?;
        check_manageable(conn, role).await?;
        execute_formatted(
            conn,
            "SELECT format('ALTER ROLE %I WITH LOGIN PASSWORD %L', $1, $2)",
            &[role, password.expose()],
        )
        .await?;
    } else {
        execute_formatted(
            conn,
            "SELECT format('CREATE ROLE %I WITH LOGIN PASSWORD %L ' || $3, $1, $2)",
            &[role, password.expose(), attributes],
        )
        .await?;
        check_attributes(conn, role, may_create_db).await?;
    }
    Ok(())
}

/// 다른 관리자가 만든 역할이면 superuser가 아닌 관리자는 ADMIN OPTION이 없어 비밀번호를 바꾸거나 SET 멤버십을 줄
/// 수 없다(PG16부터 CREATEROLE은 자기가 만든 역할에만 ADMIN을 받는다). 일반 DB 오류 대신 해야 할 일을 알린다.
async fn check_manageable(conn: &mut PgConnection, role: &str) -> Result<(), AppError> {
    let manageable: bool = sqlx::query_scalar(
        "SELECT (SELECT rolsuper FROM pg_roles WHERE rolname = current_user)
             OR EXISTS (SELECT 1 FROM pg_auth_members m JOIN pg_roles r ON r.oid = m.roleid
                        WHERE r.rolname = $1 AND m.admin_option
                          AND m.member = (SELECT oid FROM pg_roles WHERE rolname = current_user))",
    )
    .bind(role)
    .fetch_one(&mut *conn)
    .await?;
    if !manageable {
        return Err(AppError::Config(format!(
            "the admin role has no ADMIN OPTION on {role}, which another role created; grant it \
             (GRANT {role} TO <admin> WITH ADMIN OPTION) as that role, or run db bootstrap as that role"
        )));
    }
    Ok(())
}

/// 역할에 권한 속성이 없는지 확인한다(앱 역할은 CREATEDB도 안 된다).
async fn check_attributes(
    conn: &mut PgConnection,
    role: &str,
    may_create_db: bool,
) -> Result<(), AppError> {
    let (superuser, create_role, create_db, bypass_rls): (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT rolsuper, rolcreaterole, rolcreatedb, rolbypassrls FROM pg_roles WHERE rolname = $1",
    )
    .bind(role)
    .fetch_one(&mut *conn)
    .await?;
    if superuser || create_role || bypass_rls || (create_db && !may_create_db) {
        return Err(AppError::Config(format!(
            "role {role} has privileged attributes (SUPERUSER, CREATEROLE, BYPASSRLS or CREATEDB); \
             remove them before running db bootstrap"
        )));
    }
    Ok(())
}

/// 관리자 연결이 마이그레이션 역할(스키마·표 소유자)로 `SET ROLE`할 수 있게 한다. superuser는 이미 할 수 있다.
/// superuser가 아닌 관리자(PG16부터 역할을 만든 사람은 ADMIN만 받는다)에게는 상속 없는 SET 멤버십을 주고
/// `true`를 돌려준다. 그때는 일이 끝나면 [`release_migrator`]로 거둔다.
pub async fn borrow_migrator(conn: &mut PgConnection) -> Result<bool, AppError> {
    let can_set: bool = sqlx::query_scalar("SELECT pg_has_role(current_user, $1, 'SET')")
        .bind(MIGRATOR_ROLE)
        .fetch_one(&mut *conn)
        .await?;
    if can_set {
        return Ok(false);
    }
    execute_formatted(
        conn,
        "SELECT format('GRANT %I TO %I WITH INHERIT FALSE, SET TRUE', $1, current_user)",
        &[MIGRATOR_ROLE],
    )
    .await?;
    Ok(true)
}

/// 관리자 자신의 역할로 돌아가고, [`borrow_migrator`]가 준 멤버십이 있으면 거둔다.
pub async fn release_migrator(conn: &mut PgConnection, granted: bool) -> Result<(), AppError> {
    sqlx::query("RESET ROLE").execute(&mut *conn).await?;
    if granted {
        execute_formatted(
            conn,
            "SELECT format('REVOKE %I FROM %I', $1, current_user)",
            &[MIGRATOR_ROLE],
        )
        .await?;
    }
    Ok(())
}

/// 이 DB에 두 역할의 접속 권한, 마이그레이션 역할 소유의 스키마 `fvoci`, 두 역할의 `search_path`를 둔다.
/// 역할은 이미 있어야 한다. 반복해도 같다.
///
/// 스키마 소유자를 정하려면 관리자가 그 역할로 `SET ROLE`할 수 있어야 하므로 그동안만 [`borrow_migrator`]한다.
pub async fn prepare_database(conn: &mut PgConnection) -> Result<(), AppError> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *conn)
        .await?;
    execute_formatted(
        conn,
        "SELECT format('GRANT CONNECT ON DATABASE %I TO %I, %I', $1, $2, $3)",
        &[&database, MIGRATOR_ROLE, APP_ROLE],
    )
    .await?;
    let granted = borrow_migrator(conn).await?;
    let created = execute_formatted(
        conn,
        "SELECT format('CREATE SCHEMA IF NOT EXISTS %I AUTHORIZATION %I', $1, $2)",
        &[SCHEMA, MIGRATOR_ROLE],
    )
    .await;
    let released = release_migrator(conn, granted).await;
    created?;
    released?;
    let owner: String =
        sqlx::query_scalar("SELECT nspowner::regrole::text FROM pg_namespace WHERE nspname = $1")
            .bind(SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
    if owner != MIGRATOR_ROLE {
        return Err(AppError::Config(format!(
            "schema {SCHEMA} is owned by {owner}, not {MIGRATOR_ROLE}"
        )));
    }
    for role in [MIGRATOR_ROLE, APP_ROLE] {
        execute_formatted(
            conn,
            "SELECT format('ALTER ROLE %I SET search_path = %I', $1, $2)",
            &[role, SCHEMA],
        )
        .await?;
    }
    Ok(())
}

/// 역할·스키마를 준비한다.
pub async fn bootstrap(cfg: &DbCommandConfig) -> Result<(), AppError> {
    let admin = need(cfg.admin_url.as_ref(), "DATABASE_ADMIN_URL")?;
    let migrator_pw = need(cfg.migrator_password.as_ref(), "FVOCI_DB_MIGRATOR_PASSWORD")?;
    let app_pw = need(cfg.app_password.as_ref(), "FVOCI_DB_APP_PASSWORD")?;

    let mut conn = PgConnection::connect_with(&super::connect_options(admin)?).await?;
    ensure_role(
        &mut conn,
        MIGRATOR_ROLE,
        migrator_pw,
        "NOSUPERUSER NOCREATEROLE NOBYPASSRLS",
        true,
    )
    .await?;
    ensure_role(
        &mut conn,
        APP_ROLE,
        app_pw,
        "NOSUPERUSER NOCREATEROLE NOCREATEDB NOBYPASSRLS",
        false,
    )
    .await?;
    prepare_database(&mut conn).await?;
    conn.close().await?;
    Ok(())
}

/// 마이그레이션을 적용한다.
pub async fn migrate(cfg: &DbCommandConfig) -> Result<(), AppError> {
    let url = need(cfg.migration_url.as_ref(), "DATABASE_MIGRATION_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(super::connect_options(url)?)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    pool.close().await;
    Ok(())
}
