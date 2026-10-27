#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 종료 신호: 컨테이너를 멈추면 오는 SIGTERM에도 정상 종료 절차(실시간 방 리비전 남기기·작업 멈춤·DB 닫기)를 밟는다.
//! 실제 서버 실행 파일을 띄워 신호를 보낸다. 고치기 전에는 Ctrl-C(SIGINT)만 받아 SIGTERM에 그냥 죽었다(컨테이너에서는
//! PID 1이라 신호를 무시하다 10초 뒤 강제 종료).

#![cfg(unix)]

use crate::common;

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 시험이 실패해도(패닉) 띄운 서버를 남기지 않는다.
struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            self.0.kill().ok();
            self.0.wait().ok();
        }
    }
}

#[test]
fn server_shuts_down_cleanly_on_sigterm() {
    // 서버는 뜨자마자 주기 작업(휴지통 비우기·저장소 정리 등)을 한 번 돈다. 공유 테스트 DB를 바꾸지 않게 새 DB를 쓴다.
    let app_url = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            use sqlx::Connection as _;
            let admin_url =
                std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
            let mut admin = sqlx::PgConnection::connect(&admin_url).await.unwrap();
            // 현재 서버는 구 SQL 이력의 DB를 시작하지 않는다. 종료 신호 계약은 새 제품 DB에서 확인한다.
            common::fresh_database(&mut admin, "shutdown").await.app_url
        });
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fvoci-server"));
    command
        .env("FVOCI_BIND", format!("127.0.0.1:{port}"))
        .env("FVOCI_PUBLIC_URL", common::PUBLIC_URL)
        .env("FVOCI_LOG_FORMAT", "json")
        .env("DATABASE_URL", &app_url)
        // 첫 주기 다음은 잡지 않는다.
        .env("FVOCI_WORKER_INTERVAL_SECS", "3600")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, _) in std::env::vars() {
        if key.starts_with("FVOCI_OIDC_") {
            command.env_remove(key);
        }
    }
    let mut child = Running(command.spawn().unwrap());

    // 받을 준비가 될 때까지 기다린다.
    let deadline = Instant::now() + Duration::from_secs(30);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "server did not start listening");
        assert!(child.0.try_wait().unwrap().is_none(), "server exited early");
        std::thread::sleep(Duration::from_millis(100));
    }

    let status = Command::new("kill")
        .args(["-TERM", &child.0.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(15);
    let exit = loop {
        if let Some(exit) = child.0.try_wait().unwrap() {
            break exit;
        }
        assert!(Instant::now() < deadline, "server ignored SIGTERM");
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut out = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let mut err = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let logs = format!("{out}{err}");
    assert!(exit.success(), "exit {exit:?}\n{logs}");
    assert!(logs.contains("\"server.shutdown\""), "{logs}");
}
