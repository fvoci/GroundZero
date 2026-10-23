//! Offline tests: synthetic settings, certificate parsing and a fake connection.
//! Native connections, the process environment and network are never accessed here.

use super::*;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::BTreeMap;

use anyhow::{Context, Result};

const HOST: &str = "aws-0-fake-region-1.pooler.supabase.com";
const USER: &str = "postgres.abcdefghijklmnopqrst";
// Locally generated public test CA. Its private key was discarded at creation;
// this certificate has no relationship to Supabase or any deployed service.
const TEST_CA: &str = "-----BEGIN CERTIFICATE-----
MIIBoDCCAUWgAwIBAgIUad+jX2jS520v1Cx2wXFPvNT6Y6YwCgYIKoZIzj0EAwIw
JTEjMCEGA1UEAwwaR3JvdW5kWmVybyBvZmZsaW5lIHRlc3QgQ0EwHhcNMjYxMDIx
MTE1ODM0WhcNMzYxMDE4MTE1ODM0WjAlMSMwIQYDVQQDDBpHcm91bmRaZXJvIG9m
ZmxpbmUgdGVzdCBDQTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABC1pFjfw6wy2
tPsO/PY1E9gP0xMdY6QBE2cWhqZl9r1EK/X9OrohR9nl41T8GXQ1LJufpsPb9gaa
aBV1eU2+6jyjUzBRMB0GA1UdDgQWBBSDrapwu+71kUjuUOlkCk20R4M38DAfBgNV
HSMEGDAWgBSDrapwu+71kUjuUOlkCk20R4M38DAPBgNVHRMBAf8EBTADAQH/MAoG
CCqGSM49BAMCA0kAMEYCIQC46SlExBFYzajzhtPqUupczLjPKWkZlh5Dd6CDnlHh
XAIhAPRRlb7xvWOtUlGeh8LBRl3rIirruH+uKWl/Jn8QNmsP
-----END CERTIFICATE-----";

fn raw_password() -> Result<String> {
    let value = include_str!("../../../deploy/supabase/test.env")
        .lines()
        .find_map(|line| line.strip_prefix("POSTGRES_PASSWORD="))
        .context("repository test password fixture")?;
    Ok(format!(" {value}%21:@/?# 한글 "))
}

fn settings() -> Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("SUPABASE_TEST_HOST".into(), HOST.into()),
        ("SUPABASE_TEST_PORT".into(), "5432".into()),
        ("SUPABASE_TEST_DATABASE".into(), "postgres".into()),
        ("SUPABASE_TEST_USER".into(), USER.into()),
        ("SUPABASE_TEST_PASSWORD".into(), raw_password()?),
        (SSL_ROOT_CERT_NAME.into(), TEST_CA.into()),
    ]))
}

fn target() -> Result<Target> {
    parse_target(HOST, "5432", "postgres", USER, &raw_password()?, TEST_CA)
        .map_err(|error| anyhow::anyhow!(error.code()))
}

fn assert_private(report: &Value) -> Result<()> {
    let output = report.to_string();
    for sensitive in [HOST, USER, &raw_password()?, TEST_CA, "MIIBoDCCAUWg"] {
        assert!(!output.contains(sensitive));
    }
    assert_eq!(report["connection_kind"], "session_pooler");
    assert_eq!(report["changes"], "NOT RUN");
    Ok(())
}

struct FakeDatabaseError(Option<&'static str>);

impl fmt::Debug for FakeDatabaseError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("driver Debug must not be called");
    }
}

impl fmt::Display for FakeDatabaseError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("driver Display must not be called");
    }
}

impl std::error::Error for FakeDatabaseError {}

impl sqlx::error::DatabaseError for FakeDatabaseError {
    fn message(&self) -> &str {
        panic!("driver message must not be read");
    }

    fn code(&self) -> Option<Cow<'_, str>> {
        self.0.map(Cow::Borrowed)
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }
}

#[test]
fn connection_errors_use_only_types_and_sqlstate_class_without_exposing_details() -> Result<()> {
    let details = format!(
        "{HOST} {USER} {} 28P01 untrusted-certificate-detail DNS",
        raw_password()?
    );
    let mut cases = vec![
        (
            sqlx::Error::Io(io::Error::other(details.clone())),
            "connection_network_io",
        ),
        (sqlx::Error::Tls(details.clone().into()), "connection_tls"),
        (
            sqlx::Error::Protocol(details.clone()),
            "connection_protocol",
        ),
        (
            sqlx::Error::Configuration(details.into()),
            "connection_unclassified",
        ),
    ];
    for code in [
        Some("28000"),
        Some("28P01"),
        Some("3D000"),
        None,
        Some("28"),
        Some("28p01"),
        Some("28P010"),
    ] {
        cases.push((
            sqlx::Error::Database(Box::new(FakeDatabaseError(code))),
            if matches!(code, Some("28000" | "28P01")) {
                "connection_authentication"
            } else {
                "connection_database"
            },
        ));
    }
    for (error, expected) in cases {
        let (report, code) = fail(empty_report(), classify_connection_error(error), 1);
        assert_eq!(code, 1);
        assert_eq!(report["failure_code"], expected);
        assert_private(&report)?;
        for raw in [
            "28000",
            "28P01",
            "3D000",
            "28p01",
            "28P010",
            "untrusted-certificate-detail",
            "DNS",
        ] {
            assert!(!report.to_string().contains(raw));
        }
    }
    Ok(())
}

#[test]
fn absent_connect_missing_settings_or_ambient_overrides_never_start_client() -> Result<()> {
    let lookups = Cell::new(0);
    let calls = Cell::new(0);
    let (report, code) = run_with(
        false,
        |_| {
            lookups.set(lookups.get() + 1);
            None
        },
        |_| {
            calls.set(calls.get() + 1);
            (empty_report(), 0)
        },
    );
    assert_eq!((lookups.get(), calls.get()), (0, 0));
    assert_ne!(code, 0);
    assert_private(&report)?;
    for key in [
        "connection",
        "tls_certificate_and_hostname",
        "query",
        "server_version_query",
    ] {
        assert_eq!(report[key], "NOT RUN");
    }
    let required = settings()?;
    let mut invalid = Vec::new();
    for key in required.keys() {
        let mut values = required.clone();
        values.remove(key);
        invalid.push(values);
    }
    for key in AMBIENT_SETTINGS {
        let mut values = required.clone();
        values.insert((*key).into(), "synthetic-override".into());
        invalid.push(values);
    }
    for values in invalid {
        let (report, code) = run_with(
            true,
            |key| values.get(key).cloned(),
            |_| {
                calls.set(calls.get() + 1);
                (empty_report(), 0)
            },
        );
        assert_ne!(code, 0);
        assert_eq!(calls.get(), 0);
        assert_private(&report)?;
    }
    Ok(())
}

#[test]
fn split_settings_preserve_raw_password_and_reject_non_session_targets() -> Result<()> {
    let target = target()?;
    assert_eq!(target.host, HOST);
    assert_eq!(target.user, USER);
    assert_eq!(target.password, raw_password()?);
    assert_eq!(target.ssl_root_cert, TEST_CA.as_bytes());
    for private in [HOST, USER, &raw_password()?, TEST_CA, "MIIBoDCCAUWg"] {
        assert!(!format!("{target:?}").contains(private));
    }
    let password = raw_password()?;
    for (host, port, database, user, password) in [
        (HOST, "6543", "postgres", USER, password.as_str()),
        (HOST, "5432\n", "postgres", USER, password.as_str()),
        (
            "db.abcdefghijklmnopqrst.supabase.co",
            "5432",
            "postgres",
            USER,
            password.as_str(),
        ),
        (
            "aws-0-fake-region-1.pooler.supabase.com.example.invalid",
            "5432",
            "postgres",
            USER,
            password.as_str(),
        ),
        (
            "aws-0-fake-region-1.pooler.supabase.com?sslmode=disable",
            "5432",
            "postgres",
            USER,
            password.as_str(),
        ),
        (HOST, "5432", "template1", USER, password.as_str()),
        (HOST, "5432", "postgres", "", password.as_str()),
        (HOST, "5432", "postgres", "invalid\nuser", password.as_str()),
        (HOST, "5432", "postgres", USER, ""),
        (HOST, "5432", "postgres", USER, "invalid\0password"),
    ] {
        assert!(parse_target(host, port, database, user, password, TEST_CA).is_err());
    }
    let values = settings()?;
    let calls = Cell::new(0);
    let (_, code) = run_with(
        true,
        |key| values.get(key).cloned(),
        |passed| {
            calls.set(calls.get() + 1);
            assert_eq!(passed.password, password);
            assert_eq!(passed.ssl_root_cert, TEST_CA.as_bytes());
            (empty_report(), 0)
        },
    );
    assert_eq!((calls.get(), code), (1, 0));
    Ok(())
}

#[test]
fn missing_or_invalid_ca_never_starts_client_and_exposes_only_fixed_codes() -> Result<()> {
    let key = "-----BEGIN PRIVATE KEY-----\nsynthetic-test-only\n-----END PRIVATE KEY-----";
    for pem in [
        None,
        Some(String::new()),
        Some(" \r\n ".into()),
        Some("not-a-certificate".into()),
        Some("-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----".into()),
        Some("-----BEGIN CERTIFICATE-----\nYWJj\n-----END CERTIFICATE-----".into()),
        Some(TEST_CA.replace('\n', "\\n")),
        Some(TEST_CA.replace("-----END CERTIFICATE-----", "")),
        Some(format!("{TEST_CA}\n{TEST_CA}")),
        Some(key.into()),
        Some(format!("{key}\n{TEST_CA}")),
        Some(format!("{TEST_CA}\n{key}")),
        Some(format!("{TEST_CA}\nunexpected-trailing-data")),
        Some(format!("{}{TEST_CA}", " ".repeat(64 * 1024))),
    ] {
        let mut values = settings()?;
        values.remove(SSL_ROOT_CERT_NAME);
        if let Some(pem) = &pem {
            values.insert(SSL_ROOT_CERT_NAME.into(), pem.clone());
        }
        let calls = Cell::new(0);
        let (report, code) = run_with(
            true,
            |name| values.get(name).cloned(),
            |_| {
                calls.set(calls.get() + 1);
                (empty_report(), 0)
            },
        );
        assert_eq!((calls.get(), code), (0, 1));
        assert_eq!(
            report["failure_code"],
            if pem.as_ref().is_none_or(|pem| pem.trim().is_empty()) {
                "missing_ssl_root_cert"
            } else {
                "invalid_ssl_root_cert"
            }
        );
        assert_private(&report)?;
        assert!(!report.to_string().contains("synthetic-test-only"));
        for field in ["connection", "tls_certificate_and_hostname", "query"] {
            assert_eq!(report[field], "NOT RUN");
        }
    }
    // Dashboard PEM with either normal line ending and surrounding whitespace.
    for pem in [format!("\n{TEST_CA}\n"), TEST_CA.replace('\n', "\r\n")] {
        assert_eq!(
            parse_ssl_root_cert(&pem),
            Ok(pem.trim().as_bytes().to_vec())
        );
    }
    Ok(())
}

struct FakeConnection {
    events: Vec<&'static str>,
    answer: i32,
    version: i32,
    failure: Option<&'static str>,
    stall: Option<&'static str>,
    password_seen: Option<String>,
}

impl FakeConnection {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            answer: 1,
            version: 170_006,
            failure: None,
            stall: None,
            password_seen: None,
        }
    }

    async fn step(&mut self, name: &'static str) -> std::result::Result<(), Failure> {
        self.events.push(name);
        if self.stall == Some(name) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if self.failure == Some(name) {
            Err(match name {
                "connect" => Failure::ConnectionUnclassified,
                "close" => Failure::ClientError,
                _ => Failure::QueryFailed,
            })
        } else {
            Ok(())
        }
    }
}

impl ConnectionClient for FakeConnection {
    async fn connect(&mut self, target: &Target) -> std::result::Result<(), Failure> {
        self.password_seen = Some(target.password.clone());
        self.step("connect").await
    }
    async fn begin_read_only(&mut self) -> std::result::Result<(), Failure> {
        self.step("begin_read_only").await
    }
    async fn select_one(&mut self) -> std::result::Result<i32, Failure> {
        self.step("select_one").await?;
        Ok(self.answer)
    }
    async fn server_version(&mut self) -> std::result::Result<i32, Failure> {
        self.step("server_version").await?;
        Ok(self.version)
    }
    async fn rollback(&mut self) -> std::result::Result<(), Failure> {
        self.step("rollback").await
    }
    async fn close(&mut self) -> std::result::Result<(), Failure> {
        self.step("close").await
    }
}

fn short_limits() -> Limits {
    Limits {
        connect: Duration::from_millis(20),
        query: Duration::from_millis(20),
        total: Duration::from_millis(80),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn one_connection_reads_probe_and_numeric_version_then_rolls_back() -> Result<()> {
    let mut connection = FakeConnection::new();
    let (report, code) = inspect(&mut connection, &target()?, short_limits()).await;
    assert_eq!(code, 0);
    assert_private(&report)?;
    for key in [
        "connection",
        "tls_certificate_and_hostname",
        "query",
        "server_version_query",
    ] {
        assert_eq!(report[key], "PASS");
    }
    assert_eq!(report["server_version_num"], 170_006);
    assert_eq!(report["postgres_major"], 17);
    assert_eq!(
        connection.events,
        [
            "connect",
            "begin_read_only",
            "select_one",
            "server_version",
            "rollback",
            "close"
        ]
    );
    assert_eq!(connection.password_seen, Some(raw_password()?));
    assert_eq!(SELECT_ONE, "SELECT 1");
    assert_eq!(
        SELECT_SERVER_VERSION,
        "SELECT pg_catalog.current_setting('server_version_num')::integer"
    );
    assert_eq!(BEGIN_READ_ONLY, "BEGIN TRANSACTION READ ONLY");
    // Constructing SQLx options would read ambient defaults. Inspect the native
    // adapter source instead; the fake above verifies the data and lifecycle.
    let source = include_str!("../supabase_cloud_precheck.rs");
    for required in [
        ".ssl_mode(PgSslMode::VerifyFull)",
        ".ssl_root_cert_from_pem(target.ssl_root_cert.clone())",
        ".password(&target.password)",
        ".disable_statement_logging()",
        "NoSubscriber::default()",
    ] {
        assert!(source.contains(required));
    }
    assert_eq!(source.matches("PgConnection::connect_with").count(), 1);
    for forbidden in [
        "PgPool",
        "to_url_lossy",
        "PgSslMode::Require",
        "PgSslMode::Disable",
        "FROM pg_catalog",
    ] {
        assert!(!source.contains(forbidden));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn connection_query_cleanup_failures_and_wrong_answer_are_sanitized() -> Result<()> {
    for stage in [
        "connect",
        "begin_read_only",
        "select_one",
        "server_version",
        "rollback",
        "close",
        "wrong_answer",
        "invalid_version",
    ] {
        let mut connection = FakeConnection::new();
        if stage == "wrong_answer" {
            connection.answer = 2;
        } else if stage == "invalid_version" {
            connection.version = -1;
        } else {
            connection.failure = Some(stage);
        }
        let (report, code) = inspect(&mut connection, &target()?, short_limits()).await;
        assert_ne!(code, 0);
        assert_private(&report)?;
        assert_eq!(
            report["failure_code"],
            match stage {
                "connect" => "connection_unclassified",
                "close" => "client_error",
                "wrong_answer" | "invalid_version" => "unexpected_query_result",
                _ => "query_failed",
            }
        );
        assert_ne!(report["query"], "PASS");
        assert_ne!(report["server_version_query"], "PASS");
        assert!(report.get("server_version_num").is_none());
        assert!(report.get("postgres_major").is_none());
        if matches!(
            stage,
            "connect" | "begin_read_only" | "select_one" | "wrong_answer"
        ) {
            assert!(!connection.events.contains(&"server_version"));
        }
        assert_eq!(
            connection
                .events
                .iter()
                .filter(|event| **event == "connect")
                .count(),
            1
        );
        if stage == "connect" {
            assert_ne!(report["connection"], "PASS");
            assert_ne!(report["tls_certificate_and_hostname"], "PASS");
            assert!(!connection.events.contains(&"select_one"));
        } else {
            assert_eq!(report["connection"], "PASS");
            assert_eq!(report["tls_certificate_and_hostname"], "PASS");
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn version_probe_reports_older_and_newer_servers_without_claiming_support() -> Result<()> {
    for version in [150_014, 170_006, 180_003, 190_001] {
        let mut connection = FakeConnection::new();
        connection.version = version;
        let (report, code) = inspect(&mut connection, &target()?, short_limits()).await;
        assert_eq!(code, 0);
        assert_eq!(report["server_version_num"], version);
        assert_eq!(report["postgres_major"], version / 10_000);
        assert!(report.get("supported").is_none());
        assert_private(&report)?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn deadlines_bound_connection_query_cleanup_and_total_without_retry() -> Result<()> {
    let defaults = Limits::default();
    assert_eq!(defaults.connect, Duration::from_secs(5));
    assert_eq!(defaults.query, Duration::from_secs(5));
    assert_eq!(defaults.total, Duration::from_secs(20));
    for stage in [
        "connect",
        "begin_read_only",
        "select_one",
        "server_version",
        "rollback",
        "close",
    ] {
        let mut connection = FakeConnection::new();
        connection.stall = Some(stage);
        let start = std::time::Instant::now();
        let (report, code) = inspect(&mut connection, &target()?, short_limits()).await;
        assert_ne!(code, 0);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_private(&report)?;
        assert_eq!(
            connection
                .events
                .iter()
                .filter(|event| **event == "connect")
                .count(),
            1
        );
    }
    let mut connection = FakeConnection::new();
    connection.stall = Some("select_one");
    let limits = Limits {
        connect: Duration::from_secs(1),
        query: Duration::from_secs(1),
        total: Duration::from_millis(10),
    };
    let (report, code) = inspect(&mut connection, &target()?, limits).await;
    assert_ne!(code, 0);
    assert_private(&report)?;
    Ok(())
}

#[test]
fn approved_first_install_workflow_builds_before_credentials() -> Result<()> {
    let workflow = include_str!("../../../.github/workflows/supabase-cloud-readonly.yml");
    assert!(workflow.contains("on:\n  workflow_dispatch:"));
    for trigger in [
        "push",
        "pull_request",
        "pull_request_target",
        "workflow_run",
        "schedule",
    ] {
        assert!(
            !workflow
                .lines()
                .any(|line| line.starts_with(&format!("  {trigger}:")))
        );
    }
    assert!(workflow.contains("permissions:\n  contents: read"));
    assert!(workflow.contains("group: supabase-cloud-readonly"));
    assert!(workflow.contains("cancel-in-progress: false"));
    let source = workflow;
    for required in [
        "${{ github.event_name == 'workflow_dispatch'",
        "github.repository == 'fvoci/GroundZero'",
        "github.ref == 'refs/heads/fvoci/sql-consolidation-cloud-once'",
        "github.actor == github.repository_owner",
        "inputs.reviewed_sha == github.sha",
        "ref: ${{ inputs.reviewed_sha }}",
        "persist-credentials: false",
        "permissions:\n  contents: read",
        "group: supabase-cloud-readonly",
        "cancel-in-progress: false",
        "    timeout-minutes: 30",
        "        shell: bash",
        "        timeout-minutes: 20",
        "        timeout-minutes: 9",
    ] {
        assert!(source.contains(required));
    }
    for setting in [
        "HOST",
        "PORT",
        "DATABASE",
        "USER",
        "PASSWORD",
        "SSL_ROOT_CERT",
        "MIGRATOR_PASSWORD",
        "APP_PASSWORD",
    ] {
        assert!(source.contains(&format!("secrets.SUPABASE_TEST_{setting}")));
    }
    for forbidden in [
        "vars.",
        "${{ false",
        "cargo xtask test",
        "DATABASE_ADMIN_URL",
        "TARGET_SHA256",
        "EXPECTED_PROJECT_REF",
        "CONFIRM_DISPOSABLE_TARGET",
    ] {
        assert!(!source.contains(forbidden));
    }
    assert_eq!(source.matches("secrets.").count(), 8);
    let build = source
        .find("cargo build --locked --package xtask")
        .context("build step")?;
    let secret = source.find("secrets.").context("secret step")?;
    let run = source
        .find("target/debug/xtask supabase-cloud-verify --connect")
        .context("prebuilt command")?;
    assert!(build < secret && secret < run);
    assert!(source.contains("actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"));
    assert!(
        !source
            .lines()
            .any(|line| line.trim_end().ends_with(": write"))
    );
    Ok(())
}

#[test]
fn first_install_workflow_publishes_only_sanitized_metadata() -> Result<()> {
    let source = include_str!("../../../.github/workflows/supabase-cloud-readonly.yml")
        .split("  first-install:")
        .nth(1)
        .context("inventory job")?;
    for required in [
        "github.event_name == 'workflow_dispatch'",
        "github.repository == 'fvoci/GroundZero'",
        "github.actor == github.repository_owner",
        "github.ref == 'refs/heads/fvoci/sql-consolidation-cloud-once'",
        "inputs.reviewed_sha == github.sha",
        "ref: ${{ inputs.reviewed_sha }}",
        "persist-credentials: false",
        "target/debug/xtask supabase-cloud-verify --connect",
        "path: target/cloud-install/report.json",
    ] {
        assert!(
            source.contains(required),
            "missing inventory gate: {required}"
        );
    }
    assert_eq!(source.matches("secrets.").count(), 8);
    let build = source
        .find("cargo build --locked --package xtask")
        .context("build step")?;
    let credentials = source.find("secrets.").context("credential step")?;
    assert!(build < credentials);
    for forbidden in [
        "DATABASE_ADMIN_URL",
        "cargo xtask test",
        "supabase-cloud-precheck --connect",
        "env |",
        "printenv",
        "tee ",
        "DROP ",
        "TRUNCATE ",
    ] {
        assert!(!source.contains(forbidden));
    }
    Ok(())
}
