//! 구성 읽기, 상태 생성, 서버 시작과 정상 종료를 연결한다.

use std::sync::Arc;

use tokio::net::TcpListener;

use crate::config::Config;
use crate::db::Db;
use crate::error::AppError;
use crate::features::accounts::OidcProvider;
use crate::http::{self, AppState};

/// 구성으로 공유 상태를 만든다. 테스트도 같은 경로를 쓴다.
pub fn build_state(config: Config) -> Result<AppState, AppError> {
    let db = Db::connect_lazy(&config.database_url)?;
    let oidc = match &config.oidc {
        Some(oidc) => Some(Arc::new(OidcProvider::new(oidc, &config.public_url)?)),
        None => None,
    };
    Ok(AppState {
        db,
        config: Arc::new(config),
        oidc,
    })
}

/// 구성대로 서버를 띄우고 종료 신호까지 실행한다.
pub async fn run(config: Config) -> Result<(), AppError> {
    let bind = config.bind;
    let state = build_state(config)?;
    let db = state.db.clone();
    let app = http::router(state);
    let listener = TcpListener::bind(bind).await?;
    tracing::info!(addr = %bind, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    db.close().await;
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
