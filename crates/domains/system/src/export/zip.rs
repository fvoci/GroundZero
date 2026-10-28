use std::fmt::Write as _;
use std::io::Write;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::state::ObjectStore;
use fvoci_core::error::AppError;

/// 응답으로 보내는 조각 크기.
const CHUNK: usize = 64 * 1024;

/// ZIP 쓰기 스레드가 받는 것.
enum Msg {
    Start(String),
    Data(Bytes),
    /// 만들다 실패: 잘린 ZIP이 멀쩡해 보이지 않게 응답을 끊는다.
    Abort(String),
}

/// 담을 것.
pub(super) enum Entry {
    Bytes(String, Vec<u8>),
    Object(String, String),
}

/// ZIP 출력을 응답 본문 채널로 보내는 쓰기 대상(블로킹 스레드에서 쓴다).
struct BodyWriter {
    tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    buf: Vec<u8>,
}

impl BodyWriter {
    fn send(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::take(&mut self.buf));
        self.tx
            .blocking_send(Ok(chunk))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "client went away"))
    }
}

impl Write for BodyWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= CHUNK {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

fn zip_error(err: zip::result::ZipError) -> std::io::Error {
    std::io::Error::other(err)
}

/// ZIP 안 이름 조각(경로 구분자·제어 문자 제거, 길이 제한).
pub(super) fn safe_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    let short: String = trimmed.chars().take(80).collect();
    if short.is_empty() { "_".into() } else { short }
}

/// 내려받을 ZIP 이름 머리글(ASCII 대체 이름 + RFC 5987 UTF-8 이름).
fn disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            let _ignored = write!(encoded, "%{b:02X}");
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// 담을 것을 ZIP으로 만들며 바로 응답으로 흘려보낸다. 쓰기는 블로킹 스레드, 저장소 읽기는 비동기 작업.
pub(super) fn stream_zip(
    storage: Arc<dyn ObjectStore>,
    entries: Vec<Entry>,
    name: &str,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Response {
    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    let (msg_tx, msg_rx) = mpsc::channel::<Msg>(8);
    let writer_tx = body_tx.clone();
    let request_id = fvoci_core::logging::current_request_id();
    tokio::task::spawn_blocking(move || write_zip(writer_tx, body_tx, msg_rx, request_id));
    tokio::spawn(send_entries(storage, entries, msg_tx, permit));
    let mut response =
        Body::from_stream(futures_util::stream::unfold(body_rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition(name)) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// 구조 데이터에서 담을 목록을 만든다: `export.json`, 문서 일반 글자, 파일 버전 내용.
pub(super) fn entries_from(
    mut data: Value,
    docs_key: &str,
    plain_text: fn(&Value) -> String,
) -> Result<Vec<Entry>, AppError> {
    let objects = data
        .as_object_mut()
        .and_then(|o| o.remove("_objects"))
        .unwrap_or(Value::Null);
    let mut entries = vec![Entry::Bytes(
        "export.json".into(),
        serde_json::to_vec_pretty(&data).map_err(|e| AppError::Config(format!("export: {e}")))?,
    )];
    for doc in data[docs_key].as_array().into_iter().flatten() {
        let title = doc["title"].as_str().unwrap_or_default();
        let label = doc["reference"].as_str().map_or_else(
            || doc["id"].as_str().unwrap_or("_").replace('-', ""),
            safe_name,
        );
        let text = format!("{title}\n\n{}", plain_text(&doc["body"]));
        entries.push(Entry::Bytes(
            format!("documents/{label} {}.txt", safe_name(title)),
            text.into_bytes(),
        ));
    }
    for object in objects.as_array().into_iter().flatten() {
        let (Some(file), Some(name), Some(version), Some(key)) = (
            object["file"].as_str(),
            object["name"].as_str(),
            object["version"].as_i64(),
            object["key"].as_str(),
        ) else {
            continue;
        };
        entries.push(Entry::Object(
            format!(
                "files/{}/v{version} {}",
                file.replace('-', ""),
                safe_name(name)
            ),
            key.to_owned(),
        ));
    }
    Ok(entries)
}

/// 블로킹 ZIP 쓰기에는 요청 id를 직접 넘긴다(D-83).
fn write_zip(
    writer_tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    body_tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    mut msg_rx: mpsc::Receiver<Msg>,
    request_id: Option<String>,
) {
    // 저장소 실패로 멈춘 경우의 분류(우리가 만든 안전한 문구). 그 밖에는 io 오류 종류만 남긴다.
    let mut aborted: Option<String> = None;
    let result = (|| -> std::io::Result<()> {
        let mut zip = zip::ZipWriter::new_stream(BodyWriter {
            tx: writer_tx,
            buf: Vec::new(),
        });
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        while let Some(msg) = msg_rx.blocking_recv() {
            match msg {
                Msg::Start(name) => zip.start_file(name, options).map_err(zip_error)?,
                Msg::Data(bytes) => zip.write_all(&bytes)?,
                Msg::Abort(reason) => {
                    aborted = Some(reason.clone());
                    return Err(std::io::Error::other(reason));
                }
            }
        }
        let mut out = zip.finish().map_err(zip_error)?;
        out.flush()
    })();
    if let Err(err) = result {
        let detail = aborted.unwrap_or_else(|| format!("io {:?}", err.kind()));
        tracing::warn!(
            event = "export.stopped",
            request_id = request_id.as_deref(),
            detail,
            "export stopped"
        );
        let _sent = body_tx.blocking_send(Err(err));
    }
}

async fn send_entries(
    storage: Arc<dyn ObjectStore>,
    entries: Vec<Entry>,
    msg_tx: mpsc::Sender<Msg>,
    permit: tokio::sync::OwnedSemaphorePermit,
) {
    let _permit = permit;
    let mut missing = Vec::new();
    for entry in entries {
        let sent = match entry {
            Entry::Bytes(name, bytes) => {
                msg_tx.send(Msg::Start(name)).await.is_ok()
                    && msg_tx.send(Msg::Data(Bytes::from(bytes))).await.is_ok()
            }
            Entry::Object(name, key) => match storage.get(&key).await {
                Err(AppError::NotFound) => {
                    missing.push(name);
                    true
                }
                Err(err) => {
                    let _sent = msg_tx
                        .send(Msg::Abort(fvoci_core::logging::safe_error(&err)))
                        .await;
                    return;
                }
                Ok(mut stream) => {
                    let mut ok = msg_tx.send(Msg::Start(name)).await.is_ok();
                    while ok {
                        match stream.next().await {
                            None => break,
                            Some(Ok(chunk)) => ok = msg_tx.send(Msg::Data(chunk)).await.is_ok(),
                            Some(Err(err)) => {
                                let _sent = msg_tx
                                    .send(Msg::Abort(format!("storage read {:?}", err.kind())))
                                    .await;
                                return;
                            }
                        }
                    }
                    ok
                }
            },
        };
        if !sent {
            return;
        }
    }
    if !missing.is_empty() {
        let note = serde_json::json!({
            "note": "these file versions were permanently deleted while the export was running",
            "files": missing,
        });
        let _sent = msg_tx.send(Msg::Start("missing.json".into())).await.is_ok()
            && msg_tx
                .send(Msg::Data(Bytes::from(note.to_string())))
                .await
                .is_ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_inside_the_archive_stay_inside() {
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name("회의록: 1/2"), "회의록_ 1_2");
        assert_eq!(safe_name("  "), "_");
        assert_eq!(safe_name(&"가".repeat(200)).chars().count(), 80);
    }
}
