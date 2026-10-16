//! 실행 진입점. `--print-openapi`는 계약 JSON을 표준 출력에 쓰고 끝난다.

use std::process::ExitCode;

use fvoci_server::config::Config;
use fvoci_server::http::ApiDoc;
use utoipa::OpenApi;

fn main() -> ExitCode {
    if std::env::args().any(|a| a == "--print-openapi") {
        return match ApiDoc::openapi().to_pretty_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("failed to serialize openapi: {err}");
                ExitCode::FAILURE
            }
        };
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("failed to start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    let result = runtime.block_on(async {
        let config = Config::from_env()?;
        fvoci_server::app::run(config).await
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = %err, "server exited with error");
            ExitCode::FAILURE
        }
    }
}
