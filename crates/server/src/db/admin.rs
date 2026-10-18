//! `db bootstrap`과 `db migrate`. 운영자·Compose·CI가 같은 경로로 역할과 스키마를 준비한다.
//!
//! - bootstrap: 관리자 연결로 `fvoci_migrator`(스키마 소유)와 `fvoci_app`(`NOBYPASSRLS`)을 만들거나
//!   비밀번호를 갱신하고, 스키마 `fvoci`와 기본 `search_path`를 설정한다. 여러 번 실행해도 같다.
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

async fn ensure_role(
    conn: &mut PgConnection,
    role: &str,
    password: &Secret,
    attributes: &str,
) -> Result<(), AppError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(role)
            .fetch_one(&mut *conn)
            .await?;
    let verb = if exists { "ALTER" } else { "CREATE" };
    // 역할 DDL은 바인드 매개변수를 받지 않으므로 format()의 %I·%L로 인용한다.
    let statement: String =
        sqlx::query_scalar("SELECT format('%s ROLE %I WITH LOGIN PASSWORD %L ' || $4, $1, $2, $3)")
            .bind(verb)
            .bind(role)
            .bind(password.expose())
            .bind(attributes)
            .fetch_one(&mut *conn)
            .await?;
    sqlx::raw_sql(&statement).execute(&mut *conn).await?;
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
    )
    .await?;
    ensure_role(
        &mut conn,
        APP_ROLE,
        app_pw,
        "NOSUPERUSER NOCREATEROLE NOCREATEDB NOBYPASSRLS",
    )
    .await?;

    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut conn)
        .await?;
    let statements: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(ARRAY[
            format('GRANT CONNECT ON DATABASE %I TO %I, %I', $1, $2, $3),
            format('CREATE SCHEMA IF NOT EXISTS %I AUTHORIZATION %I', $4, $2),
            format('ALTER ROLE %I SET search_path = %I', $2, $4),
            format('ALTER ROLE %I SET search_path = %I', $3, $4)
        ])",
    )
    .bind(&database)
    .bind(MIGRATOR_ROLE)
    .bind(APP_ROLE)
    .bind(SCHEMA)
    .fetch_all(&mut conn)
    .await?;
    for statement in statements {
        sqlx::raw_sql(&statement).execute(&mut conn).await?;
    }
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
