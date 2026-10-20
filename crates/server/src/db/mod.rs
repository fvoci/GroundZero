//! 연결 풀과 트랜잭션 경계.
//!
//! 요청 처리 코드는 원시 풀을 받지 않는다. 사용자 문맥이 필요한 작업은 [`Db::begin_as`]로
//! 트랜잭션을 열어 `app.user_id`를 설정하고, 그 안에서만 RLS가 적용된 테이블을 읽고 쓴다.
//! 로그인·세션 조회처럼 사용자 문맥 이전의 작업은 [`Db::begin_anonymous`]를 쓴다.

pub mod admin;

use std::time::Duration;

use sqlx::ConnectOptions;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

use crate::config::DatabaseUrl;
use crate::error::AppError;
use crate::ids::UserId;
use uuid::Uuid;

/// 앱 역할 연결 풀.
#[derive(Debug, Clone)]
pub struct Db {
    pool: PgPool,
}

/// 사용자 문맥이 설정된 트랜잭션. `commit`하지 않고 버리면 롤백된다.
#[derive(Debug)]
pub struct UserTx {
    tx: Transaction<'static, Postgres>,
    user: UserId,
}

impl UserTx {
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub fn user(&self) -> UserId {
        self.user
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await?;
        Ok(())
    }
}

/// 사용자 문맥 없는 트랜잭션. 로그인 상태·세션 테이블과 정의자 함수만 다룬다.
#[derive(Debug)]
pub struct AnonTx {
    tx: Transaction<'static, Postgres>,
}

impl AnonTx {
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await?;
        Ok(())
    }
}

impl Db {
    /// 연결 풀을 만든다. 실제 연결은 첫 사용 시점에 지연된다.
    pub fn connect_lazy(url: &DatabaseUrl) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(5))
            .connect_lazy_with(connect_options(url)?);
        Ok(Self { pool })
    }

    /// 왕복 쿼리로 연결 상태를 확인한다.
    pub async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    /// 사용자 문맥 트랜잭션을 연다. `app.user_id`는 트랜잭션 범위(`is_local = true`)로만 설정된다.
    pub async fn begin_as(&self, user: UserId) -> Result<UserTx, AppError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(user.0.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(UserTx { tx, user })
    }

    /// 사용자 문맥 없는 트랜잭션을 연다.
    pub async fn begin_anonymous(&self) -> Result<AnonTx, AppError> {
        Ok(AnonTx {
            tx: self.pool.begin().await?,
        })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// 새 행의 id(UUIDv7) `n`개. DB는 id를 만들지 않는다: 행을 만드는 정의자 함수는 마지막 인자(`p_ids uuid[]`)로
/// 이 목록을 받아 차례로 쓰고, 모자라면 멈춘다(D-88). 한 프로세스에서 만든 값은 만든 순서대로 커진다.
pub fn new_ids(n: usize) -> Vec<Uuid> {
    (0..n).map(|_| Uuid::now_v7()).collect()
}

/// 만드는 행 수가 미리 정해지지 않는 함수의 id 목록. 그 함수의 `<함수>_ids`(같은 인자, 필요한 개수의 상한)를
/// 같은 트랜잭션에서 본 함수 바로 앞에 부른다.
pub async fn needed_ids(
    conn: &mut PgConnection,
    bound: sqlx::query::QueryScalar<'_, Postgres, i32, sqlx::postgres::PgArguments>,
) -> Result<Vec<Uuid>, AppError> {
    let n = bound
        .fetch_one(conn)
        .await
        .map_err(AppError::from_db_function)?;
    Ok(new_ids(usize::try_from(n).unwrap_or(0)))
}

/// 유일 제약 위반(23505)인지.
pub fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "23505")
}

/// 연결 설정. sqlx의 쿼리 문장 로그(느린 쿼리 WARN 포함)는 끈다: SQL 원문을 운영 로그에 남기지 않는다(D-83).
pub fn connect_options(url: &DatabaseUrl) -> Result<PgConnectOptions, AppError> {
    Ok(url
        .expose()
        .parse::<PgConnectOptions>()?
        .disable_statement_logging())
}
