//! 실행 진입점.
//!
//! - `fvoci-server` : 서버 실행 (구성은 환경 변수, `fvoci-server --help`·`deploy/selfhost.env.example`)
//! - `fvoci-server --help` / `fvoci-server db --help` : 설치 명령·설정 안내 (DB 연결 없음)
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

const USAGE: &str = "usage: fvoci-server [--help | --print-openapi | version | healthcheck | db bootstrap | db migrate | backup create <dir> | backup restore <dir>]";

const HELP: &str = "\
With no arguments, start the server using its environment settings.

Database setup (run in order; continue only when the previous command succeeds):
  db bootstrap          Prepare the database using DATABASE_ADMIN_URL,
                        FVOCI_DB_MIGRATOR_PASSWORD and FVOCI_DB_APP_PASSWORD.
  db migrate            Apply migrations using DATABASE_MIGRATION_URL.
  (no arguments)        Start the server using DATABASE_URL and app settings.

The standard self-host Compose setup runs these steps automatically.
The binary reads process environment variables; it does not load .env files.
RLS setup is handled by the application and migrations.

Other commands:
  backup create <dir>    Back up the database and files.
  backup restore <dir>   Restore into an empty installation of the same release.
  version               Print product version and source revision as JSON.
  healthcheck           Check this container's running server.
  --print-openapi       Print the API contract as JSON.

Installation and migration troubleshooting: docs/self-hosting.md#db-setup
Self-hosted Supabase database: docs/self-hosting-supabase.md";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Serve,
    Help,
    PrintOpenapi,
    Version,
    Bootstrap,
    Migrate,
    BackupCreate(String),
    BackupRestore(String),
    Healthcheck,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Ok(Command::Serve),
        ["--help" | "-h" | "help"] | ["db", "--help" | "-h" | "help"] => Ok(Command::Help),
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

/// 기존 오류 분류에 설치자가 따라갈 다음 단계만 더한다. DB 작업이나 자동 복구는 하지 않는다.
fn db_failure_hint(command: &Command, error: &AppError) -> Option<&'static str> {
    match command {
        Command::Bootstrap => Some(
            "Check DATABASE_ADMIN_URL, FVOCI_DB_MIGRATOR_PASSWORD and FVOCI_DB_APP_PASSWORD \
             in the command environment and the reported detail. See docs/self-hosting.md#db-setup.",
        ),
        Command::Migrate => Some(match error {
            AppError::Migrate(
                sqlx::migrate::MigrateError::VersionMismatch(_)
                | sqlx::migrate::MigrateError::VersionMissing(_)
                | sqlx::migrate::MigrateError::Dirty(_),
            ) => {
                "Stop the upgrade and preserve the database, volumes and backups. Check the release \
                 and migration history before retrying; see docs/self-hosting.md#db-setup."
            }
            AppError::Migrate(sqlx::migrate::MigrateError::ExecuteMigration(_, _)) => {
                "Stop the upgrade and preserve the database and volumes. Report the migration version, \
                 error code and release to the maintainer; see docs/self-hosting.md#db-setup."
            }
            _ => {
                "Check DATABASE_MIGRATION_URL in the command environment, database availability and \
                 the bootstrap result. See docs/self-hosting.md#db-setup before retrying."
            }
        }),
        _ => None,
    }
}

async fn run(command: &Command) -> Result<(), AppError> {
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
        Command::Help | Command::PrintOpenapi | Command::Version => Ok(()),
        Command::Healthcheck => healthcheck().await,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse_args(&args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}\n{USAGE}\nRun fvoci-server --help for setup instructions.");
            return ExitCode::FAILURE;
        }
    };

    if matches!(command, Command::Help) {
        println!("{USAGE}\n\n{HELP}");
        return ExitCode::SUCCESS;
    }

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

    match runtime.block_on(run(&command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(
                event = "server.exit_error",
                error_code = err.code(),
                detail = fvoci_server::logging::safe_error(&err),
                hint = db_failure_hint(&command, &err),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_is_a_separate_command_from_database_work() {
        for args in [
            vec!["--help"],
            vec!["-h"],
            vec!["help"],
            vec!["db", "--help"],
            vec!["db", "-h"],
            vec!["db", "help"],
        ] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            assert_eq!(parse_args(&args), Ok(Command::Help));
        }
        assert!(parse_args(&["db".into()]).is_err());
        assert!(parse_args(&["db".into(), "migrate".into(), "extra".into()]).is_err());
    }

    #[test]
    fn migration_history_errors_require_preservation_before_retry() {
        use sqlx::migrate::MigrateError;

        for error in [
            MigrateError::VersionMismatch(1),
            MigrateError::VersionMissing(1),
            MigrateError::Dirty(1),
        ] {
            let hint = db_failure_hint(&Command::Migrate, &AppError::Migrate(error));
            assert!(hint.is_some_and(|hint| hint.contains("Stop the upgrade and preserve")));
        }
        let error = AppError::Config("DATABASE_MIGRATION_URL is required".into());
        assert!(
            db_failure_hint(&Command::Migrate, &error)
                .is_some_and(|hint| hint.contains("DATABASE_MIGRATION_URL"))
        );
        let error = AppError::Migrate(MigrateError::ExecuteMigration(sqlx::Error::RowNotFound, 1));
        assert!(
            db_failure_hint(&Command::Migrate, &error)
                .is_some_and(|hint| hint.contains("Report the migration version"))
        );
        assert_eq!(db_failure_hint(&Command::Serve, &error), None);
    }
}
