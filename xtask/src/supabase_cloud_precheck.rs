//! Supabase Cloud Session pooler에 한 번 연결해 읽기 전용 SELECT 1만 확인한다.
//! --connect 없이는 환경을 읽지 않으며, 연결 정보와 오류 원문은 출력하지 않는다.

use std::fmt;
use std::io::{self, Write};
use std::time::Duration;

use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::{ConnectOptions, Connection, PgConnection};

const PASSWORD_NAME: &str = "SUPABASE_TEST_PASSWORD";
const BEGIN_READ_ONLY: &str = "BEGIN TRANSACTION READ ONLY";
const SELECT_ONE: &str = "SELECT 1";
// new_without_pgpass still reads ambient defaults. Reject presence without
// printing values or mutating environment; TLS trust also stays with the OS.
const AMBIENT_SETTINGS: &[&str] = &[
    "PGHOSTADDR",
    "PGHOST",
    "PGPORT",
    "PGUSER",
    "PGPASSWORD",
    "PGDATABASE",
    "PGSSLROOTCERT",
    "PGSSLCERT",
    "PGSSLKEY",
    "PGSSLMODE",
    "PGAPPNAME",
    "PGOPTIONS",
    "PGSERVICE",
    "PGSERVICEFILE",
    "PGPASSFILE",
    "PGGSSENCMODE",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "OPENSSL_CONF",
    "OPENSSL_MODULES",
];

/// Carries only fixed codes, never a driver's error or a user-provided string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Failure {
    ExplicitConnectRequired,
    InvalidArguments,
    InvalidTarget,
    AmbientSettingsNotAllowed,
    ConnectionTlsOrQueryFailed,
    UnexpectedQueryResult,
    ClientError,
    Timeout,
}

impl Failure {
    fn code(self) -> &'static str {
        match self {
            Self::ExplicitConnectRequired => "explicit_connect_required",
            Self::InvalidArguments => "invalid_arguments",
            Self::InvalidTarget => "invalid_target",
            Self::AmbientSettingsNotAllowed => "ambient_settings_not_allowed",
            Self::ConnectionTlsOrQueryFailed => "connection_tls_or_query_failed",
            Self::UnexpectedQueryResult => "unexpected_query_result",
            Self::ClientError => "client_error",
            Self::Timeout => "timeout",
        }
    }
}

struct Target {
    host: String,
    user: String,
    password: String,
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Target(<redacted>)")
    }
}

fn parse_target(
    host: &str,
    port: &str,
    database: &str,
    user: &str,
    password: &str,
) -> Result<Target, Failure> {
    let name = host
        .strip_prefix("aws-")
        .and_then(|s| s.strip_suffix(".pooler.supabase.com"));
    let valid_host = name
        .and_then(|s| s.split_once('-'))
        .is_some_and(|(cluster, region)| {
            !cluster.is_empty()
                && cluster.bytes().all(|b| b.is_ascii_digit())
                && region.split('-').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                })
        });
    if !valid_host
        || host.len() > 253
        || port != "5432"
        || database != "postgres"
        || user.is_empty()
        || user.len() > 128
        || user.chars().any(char::is_control)
        || password.is_empty()
        || password.len() > 8192
        || password.chars().any(char::is_control)
    {
        return Err(Failure::InvalidTarget);
    }
    // No URI construction, percent decoding, trimming or project-ref checks.
    Ok(Target {
        host: host.to_owned(),
        user: user.to_owned(),
        password: password.to_owned(),
    })
}

fn connect_options(target: &Target) -> PgConnectOptions {
    PgConnectOptions::new_without_pgpass()
        .host(&target.host)
        .port(5432)
        .database("postgres")
        .username(&target.user)
        .password(&target.password)
        .ssl_mode(PgSslMode::VerifyFull)
        .statement_cache_capacity(0)
        .application_name("fvoci-cloud-readonly-precheck")
        .options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", "5000"),
            ("lock_timeout", "2000"),
            ("idle_in_transaction_session_timeout", "5000"),
        ])
        .disable_statement_logging()
}

fn empty_report() -> Value {
    json!({"connection_kind": "session_pooler", "connection": "NOT RUN",
        "tls_certificate_and_hostname": "NOT RUN", "query": "NOT RUN", "changes": "NOT RUN"})
}

fn fail(mut report: Value, failure: Failure, code: i32) -> (Value, i32) {
    report["failure_code"] = json!(failure.code());
    (report, code)
}

#[derive(Clone, Copy, Debug)]
struct Limits {
    connect: Duration,
    query: Duration,
    total: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            query: Duration::from_secs(5),
            total: Duration::from_secs(20),
        }
    }
}

trait ConnectionClient {
    async fn connect(&mut self, target: &Target) -> Result<(), Failure>;
    async fn begin_read_only(&mut self) -> Result<(), Failure>;
    async fn select_one(&mut self) -> Result<i32, Failure>;
    async fn rollback(&mut self) -> Result<(), Failure>;
    async fn close(&mut self) -> Result<(), Failure>;
}

struct NativeClient(Option<PgConnection>);

impl NativeClient {
    fn connection(&mut self) -> Result<&mut PgConnection, Failure> {
        self.0.as_mut().ok_or(Failure::ClientError)
    }
    async fn statement(&mut self, statement: &str) -> Result<(), Failure> {
        sqlx::raw_sql(statement)
            .execute(self.connection()?)
            .await
            .map_err(|_| Failure::ConnectionTlsOrQueryFailed)?;
        Ok(())
    }
}

impl ConnectionClient for NativeClient {
    async fn connect(&mut self, target: &Target) -> Result<(), Failure> {
        self.0 = Some(
            PgConnection::connect_with(&connect_options(target))
                .await
                .map_err(|_| Failure::ConnectionTlsOrQueryFailed)?,
        );
        Ok(())
    }
    async fn begin_read_only(&mut self) -> Result<(), Failure> {
        self.statement(BEGIN_READ_ONLY).await
    }
    async fn select_one(&mut self) -> Result<i32, Failure> {
        sqlx::query_scalar(SELECT_ONE)
            .fetch_one(self.connection()?)
            .await
            .map_err(|_| Failure::ConnectionTlsOrQueryFailed)
    }
    async fn rollback(&mut self) -> Result<(), Failure> {
        self.statement("ROLLBACK").await
    }
    async fn close(&mut self) -> Result<(), Failure> {
        if let Some(connection) = self.0.take() {
            connection.close().await.map_err(|_| Failure::ClientError)?;
        }
        Ok(())
    }
}

async fn bounded<T>(
    duration: Duration,
    future: impl std::future::Future<Output = Result<T, Failure>>,
) -> Result<T, Failure> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| Failure::Timeout)?
}

async fn inspect_steps(
    client: &mut impl ConnectionClient,
    target: &Target,
    limits: Limits,
    report: &mut Value,
) -> Result<(), Failure> {
    report["connection"] = json!("UNCONFIRMED");
    report["tls_certificate_and_hostname"] = json!("UNCONFIRMED");
    bounded(limits.connect, client.connect(target)).await?;
    report["connection"] = json!("PASS");
    report["tls_certificate_and_hostname"] = json!("PASS");
    report["query"] = json!("FAIL");
    let query = match bounded(limits.query, client.begin_read_only()).await {
        Ok(()) => bounded(limits.query, client.select_one()).await,
        Err(error) => Err(error),
    };
    // Cleanup is bounded even after failure. Preserve the first error; never retry.
    let rollback = bounded(limits.query, client.rollback()).await;
    let close = bounded(limits.query, client.close()).await;
    let value = query?;
    rollback?;
    close?;
    if value != 1 {
        return Err(Failure::UnexpectedQueryResult);
    }
    report["query"] = json!("PASS");
    Ok(())
}

async fn inspect(
    client: &mut impl ConnectionClient,
    target: &Target,
    limits: Limits,
) -> (Value, i32) {
    let mut report = empty_report();
    match bounded(
        limits.total,
        inspect_steps(client, target, limits, &mut report),
    )
    .await
    {
        Ok(()) => (report, 0),
        Err(error) => fail(report, error, 1),
    }
}

fn execute(target: &Target) -> (Value, i32) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return fail(empty_report(), Failure::ClientError, 1);
    };
    // Server notices can contain identities too; suppress all driver diagnostics.
    let result =
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            runtime.block_on(async {
                let mut client = NativeClient(None);
                let result = inspect(&mut client, target, Limits::default()).await;
                drop(client); // Deadline cancellation closes the sole connection's socket.
                result
            })
        });
    // A system DNS resolver may outlive the connect timeout. This one-shot command
    // exits after reporting; do not wait indefinitely in Runtime's destructor.
    runtime.shutdown_background();
    result
}

fn run_with(
    connect: bool,
    mut lookup: impl FnMut(&str) -> Option<String>,
    executor: impl FnOnce(&Target) -> (Value, i32),
) -> (Value, i32) {
    if !connect {
        return fail(empty_report(), Failure::ExplicitConnectRequired, 2);
    }
    if AMBIENT_SETTINGS.iter().any(|name| lookup(name).is_some()) {
        return fail(empty_report(), Failure::AmbientSettingsNotAllowed, 1);
    }
    let target = parse_target(
        &lookup("SUPABASE_TEST_HOST").unwrap_or_default(),
        &lookup("SUPABASE_TEST_PORT").unwrap_or_default(),
        &lookup("SUPABASE_TEST_DATABASE").unwrap_or_default(),
        &lookup("SUPABASE_TEST_USER").unwrap_or_default(),
        &lookup(PASSWORD_NAME).unwrap_or_default(),
    );
    match target {
        Ok(target) => executor(&target),
        Err(error) => fail(empty_report(), error, 1),
    }
}

fn print_report((report, code): (Value, i32)) -> i32 {
    if writeln!(io::stdout().lock(), "{report}").is_err() {
        return 1;
    }
    code
}

pub fn rejected_arguments() -> i32 {
    print_report(fail(empty_report(), Failure::InvalidArguments, 2))
}

pub fn run(connect: bool) -> i32 {
    print_report(run_with(
        connect,
        |name| {
            if AMBIENT_SETTINGS.contains(&name) {
                std::env::var_os(name).is_some().then(String::new) // Presence only.
            } else {
                std::env::var(name).ok()
            }
        },
        execute,
    ))
}

#[cfg(test)]
mod tests;
