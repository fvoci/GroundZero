//! 실행 진입점.
//!
//! - `fvoci-server` : 서버 실행 (구성은 환경 변수, `docs/development.md`)
//! - `fvoci-server db bootstrap` : 역할·스키마 준비 (`DATABASE_ADMIN_URL`)
//! - `fvoci-server db migrate` : 마이그레이션 적용 (`DATABASE_MIGRATION_URL`)
//! - `fvoci-server --print-openapi` : 계약 JSON을 표준 출력에 쓴다

use std::process::ExitCode;

use fvoci_server::config::{Config, DbCommandConfig};
use fvoci_server::db::admin;
use fvoci_server::error::AppError;
use fvoci_server::http::ApiDoc;
use utoipa::OpenApi;

enum Command {
    Serve,
    PrintOpenapi,
    Bootstrap,
    Migrate,
}

fn parse_args() -> Result<Command, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Ok(Command::Serve),
        ["--print-openapi"] => Ok(Command::PrintOpenapi),
        ["db", "bootstrap"] => Ok(Command::Bootstrap),
        ["db", "migrate"] => Ok(Command::Migrate),
        other => Err(format!("unknown arguments: {other:?}")),
    }
}

async fn run(command: Command) -> Result<(), AppError> {
    match command {
        Command::Serve => fvoci_server::app::run(Config::from_env()?).await,
        Command::Bootstrap => admin::bootstrap(&DbCommandConfig::from_env()).await,
        Command::Migrate => admin::migrate(&DbCommandConfig::from_env()).await,
        Command::PrintOpenapi => Ok(()),
    }
}

fn main() -> ExitCode {
    let command = match parse_args() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}\nusage: fvoci-server [--print-openapi | db bootstrap | db migrate]");
            return ExitCode::FAILURE;
        }
    };

    if matches!(command, Command::PrintOpenapi) {
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

    match runtime.block_on(run(command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = %err, "exited with error");
            ExitCode::FAILURE
        }
    }
}
