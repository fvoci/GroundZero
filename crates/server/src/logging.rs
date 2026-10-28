//! 운영 로그(D-83). 기존 `tracing`을 그대로 쓰고 거르기와 출력 형식만 정한다.
//!
//! - 레벨·모듈 거르기: `FVOCI_LOG`(없으면 `info`. 옛 이름 `RUST_LOG`는 읽지 않는다, D-92). 예: `info,fvoci_server::collab=debug`, 서버 전체는
//!   `info,fvoci_server=debug,fvoci_core=debug,fvoci_db=debug,fvoci_http=debug`(공통 crate의 target은 `fvoci_core`·`fvoci_db`·`fvoci_http`).
//!   시작할 때 한 번 읽으므로 바꾸면 다시 시작해야 한다. 잘못 쓰면 시작하지 않는다.
//! - 형식: `FVOCI_LOG_FORMAT=json`(기본, 한 줄에 JSON 객체 하나) 또는 `pretty`(개발용, 같은 사건·필드를 한 줄
//!   글로). 둘 다 stdout으로, 색(ANSI)은 쓰지 않는다.
//! - 공통 필드: `timestamp`(UTC RFC 3339), `level`, `target`, `event`(기계용 코드, 없으면 `log`), `message`.
//!   요청 처리 중이면 `request_id`, 주기 작업 중이면 `job`·`job_id`가 붙는다(작업 단위 값이라 거르기와 상관없이
//!   붙는다). 그 밖의 필드는 사건이 준 그대로(숫자는 숫자).
//! - 민감정보: 요청·응답 본문, 헤더, 쿼리, 설정 값, DB 바인딩, 외부 서비스 오류 원문은 쓰지 않는다. 오류는
//!   `error_code`(응답 오류 코드와 같은 값)와 우리가 붙인 짧은 분류(`detail`, [`safe_error`](fvoci_core::logging::safe_error))만 남긴다. 헤더·본문을
//!   debug·trace에서 찍을 수 있는 전송·HTTP 클라이언트 라이브러리는 거르기와 상관없이 `info`보다 자세히 내보내지
//!   않는다([`LIBRARY_CAP`]).
//! - 감사 기록은 DB(`audit_log`)에 따로 남으므로 로그 레벨과 상관없다.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::future::Future;
use std::io::Write as _;

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{EnvFilter, FilterExt, LevelFilter, Targets};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

use fvoci_core::logging::current_request_id;

/// 레벨을 올려도 `info`보다 자세히 내보내지 않는 라이브러리(헤더·쿠키·본문·URL을 debug·trace에서 찍을 수 있는 것).
pub const LIBRARY_CAP: &[&str] = &[
    "hyper",
    "h2",
    "reqwest",
    "rustls",
    "native_tls",
    "tungstenite",
    "tokio_tungstenite",
    "openidconnect",
    "oauth2",
    "tower_http",
    "axum",
    "aws",
    "sqlx",
];

/// 출력 형식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Pretty,
}

/// 환경 변수에서 거르기와 형식을 읽는다.
///
/// # Errors
/// 거르기나 형식 값이 잘못되었을 때.
pub fn settings(lookup: impl Fn(&str) -> Option<String>) -> Result<(EnvFilter, Format), String> {
    let directives = lookup("FVOCI_LOG")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "info".to_owned());
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse(&directives)
        .map_err(|e| format!("FVOCI_LOG is not a valid filter: {e}"))?;
    let format = match lookup("FVOCI_LOG_FORMAT").as_deref().map(str::trim) {
        None | Some("" | "json") => Format::Json,
        Some("pretty") => Format::Pretty,
        Some(_) => return Err("FVOCI_LOG_FORMAT must be json or pretty".to_owned()),
    };
    Ok((filter, format))
}

/// 어느 레벨에서도 내보내지 않는 라이브러리. PDF 본문 뽑기(검색 색인)는 경고·오류에 파일 내용(스트림·글자)을 그대로
/// 찍는다. 실패는 우리 쪽 `search.extract_failed`로 남는다.
pub const LIBRARY_MUTED: &[&str] = &["pdf_extract", "lopdf"];

fn library_cap() -> Targets {
    let capped = LIBRARY_CAP.iter().fold(
        Targets::new().with_default(LevelFilter::TRACE),
        |t, name| t.with_target(*name, LevelFilter::INFO),
    );
    LIBRARY_MUTED
        .iter()
        .fold(capped, |t, name| t.with_target(*name, LevelFilter::OFF))
}

/// 거르기·상한을 붙인 로그 층. 시험은 쓰는 곳(`writer`)만 바꿔 같은 층을 쓴다.
pub fn layer<S, W>(filter: EnvFilter, format: Format, writer: W) -> impl Layer<S>
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    LineLayer { writer, format }.with_filter(filter.and(library_cap()))
}

/// 프로세스 전체의 로그를 시작한다(한 번). 패닉도 한 줄 JSON으로 남긴다(기본 훅은 여러 줄을 stderr에 쓰고, 패닉
/// 값에 다룬 데이터가 섞일 수 있어 위치만 남긴다).
pub fn init(filter: EnvFilter, format: Format) {
    tracing_subscriber::registry()
        .with(layer(filter, format, std::io::stdout))
        .init();
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        tracing::error!(
            event = "panic",
            location,
            "a task panicked (payload not logged)"
        );
    }));
}

/// 한 사건을 한 줄로 쓰는 층.
struct LineLayer<W> {
    writer: W,
    format: Format,
}

#[derive(Default)]
struct Collect(BTreeMap<String, Value>);

impl Visit for Collect {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().to_owned(), Value::String(value.to_owned()));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), Value::Bool(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let mut text = String::new();
        let _ignored = write!(text, "{value:?}");
        self.0.insert(field.name().to_owned(), Value::String(text));
    }
}

impl<S, W> Layer<S> for LineLayer<W>
where
    S: Subscriber,
    W: for<'w> MakeWriter<'w> + 'static,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut fields = Collect::default();
        event.record(&mut fields);
        let mut extra = fields.0;
        // `log` 크레이트에서 넘어온 사건의 위치 필드(log.file 등)는 버린다.
        extra.retain(|k, _| !k.starts_with("log."));
        // 공통 필드 이름과 겹치는 사건 필드는 공통 필드를 덮지 않게 이름을 바꾼다.
        for reserved in ["timestamp", "level", "target"] {
            if let Some(value) = extra.remove(reserved) {
                extra.insert(format!("field.{reserved}"), value);
            }
        }
        if let Some(id) = current_request_id() {
            extra
                .entry("request_id".into())
                .or_insert(Value::String(id));
        }
        if let Ok((job, id)) = JOB.try_with(|j| (j.name, j.id.clone())) {
            extra
                .entry("job".into())
                .or_insert(Value::String(job.into()));
            extra.entry("job_id".into()).or_insert(Value::String(id));
        }
        let event_code = extra
            .remove("event")
            .unwrap_or_else(|| Value::String("log".into()));
        let message = extra
            .remove("message")
            .unwrap_or_else(|| Value::String(String::new()));
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        let level = level_name(*meta.level());
        let mut text = match self.format {
            Format::Json => {
                let mut line = serde_json::Map::new();
                line.insert("timestamp".into(), Value::String(timestamp));
                line.insert("level".into(), Value::String(level.into()));
                line.insert("target".into(), Value::String(meta.target().to_owned()));
                line.insert("event".into(), event_code);
                line.insert("message".into(), message);
                line.extend(extra);
                serde_json::to_string(&Value::Object(line)).unwrap_or_default()
            }
            Format::Pretty => {
                let mut line = format!(
                    "{timestamp} {level:<5} {} {} {}",
                    meta.target(),
                    plain(&event_code),
                    plain(&message).escape_debug()
                );
                for (k, v) in &extra {
                    let _ignored = write!(line, " {k}={v}");
                }
                line
            }
        };
        if text.is_empty() {
            return;
        }
        text.push('\n');
        // 한 번에 쓴다(동시에 쓰는 줄이 섞이지 않게).
        let _ignored = self.writer.make_writer().write_all(text.as_bytes());
    }
}

fn plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
}

/// 주기 작업 하나의 이름과 실행 id.
#[derive(Debug, Clone)]
pub struct Job {
    pub name: &'static str,
    pub id: String,
}

tokio::task_local! {
    /// 지금 실행 중인 주기 작업.
    static JOB: Job;
}

/// 주기 작업 하나를 새 작업 id로 실행한다(그 안의 로그 줄에 `job`·`job_id`가 붙는다).
pub async fn in_job<F: Future>(name: &'static str, work: F) -> F::Output {
    let job = Job {
        name,
        id: uuid::Uuid::now_v7().to_string(),
    };
    JOB.scope(job, work).await
}

/// 받은 `x-request-id`를 그대로 쓸 수 있는지: 영문·숫자·`-`·`_`·`.` 8~64자(로그 주입 방지).
#[must_use]
pub fn acceptable_request_id(raw: &str) -> bool {
    (8..=64).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// HTTP 클라이언트 오류의 분류(주소·본문 없이).
#[must_use]
pub fn safe_reqwest(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timeout".into()
    } else if err.is_connect() {
        "connect".into()
    } else if let Some(status) = err.status() {
        format!("status {}", status.as_u16())
    } else if err.is_decode() {
        "decode".into()
    } else {
        "request".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_from_outside_are_checked() {
        assert!(acceptable_request_id(
            "0198f1f2-0000-7000-8000-000000000001"
        ));
        assert!(!acceptable_request_id("short"));
        assert!(!acceptable_request_id("has space and\nnewline!"));
        assert!(!acceptable_request_id(&"a".repeat(65)));
    }

    #[test]
    fn settings_default_to_info_json_and_reject_bad_values() {
        let none = |_: &str| None;
        let (filter, format) = settings(none).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(format, Format::Json);
        assert_eq!(filter.max_level_hint(), Some(LevelFilter::INFO));
        let pretty = |k: &str| (k == "FVOCI_LOG_FORMAT").then(|| "pretty".to_owned());
        assert_eq!(settings(pretty).map(|s| s.1).ok(), Some(Format::Pretty));
        let bad = |k: &str| (k == "FVOCI_LOG_FORMAT").then(|| "xml".to_owned());
        assert!(settings(bad).is_err());
        let bad = |k: &str| (k == "FVOCI_LOG").then(|| "info,=nonsense[".to_owned());
        assert!(settings(bad).is_err());
        // 옛 이름 RUST_LOG는 읽지 않는다(D-92). FVOCI_LOG가 없으면 기본 info다.
        let old_name = |k: &str| (k == "RUST_LOG").then(|| "error".to_owned());
        let (filter, _) = settings(old_name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(filter.max_level_hint(), Some(LevelFilter::INFO));
    }
}
