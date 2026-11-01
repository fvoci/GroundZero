use anyhow::{Result, ensure};
use sqlx::ConnectOptions;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use super::{Phase, allowed::Binding};

pub(super) const AMBIENT_SETTINGS: &[&str] = &[
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

pub(super) struct Target {
    pub admin: PgConnectOptions,
    pub migrator: PgConnectOptions,
    pub app: PgConnectOptions,
    pub binding: Binding,
    pub migrator_password: String,
    pub app_password: String,
    pub certificate: Option<Certificate>,
}

pub(super) struct Certificate(PathBuf);

impl Certificate {
    fn create(pem: &str, directory: &str) -> Result<Self> {
        let directory = std::path::Path::new(directory);
        ensure!(
            directory.is_absolute() && directory.is_dir(),
            "invalid_runner_temp"
        );
        let cert = Self(directory.join(format!("fvoci-cloud-{}.pem", uuid::Uuid::now_v7())));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&cert.0)?;
        file.write_all(pem.as_bytes())?;
        Ok(cert)
    }

    pub fn remove(&mut self) -> Result<()> {
        if self.0.exists() {
            std::fs::remove_file(&self.0)?;
        }
        Ok(())
    }
}

impl Drop for Certificate {
    fn drop(&mut self) {
        if self.remove().is_err() {
            eprintln!("certificate_cleanup_failed");
        }
    }
}

fn required(lookup: &mut impl FnMut(&str) -> Option<String>, name: &str) -> Result<String> {
    let value = lookup(name).unwrap_or_default();
    ensure!(
        !value.is_empty() && value.len() <= 65536,
        "missing_or_invalid_secret"
    );
    Ok(value)
}

fn account_names(phase: Phase) -> Option<(&'static str, &'static str)> {
    match phase {
        Phase::Inspect => Some(("SUPABASE_INSPECT_USER", "SUPABASE_INSPECT_PASSWORD")),
        Phase::Rebuild => Some((
            "SUPABASE_REBUILD_ADMIN_USER",
            "SUPABASE_REBUILD_ADMIN_PASSWORD",
        )),
        Phase::Verify => None,
    }
}

pub(super) fn actions_guard(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<String> {
    actions_guard_at(lookup, option_env!("FVOCI_CLOUD_BUILD_SHA"))
}

fn actions_guard_at(
    lookup: &mut impl FnMut(&str) -> Option<String>,
    build_sha: Option<&str>,
) -> Result<String> {
    for (key, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_EVENT_NAME", "workflow_dispatch"),
        ("GITHUB_REPOSITORY", "fvoci/GroundZero"),
        ("GITHUB_REF", "refs/heads/main"),
    ] {
        ensure!(
            lookup(key).as_deref() == Some(expected),
            "actions_main_required"
        );
    }
    let sha = lookup("GITHUB_SHA").unwrap_or_default();
    ensure!(
        sha.len() == 40
            && sha.bytes().all(|b| b.is_ascii_hexdigit())
            && build_sha == Some(sha.as_str()),
        "build_sha_mismatch"
    );
    Ok(sha)
}

impl Target {
    pub fn url(&self, options: &PgConnectOptions) -> String {
        let mut url = options.to_url_lossy();
        // SQLx 0.8's lossy URL formats CertificateInput::File with a "file: " prefix.
        // Replace that diagnostic representation with the actual path for product parsing.
        let pairs: Vec<_> = url
            .query_pairs()
            .filter(|(k, _)| k != "sslrootcert")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut().extend_pairs(pairs);
        if let Some(cert) = &self.certificate {
            url.query_pairs_mut()
                .append_pair("sslrootcert", &cert.0.to_string_lossy());
        }
        url.query_pairs_mut().append_pair("options", "-c statement_timeout=15000 -c lock_timeout=3000 -c idle_in_transaction_session_timeout=30000");
        url.to_string()
    }

    pub fn load(
        phase: Phase,
        sha: &str,
        lookup: &mut impl FnMut(&str) -> Option<String>,
    ) -> Result<Self> {
        // libpq defaults and TLS process configuration must never redirect a validated target.
        for name in AMBIENT_SETTINGS {
            ensure!(lookup(name).is_none(), "ambient_settings_not_allowed");
        }
        let host = required(lookup, "SUPABASE_REBUILD_HOST")?;
        let project = host
            .strip_prefix("db.")
            .and_then(|h| h.strip_suffix(".supabase.co"));
        ensure!(
            project.is_some_and(|p| p.len() == 20
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())),
            "direct_cloud_endpoint_required"
        );
        let (admin_user, password) = match account_names(phase) {
            None => ("postgres".to_owned(), String::new()),
            Some((user, password)) => (required(lookup, user)?, required(lookup, password)?),
        };
        ensure!(
            admin_user.len() <= 63
                && admin_user
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && admin_user != "fvoci_app"
                && admin_user != "fvoci_migrator",
            "invalid_admin_role"
        );
        let cert = required(lookup, "SUPABASE_REBUILD_SSL_ROOT_CERT")?;
        let pem = cert.trim();
        ensure!(
            pem.starts_with("-----BEGIN CERTIFICATE-----")
                && pem.ends_with("-----END CERTIFICATE-----")
                && pem.matches("-----BEGIN").count() == 1,
            "invalid_root_certificate"
        );
        native_tls::Certificate::from_pem(pem.as_bytes())?;
        // Product SQLx 0.8 accepts sslrootcert as a path. Keep it only in the ephemeral runner temp,
        // with 0600 permissions; never put it in the checkout, cache, artifact or command arguments.
        let certificate = Certificate::create(pem, &required(lookup, "RUNNER_TEMP")?)?;
        let admin = PgConnectOptions::new_without_pgpass()
            .host(&host)
            .port(5432)
            .database("postgres")
            .username(&admin_user)
            .password(&password)
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert(&certificate.0)
            .statement_cache_capacity(0)
            .application_name("fvoci-cloud-rebuild")
            .options([
                ("statement_timeout", "15000"),
                ("lock_timeout", "3000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ])
            .disable_statement_logging();
        let (migrator_password, app_password) = match phase {
            Phase::Inspect => (String::new(), String::new()),
            Phase::Rebuild | Phase::Verify => (
                required(lookup, "SUPABASE_REBUILD_MIGRATOR_PASSWORD")?,
                required(lookup, "SUPABASE_REBUILD_APP_PASSWORD")?,
            ),
        };
        Ok(Self {
            migrator: admin
                .clone()
                .username("fvoci_migrator")
                .password(&migrator_password),
            app: admin.clone().username("fvoci_app").password(&app_password),
            binding: Binding::product(super::fingerprint(&serde_json::json!([
                host, "postgres", sha
            ])))?,
            admin,
            migrator_password,
            app_password,
            certificate: Some(certificate),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn inspection_never_selects_the_write_admin_credentials() {
        assert_eq!(
            account_names(Phase::Inspect),
            Some(("SUPABASE_INSPECT_USER", "SUPABASE_INSPECT_PASSWORD"))
        );
    }

    #[test]
    fn runner_certificate_is_private_temporary_and_url_is_parseable() -> Result<()> {
        let mut cert = Certificate::create(
            "public synthetic CA",
            &std::env::temp_dir().to_string_lossy(),
        )?;
        let path = cert.0.clone();
        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o600
        );
        let options = PgConnectOptions::new_without_pgpass()
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert(&path);
        let target = Target {
            admin: options.clone(),
            migrator: options.clone(),
            app: options.clone(),
            binding: Binding::fixture(),
            migrator_password: String::new(),
            app_password: String::new(),
            certificate: Some(cert),
        };
        let url = target.url(&options);
        let parsed: PgConnectOptions = url.parse()?;
        assert!(matches!(parsed.get_ssl_mode(), PgSslMode::VerifyFull));
        let pairs: Vec<_> = parsed
            .to_url_lossy()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| k == "sslrootcert")
                .map(|(_, v)| v.as_str()),
            Some(format!("file: {}", path.display()).as_str())
        );
        cert = target.certificate.context("missing_certificate")?;
        cert.remove()?;
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn actions_guard_does_not_read_secrets_before_rejecting() {
        let mut keys = Vec::new();
        assert!(
            actions_guard(&mut |key| {
                keys.push(key.to_owned());
                None
            })
            .is_err()
        );
        assert_eq!(keys, ["GITHUB_ACTIONS"]);
    }

    #[test]
    fn reviewed_main_at_the_built_sha_is_the_only_accepted_context() {
        let sha = "a".repeat(40);
        let context = [
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_EVENT_NAME", "workflow_dispatch"),
            ("GITHUB_REPOSITORY", "fvoci/GroundZero"),
            ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_SHA", sha.as_str()),
        ];
        let mut lookup = |key: &str| {
            context
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        };
        assert!(actions_guard_at(&mut lookup, Some(&sha)).is_ok());
        assert!(actions_guard_at(&mut lookup, None).is_err());
        assert!(actions_guard_at(&mut lookup, Some(&"b".repeat(40))).is_err());
        for (key, value) in [
            ("GITHUB_ACTIONS", "false"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_REPOSITORY", "other/repo"),
            ("GITHUB_REF", "refs/heads/feature"),
        ] {
            assert!(
                actions_guard_at(
                    &mut |candidate| {
                        if candidate == key {
                            Some(value.to_owned())
                        } else {
                            lookup(candidate)
                        }
                    },
                    Some(&sha)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn rejects_poolers_before_reading_credentials() {
        for host in [
            "aws-0-ap-northeast-2.pooler.supabase.com",
            "localhost",
            "db.evil.supabase.co.attacker.test",
        ] {
            assert!(
                Target::load(Phase::Inspect, "sha", &mut |key| {
                    assert!(!(key.starts_with("SUPABASE_") && key.ends_with("PASSWORD")));
                    (key == "SUPABASE_REBUILD_HOST").then(|| host.to_owned())
                })
                .is_err()
            );
        }
    }
}
