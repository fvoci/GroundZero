//! Exercise the default HTTP client's OS trust store in separate processes.
//! Requires the existing OpenSSL CLI to serve the test-only certificate fixtures.

use std::error::Error;
use std::fmt::Write as _;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

#[tokio::test]
async fn private_ca_requires_ssl_cert_file() -> Result<(), Box<dyn Error>> {
    // Process-local environment avoids changing trust for concurrent tests.
    if let Ok(url) = std::env::var("FVOCI_TLS_TEST_URL") {
        return probe(&url).await;
    }
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tls");
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
    for trusted in [false, true] {
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "private_ca_requires_ssl_cert_file",
                "--nocapture",
            ])
            .env("FVOCI_TLS_TEST_URL", format!("https://{address}"))
            .env("FVOCI_TLS_TEST_TRUST", if trusted { "1" } else { "0" })
            .env_remove("SSL_CERT_FILE")
            .env_remove("SSL_CERT_DIR");
        if trusted {
            child.env("SSL_CERT_FILE", fixtures.join("ca.pem"));
        }
        let output = child.output()?;
        assert!(
            output.status.success(),
            "trust={trusted}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
