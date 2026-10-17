//! 구성 읽기, 상태 생성, 서버 시작과 정상 종료를 연결한다.

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::Config;
use crate::db::Db;
use crate::error::AppError;
use crate::features::accounts::OidcProvider;
use crate::http::{self, AppState};
use crate::search::SearchClient;
use crate::worker;

/// 구성으로 공유 상태를 만든다. 테스트도 같은 경로를 쓴다.
pub fn build_state(config: Config) -> Result<AppState, AppError> {
    let db = Db::connect_lazy(&config.database_url)?;
    let oidc = match &config.oidc {
        Some(oidc) => Some(Arc::new(OidcProvider::new(oidc, &config.public_url)?)),
        None => None,
    };
    let search = config.search.as_ref().map(SearchClient::new);
    let storage = crate::storage::Storage::new(&config.storage);
    let upload_permits = Arc::new(tokio::sync::Semaphore::new(config.files.upload_concurrency));
    let export_permits = Arc::new(tokio::sync::Semaphore::new(config.files.export_concurrency));
    Ok(AppState {
        db,
        config: Arc::new(config),
        oidc,
        search,
        storage,
        upload_permits,
        export_permits,
        collab: crate::collab::Rooms::default(),
    })
}

/// 구성대로 서버(와 실행 역할에 따라 백그라운드 작업)를 띄우고 종료 신호까지 실행한다.
pub async fn run(config: Config) -> Result<(), AppError> {
    let bind = config.bind;
    let role = config.role;
    let interval = config.worker_interval;
    let state = build_state(config)?;
    let db = state.db.clone();
    let (stop, stopped) = watch::channel(false);
    let worker = role.runs_worker().then(|| {
        worker::spawn(
            db.clone(),
            state.search.clone(),
            state.storage.clone(),
            interval,
            stopped,
        )
    });
    let rooms = state.collab.clone();
    if role.serves_http() {
        let app = http::router(state);
        let listener = TcpListener::bind(bind).await?;
        tracing::info!(event = "server.listening", addr = %bind, ?role, "listening");
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        // 실시간 편집 방은 리비전을 남기고 닫는다(클라이언트는 다시 붙는다, D-70).
        rooms.shutdown().await;
    } else {
        tracing::info!(
            event = "server.worker_only",
            ?role,
            "background worker only"
        );
        shutdown_signal().await;
    }
    // 받는 쪽(작업)이 이미 끝났으면 보낼 곳이 없을 뿐이다.
    if stop.send(true).is_err() {
        tracing::debug!(
            event = "server.worker_already_stopped",
            "background worker already stopped"
        );
    }
    if let Some(handle) = worker
        && let Err(err) = handle.await
    {
        tracing::error!(
            event = "server.worker_stopped",
            panicked = err.is_panic(),
            "background worker stopped abnormally"
        );
    }
    db.close().await;
    tracing::info!(event = "server.shutdown", "shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    // ctrl_c 등록 실패는 신호 없이 계속 실행하는 것으로 처리한다. 로그로만 남긴다.
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!(event = "server.signal_failed", detail = ?err.kind(), "failed to listen for shutdown signal");
        std::future::pending::<()>().await;
    }
}
