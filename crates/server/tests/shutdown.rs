#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 종료 신호: 컨테이너를 멈추면 오는 SIGTERM에도 정상 종료 절차(실시간 방 리비전 남기기·작업 멈춤·DB 닫기)를 밟는다.
//! 실제 서버 실행 파일을 띄워 신호를 보낸다. 고치기 전에는 Ctrl-C(SIGINT)만 받아 SIGTERM에 그냥 죽었다(컨테이너에서는
//! PID 1이라 신호를 무시하다 10초 뒤 강제 종료).

#![cfg(unix)]

mod common;

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn server_shuts_down_cleanly_on_sigterm() {
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
        // 잠깐 사는 동안 공유 테스트 DB에서 주기 작업(휴지통 비우기 등)을 잡지 않게 한다.
        .env("FVOCI_WORKER_INTERVAL_SECS", "3600")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, _) in std::env::vars() {
        if key.starts_with("FVOCI_OIDC_") {
            command.env_remove(key);
        }
    }
    let mut child = command.spawn().unwrap();

    // 받을 준비가 될 때까지 기다린다.
    let deadline = Instant::now() + Duration::from_secs(30);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "server did not start listening");
        assert!(child.try_wait().unwrap().is_none(), "server exited early");
        std::thread::sleep(Duration::from_millis(100));
    }

    let status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(15);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        assert!(Instant::now() < deadline, "server ignored SIGTERM");
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let logs = format!("{out}{err}");
    assert!(exit.success(), "exit {exit:?}\n{logs}");
    assert!(logs.contains("\"server.shutdown\""), "{logs}");
}
