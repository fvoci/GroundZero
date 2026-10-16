//! 연결 풀과 트랜잭션 경계. RLS 문맥 설정은 첫 사용자 흐름에서 추가한다.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config::DatabaseUrl;
use crate::error::AppError;

/// 연결 풀을 만든다. 실제 연결은 첫 사용 시점에 지연된다.
pub fn connect_lazy(url: &DatabaseUrl) -> Result<PgPool, AppError> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy(url.as_str())?;
    Ok(pool)
}

/// 왕복 쿼리로 연결 상태를 확인한다.
pub async fn ping(pool: &PgPool) -> Result<(), AppError> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
