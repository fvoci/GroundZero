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

/// 새 행의 id(UUIDv7) `n`개. `p_ids uuid[]`를 받는 SQL 함수는 마지막 인자로 이 목록을 받아 차례로 쓴다(D-88).
/// 개수는 고정된 상한 또는 입력 행 수로 정하고, DB 상태에 따라 달라지면 [`needed_ids`]를 쓴다.
/// DB는 모자란 id를 만들지 않는다(`FV001`). 남는 id는 버리며, 이 목록은 재시도용 `request_id`와 별개다.
/// 한 프로세스에서 만든 값은 만든 순서대로 커진다.
pub fn new_ids(n: usize) -> Vec<Uuid> {
    (0..n).map(|_| Uuid::now_v7()).collect()
}

/// 만드는 행 수가 DB 상태에 따라 달라지는 함수의 id 목록.
///
/// `bound`는 `<함수>_ids`에 개수 산정에 필요한 인자를 바인딩한 쿼리다(본 함수의 모든 인자와 같지는 않다).
/// 본 함수와 같은 트랜잭션에서 바로 앞에 실행하고, 돌려받은 목록을 본 함수의 마지막 `p_ids`에 바인딩한다.
/// NULL·음수 상한은 빈 목록으로 처리하되 본 함수 호출은 생략하지 않는다(기존 404 등의 오류는 본 함수가 낸다).
/// 상한 조회의 DB 오류는 `AppError::from_db_function`으로 전달한다. 재시도·커밋은 이 함수가 맡지 않는다.
pub async fn needed_ids(
    conn: &mut PgConnection,
    bound: sqlx::query::QueryScalar<'_, Postgres, Option<i32>, sqlx::postgres::PgArguments>,
) -> Result<Vec<Uuid>, AppError> {
    let n = bound
        .fetch_one(conn)
        .await
        .map_err(AppError::from_db_function)?
        .unwrap_or(0);
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
