//! Exercise the default HTTP client's OS trust store in separate processes.
//! OpenSSL generates one-day certificates in a private temporary directory.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!("fvoci-private-ca-{}", uuid::Uuid::now_v7()));
        #[cfg(unix)]
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        #[cfg(not(unix))]
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("removing temporary TLS fixtures: {error}");
        }
    }
}

fn openssl(directory: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("openssl")
        .current_dir(directory)
        .args(args)
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("OpenSSL CLI is required for private CA tests: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "OpenSSL fixture generation failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

fn generate_fixtures() -> Result<TempDir, Box<dyn Error>> {
    let directory = TempDir::new()?;
    for (name, subject) in [
        ("ca", "/CN=fvoci private CA test only"),
        ("unrelated-ca", "/CN=fvoci unrelated CA test only"),
    ] {
        openssl(
            &directory.0,
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-noenc",
                "-sha256",
                "-days",
                "1",
                "-subj",
                subject,
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-addext",
                "keyUsage=critical,keyCertSign,cRLSign",
                "-keyout",
                &format!("{name}.key"),
                "-out",
                &format!("{name}.pem"),
            ],
        )?;
    }
    openssl(
        &directory.0,
        &[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-noenc",
            "-sha256",
            "-subj",
            "/CN=fvoci TLS server test only",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-addext",
            "keyUsage=critical,digitalSignature,keyEncipherment",
            "-addext",
            "extendedKeyUsage=serverAuth",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ],
    )?;
    openssl(
        &directory.0,
        &[
            "x509",
            "-req",
            "-sha256",
            "-days",
            "1",
            "-copy_extensions",
            "copy",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-CAserial",
            "ca.srl",
            "-out",
            "server.pem",
        ],
    )?;
    let mut bundle = fs::read(directory.0.join("unrelated-ca.pem"))?;
    bundle.extend_from_slice(&fs::read(directory.0.join("ca.pem"))?);
    fs::write(directory.0.join("combined-ca.pem"), bundle)?;
    Ok(directory)
}

struct TlsServer(Child);

impl Drop for TlsServer {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            eprintln!("stopping test TLS server: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("reaping test TLS server: {error}");
        }
    }
}

async fn probe(url: &str) -> Result<(), Box<dyn Error>> {
    let trusted = std::env::var("FVOCI_TLS_TEST_TRUST")? == "1";
    let result = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?
        .get(url)
        .send()
        .await;
    if trusted {
        let response = result?;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert!(response.text().await?.contains("s_server"));
    } else {
        assert!(
            result.is_err(),
            "a private CA must not be trusted by default"
        );
        if let Err(error) = result {
            let mut messages = error.to_string();
            let mut source = error.source();
            while let Some(cause) = source {
                write!(messages, ": {cause}")?;
                source = cause.source();
            }
            assert!(
                messages.to_ascii_lowercase().contains("certificate"),
                "expected certificate rejection, got {messages}"
            );
        }
    }
    Ok(())
}

async fn verify_ca_files(
    test_name: &str,
    cases: &[(Option<&str>, bool)],
) -> Result<(), Box<dyn Error>> {
    // Process-local environment avoids changing trust for concurrent tests.
    if let Ok(url) = std::env::var("FVOCI_TLS_TEST_URL") {
        return probe(&url).await;
    }
    let directory = generate_fixtures()?;
    let fixtures = &directory.0;
    let address = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let mut server = TlsServer(
        Command::new("openssl")
            .args(["s_server", "-quiet", "-www", "-accept"])
            .arg(address.to_string())
            .arg("-cert")
            .arg(fixtures.join("server.pem"))
            .arg("-key")
            .arg(fixtures.join("server.key"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(address).is_err() {
        assert!(server.0.try_wait()?.is_none(), "test TLS server exited");
        assert!(Instant::now() < deadline, "test TLS server did not listen");
        std::thread::sleep(Duration::from_millis(25));
    }
    for &(certificate_file, trusted) in cases {
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args(["--exact", test_name, "--nocapture"])
            .env("FVOCI_TLS_TEST_URL", format!("https://{address}"))
            .env("FVOCI_TLS_TEST_TRUST", if trusted { "1" } else { "0" })
            .env_remove("SSL_CERT_FILE")
            .env_remove("SSL_CERT_DIR");
        if let Some(file) = certificate_file {
            child.env("SSL_CERT_FILE", fixtures.join(file));
        }
        let output = child.output()?;
        assert!(
            output.status.success(),
            "certificate_file={certificate_file:?}, trust={trusted}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[tokio::test]
async fn private_ca_requires_ssl_cert_file() -> Result<(), Box<dyn Error>> {
    verify_ca_files(
        "private_ca_requires_ssl_cert_file",
        &[(None, false), (Some("ca.pem"), true)],
    )
    .await
}

#[tokio::test]
async fn private_ca_trusts_combined_bundle() -> Result<(), Box<dyn Error>> {
    verify_ca_files(
        "private_ca_trusts_combined_bundle",
        &[
            (Some("unrelated-ca.pem"), false),
            (Some("combined-ca.pem"), true),
        ],
    )
    .await
}
