//! 실행 진입점.
//!
//! - `fvoci-server` : 서버 실행 (구성은 환경 변수, `docs/development.md`)
//! - `fvoci-server db bootstrap` : 역할·스키마 준비 (`DATABASE_ADMIN_URL`)
//! - `fvoci-server db migrate` : 마이그레이션 적용 (`DATABASE_MIGRATION_URL`)
//! - `fvoci-server backup create <dir>` : DB와 파일 내용 백업 (`DATABASE_MIGRATION_URL`, 서버 구성)
//! - `fvoci-server backup restore <dir>` : 빈 DB·저장소에 복구 (`DATABASE_ADMIN_URL`, 서버 구성)
//! - `fvoci-server --print-openapi` : 계약 JSON을 표준 출력에 쓴다
//! - `fvoci-server version` : 이 바이너리의 제품 버전·소스 SHA를 JSON으로 쓴다(릴리스 이미지 확인용, D-87)
//! - `fvoci-server healthcheck` : 같은 컨테이너의 서버 `/healthz`가 정상인지(컨테이너 상태 확인용, 이미지에 curl 없음)

use std::process::ExitCode;

use fvoci_server::backup;
use fvoci_server::config::{Config, DbCommandConfig};
use fvoci_server::db::admin;
use fvoci_server::error::AppError;
use fvoci_server::http::ApiDoc;
use utoipa::OpenApi;

enum Command {
    Serve,
    PrintOpenapi,
    Version,
    Bootstrap,
    Migrate,
    BackupCreate(String),
    BackupRestore(String),
    Healthcheck,
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
        ["version"] => Ok(Command::Version),
        ["healthcheck"] => Ok(Command::Healthcheck),
        ["db", "bootstrap"] => Ok(Command::Bootstrap),
        ["db", "migrate"] => Ok(Command::Migrate),
        ["backup", "create", dir] => Ok(Command::BackupCreate((*dir).to_owned())),
        ["backup", "restore", dir] => Ok(Command::BackupRestore((*dir).to_owned())),
        other => Err(format!("unknown arguments: {other:?}")),
    }
}

async fn run(command: Command) -> Result<(), AppError> {
    match command {
        Command::Serve => fvoci_server::app::run(Config::from_env()?).await,
        Command::Bootstrap => admin::bootstrap(&DbCommandConfig::from_env()).await,
        Command::Migrate => admin::migrate(&DbCommandConfig::from_env()).await,
        Command::BackupCreate(dir) => {
            let manifest = backup::create(
                &Config::from_env()?,
                &DbCommandConfig::from_env(),
                dir.as_ref(),
            )
            .await?;
            println!(
                "backup written to {dir}: {} tables, {} files, migration {}",
                manifest.tables.len(),
                manifest.objects.len(),
                manifest.migration_version
            );
            Ok(())
        }
        Command::BackupRestore(dir) => {
            let manifest = backup::restore(
                &Config::from_env()?,
                &DbCommandConfig::from_env(),
                dir.as_ref(),
            )
            .await?;
            println!(
                "restored from {dir}: {} tables, {} files; everyone signs in again",
                manifest.tables.len(),
                manifest.objects.len()
            );
            Ok(())
        }
        Command::PrintOpenapi | Command::Version => Ok(()),
        Command::Healthcheck => healthcheck().await,
    }
}

fn main() -> ExitCode {
    let command = match parse_args() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!(
                "{msg}\nusage: fvoci-server [--print-openapi | version | healthcheck | db bootstrap | db migrate | backup create <dir> | backup restore <dir>]"
            );
            return ExitCode::FAILURE;
        }
    };

    if matches!(command, Command::Version) {
        return match serde_json::to_string(&fvoci_server::build_info::CURRENT) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("failed to serialize build info: {err}");
                ExitCode::FAILURE
            }
        };
    }

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

    match fvoci_server::logging::settings(|key| std::env::var(key).ok()) {
        Ok((filter, format)) => fvoci_server::logging::init(filter, format),
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    }

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
            tracing::error!(
                event = "server.exit_error",
                error_code = err.code(),
                detail = fvoci_server::logging::safe_error(&err),
                "exited with error"
            );
            ExitCode::FAILURE
        }
    }
}

/// 서버의 `/healthz`를 묻는다(바인드 주소의 포트로, 같은 호스트에서). 정상이 아니면 실패로 끝난다.
async fn healthcheck() -> Result<(), AppError> {
    let bind = std::env::var("FVOCI_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Config(format!("healthcheck client: {e}")))?
        .get(format!("http://127.0.0.1:{port}/healthz"))
        .send()
        .await
        .map_err(|e| AppError::Config(format!("healthcheck: {e}")))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(AppError::Config(format!(
            "healthcheck: {}",
            response.status()
        )))
    }
}
