//! 구성 읽기, 풀 생성, 서버 시작과 정상 종료를 연결한다.

use tokio::net::TcpListener;

use crate::config::Config;
use crate::error::AppError;
use crate::http::{self, AppState};

/// 구성대로 서버를 띄우고 종료 신호까지 실행한다.
pub async fn run(config: Config) -> Result<(), AppError> {
    let pool = crate::db::connect_lazy(&config.database_url)?;
    let app = http::router(AppState { pool: pool.clone() });
    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    pool.close().await;
    tracing::info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    // ctrl_c 등록 실패는 신호 없이 계속 실행하는 것으로 처리한다. 로그로만 남긴다.
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %err, "failed to listen for shutdown signal");
        std::future::pending::<()>().await;
    }
}
