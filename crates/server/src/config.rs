//! 환경 변수에서 읽는 서버 구성. 비밀 값은 `Secret`으로 감싸 로그에 출력하지 않는다.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;

use fvoci_core::error::AppError;
use fvoci_core::secret::Secret;
use fvoci_db::config::DatabaseUrl;

/// OIDC 제공자 설정 (D-45). 표준 OIDC만 쓴다.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// 발급자 URL. 디스커버리 문서의 `issuer`와 정확히 같아야 한다.
    pub issuer: Url,
    pub client_id: String,
    pub client_secret: Secret,
}

/// 통합 검색(Meilisearch) 연결 (D-39). `FVOCI_SEARCH_URL`·`FVOCI_SEARCH_KEY`. 없으면 제목만 DB에서 찾는다.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub url: String,
    pub key: Secret,
}

pub use fvoci_files::FileLimits;
pub use fvoci_storage::config::{S3Config, StorageConfig};

/// 운영 한도 (D-52). 기본값은 `O-26`에서 사용량·테스트 결과로 조정한다.
pub use fvoci_documents::DocumentLimits as Limits;

/// 서버 실행 구성.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP 바인드 주소. `FVOCI_BIND`, 기본 `127.0.0.1:8080`.
    pub bind: SocketAddr,
    /// 앱 역할 연결 문자열. `DATABASE_URL`, 필수.
    pub database_url: DatabaseUrl,
    /// 브라우저가 접속하는 공개 원점. 리다이렉트 URI·쿠키 `Secure`·Origin 검사의 기준.
    /// `FVOCI_PUBLIC_URL`, 기본 `http://127.0.0.1:8080`.
    pub public_url: Url,
    /// OIDC 설정. `FVOCI_OIDC_ISSUER`·`FVOCI_OIDC_CLIENT_ID`·`FVOCI_OIDC_CLIENT_SECRET`. 없으면 로그인 불가.
    pub oidc: Option<OidcConfig>,
    /// 초기 관리자(D-51): 이 OIDC subject(설정한 발급자의 것)가 처음 로그인하면 첫 워크스페이스의 소유자가 된다.
    /// `FVOCI_INITIAL_ADMIN_SUBJECT`. 없으면 아무도 자동으로 관리자가 되지 않는다(X-10).
    pub initial_admin_subject: Option<String>,
    /// 그 첫 워크스페이스 이름. `FVOCI_INITIAL_WORKSPACE`, 기본 `fvoci`.
    pub initial_workspace: String,
    /// Zotero Web API 주소(D-34 읽기 연동). `FVOCI_ZOTERO_API_URL`, 기본 `https://api.zotero.org`(테스트는 대역 서버).
    pub zotero_api_url: Url,
    /// 세션 절대 만료. `FVOCI_SESSION_TTL_SECS`, 기본 43200(12시간).
    pub session_ttl: Duration,
    /// 실시간 편집 연결의 세션·권한 재확인 간격. `FVOCI_COLLAB_RECHECK_SECS`, 1~3600, 기본 30초(운영 기본값,
    /// docs/self-hosting.md의 `FVOCI_COLLAB_RECHECK_SECS` 설명).
    pub collab_recheck: Duration,
    /// 정적 웹 자산 디렉터리. `FVOCI_WEB_DIR`. 없으면 API만 서빙한다.
    pub web_dir: Option<PathBuf>,
    pub limits: Limits,
    /// 이 프로세스가 맡을 일. `FVOCI_ROLE`(`all`·`api`·`worker`), 기본 `all`(셀프호스트 한 프로세스).
    pub role: RunRole,
    /// 백그라운드 작업 확인 간격. `FVOCI_WORKER_INTERVAL_SECS`, 기본 60, 5~3600.
    pub worker_interval: Duration,
    /// 감사 기록 보존 일수. 환경 변수 미설정 시 730일.
    pub audit_retention_days: Option<i32>,
    /// 활동 기록 보존 일수. 환경 변수 미설정 시 365일.
    pub activity_retention_days: Option<i32>,
    pub search: Option<SearchConfig>,
    /// 파일 저장소.
    pub storage: StorageConfig,
    /// 파일 한도.
    pub files: FileLimits,
}

/// 실행 역할(O-19): API와 백그라운드 작업을 한 바이너리에 두고 실행할 때 고른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunRole {
    /// HTTP API와 백그라운드 작업을 함께.
    All,
    /// HTTP API만.
    Api,
    /// 백그라운드 작업만(HTTP를 열지 않는다).
    Worker,
}

impl RunRole {
    /// HTTP를 여는지.
    #[must_use]
    pub fn serves_http(self) -> bool {
        matches!(self, Self::All | Self::Api)
    }

    /// 백그라운드 작업을 도는지.
    #[must_use]
    pub fn runs_worker(self) -> bool {
        matches!(self, Self::All | Self::Worker)
    }
}

fn parse_or<T: std::str::FromStr>(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: T,
) -> Result<T, AppError>
where
    T::Err: std::fmt::Display,
{
    match lookup(key).filter(|v| !v.trim().is_empty()) {
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|e| AppError::Config(format!("{key} is invalid: {e}"))),
        None => Ok(default),
    }
}

fn required(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Result<String, AppError> {
    lookup(key)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Config(format!("{key} is required")))
}

impl Config {
    /// 프로세스 환경 변수에서 구성을 읽는다.
    pub fn from_env() -> Result<Self, AppError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// 조회 함수로 구성을 읽는다. 테스트에서 환경 변수 없이 쓴다.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, AppError> {
        let bind = parse_or(
            &lookup,
            "FVOCI_BIND",
            SocketAddr::from(([127, 0, 0, 1], 8080)),
        )?;
        let database_url = Secret::new(required(&lookup, "DATABASE_URL")?);
        let public_url: Url = parse_or(
            &lookup,
            "FVOCI_PUBLIC_URL",
            Url::parse("http://127.0.0.1:8080")
                .map_err(|e| AppError::Config(format!("default public url: {e}")))?,
        )?;
        if public_url.cannot_be_a_base() || public_url.host_str().is_none() {
            return Err(AppError::Config(
                "FVOCI_PUBLIC_URL must be an absolute http(s) URL".into(),
            ));
        }

        let oidc = match lookup("FVOCI_OIDC_ISSUER").filter(|v| !v.trim().is_empty()) {
            Some(issuer) => Some(OidcConfig {
                issuer: Url::parse(issuer.trim())
                    .map_err(|e| AppError::Config(format!("FVOCI_OIDC_ISSUER is invalid: {e}")))?,
                client_id: required(&lookup, "FVOCI_OIDC_CLIENT_ID")?,
                client_secret: Secret::new(required(&lookup, "FVOCI_OIDC_CLIENT_SECRET")?),
            }),
            None => None,
        };

        let session_ttl = Duration::from_secs(parse_or(&lookup, "FVOCI_SESSION_TTL_SECS", 43_200)?);
        // 0은 재확인을 끄는 것과 같아 1로 올리지 않고 거절한다. 상한 3600: 열린 연결에서 권한 철회 확인이 한 시간을
        // 넘게 늦어지면 D-70 실시간 편집의 연결 중 권한 재확인 취지에 어긋난다(저장 API·재연결은 매번 확인한다).
        let collab_recheck: u64 = parse_or(&lookup, "FVOCI_COLLAB_RECHECK_SECS", 30)?;
        if !(1..=3600).contains(&collab_recheck) {
            return Err(AppError::Config(
                "FVOCI_COLLAB_RECHECK_SECS must be between 1 and 3600".into(),
            ));
        }
        let collab_recheck = Duration::from_secs(collab_recheck);
        let web_dir = lookup("FVOCI_WEB_DIR")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from);
        let limits = Limits {
            max_document_bytes: parse_or(&lookup, "FVOCI_MAX_DOCUMENT_BYTES", 2_000_000)?,
            max_title_chars: parse_or(&lookup, "FVOCI_MAX_TITLE_CHARS", 200)?,
            max_references_per_document: parse_or(
                &lookup,
                "FVOCI_MAX_REFERENCES_PER_DOCUMENT",
                500,
            )?,
            max_document_depth: parse_or(&lookup, "FVOCI_MAX_DOCUMENT_DEPTH", 10)?,
        };
        // 1 이하는 트리를 조용히 끄고, 너무 크면 DB의 깊이 계산 상한(1000)을 넘어 한도가 무의미해진다.
        if !(2..=100).contains(&limits.max_document_depth) {
            return Err(AppError::Config(
                "FVOCI_MAX_DOCUMENT_DEPTH must be between 2 and 100".into(),
            ));
        }

        let role = match lookup("FVOCI_ROLE").as_deref().map(str::trim) {
            None | Some("" | "all") => RunRole::All,
            Some("api") => RunRole::Api,
            Some("worker") => RunRole::Worker,
            Some(other) => {
                return Err(AppError::Config(format!(
                    "FVOCI_ROLE must be all, api or worker (got {other})"
                )));
            }
        };
        let search = match lookup("FVOCI_SEARCH_URL").filter(|v| !v.trim().is_empty()) {
            Some(url) => {
                Url::parse(url.trim())
                    .map_err(|e| AppError::Config(format!("FVOCI_SEARCH_URL is invalid: {e}")))?;
                Some(SearchConfig {
                    url: url.trim().to_owned(),
                    key: Secret::new(required(&lookup, "FVOCI_SEARCH_KEY")?),
                })
            }
            None => None,
        };
        let worker_interval: u64 = parse_or(&lookup, "FVOCI_WORKER_INTERVAL_SECS", 60)?;
        if !(5..=3600).contains(&worker_interval) {
            return Err(AppError::Config(
                "FVOCI_WORKER_INTERVAL_SECS must be between 5 and 3600".into(),
            ));
        }

        let storage = match lookup("FVOCI_STORAGE").as_deref().map(str::trim) {
            None | Some("" | "local") => StorageConfig::Local {
                dir: PathBuf::from(
                    lookup("FVOCI_STORAGE_DIR")
                        .filter(|v| !v.trim().is_empty())
                        .unwrap_or_else(|| "data/files".to_owned()),
                ),
            },
            Some("s3") => {
                let endpoint = required(&lookup, "FVOCI_S3_ENDPOINT")?;
                let endpoint = Url::parse(endpoint.trim())
                    .map_err(|e| AppError::Config(format!("FVOCI_S3_ENDPOINT is invalid: {e}")))?;
                if endpoint.cannot_be_a_base() || endpoint.host_str().is_none() {
                    return Err(AppError::Config(
                        "FVOCI_S3_ENDPOINT must be an absolute http(s) URL".into(),
                    ));
                }
                StorageConfig::S3(S3Config {
                    endpoint,
                    region: lookup("FVOCI_S3_REGION")
                        .filter(|v| !v.trim().is_empty())
                        .unwrap_or_else(|| "auto".to_owned()),
                    bucket: required(&lookup, "FVOCI_S3_BUCKET")?,
                    access_key: required(&lookup, "FVOCI_S3_ACCESS_KEY")?,
                    secret_key: Secret::new(required(&lookup, "FVOCI_S3_SECRET_KEY")?),
                })
            }
            Some(other) => {
                return Err(AppError::Config(format!(
                    "FVOCI_STORAGE must be local or s3 (got {other})"
                )));
            }
        };
        const MIB: u64 = 1024 * 1024;
        let files = FileLimits {
            max_file_bytes: parse_or(&lookup, "FVOCI_MAX_FILE_BYTES", 1024 * MIB)?,
            chunk_bytes: parse_or(&lookup, "FVOCI_UPLOAD_CHUNK_BYTES", 8 * MIB)?,
            max_storage_bytes: parse_or(
                &lookup,
                "FVOCI_MAX_WORKSPACE_STORAGE_BYTES",
                50 * 1024 * MIB,
            )?,
            upload_ttl: Duration::from_secs(parse_or(&lookup, "FVOCI_UPLOAD_TTL_SECS", 86_400)?),
            upload_concurrency: parse_or(&lookup, "FVOCI_UPLOAD_CONCURRENCY", 8)?,
            export_concurrency: parse_or(&lookup, "FVOCI_EXPORT_CONCURRENCY", 2)?,
        };
        if !(1..=64).contains(&files.export_concurrency) {
            return Err(AppError::Config(
                "FVOCI_EXPORT_CONCURRENCY must be between 1 and 64".into(),
            ));
        }
        if !(1..=256).contains(&files.upload_concurrency) {
            return Err(AppError::Config(
                "FVOCI_UPLOAD_CONCURRENCY must be between 1 and 256".into(),
            ));
        }
        if !(5 * MIB..=64 * MIB).contains(&files.chunk_bytes) {
            return Err(AppError::Config(
                "FVOCI_UPLOAD_CHUNK_BYTES must be between 5 MiB and 64 MiB".into(),
            ));
        }
        // S3는 한 업로드에 조각 10000개까지.
        if files.max_file_bytes == 0 || files.max_file_bytes > files.chunk_bytes * 10_000 {
            return Err(AppError::Config(
                "FVOCI_MAX_FILE_BYTES must be positive and at most 10000 chunks".into(),
            ));
        }

        Ok(Self {
            bind,
            database_url,
            public_url,
            oidc,
            initial_admin_subject: lookup("FVOCI_INITIAL_ADMIN_SUBJECT")
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),
            initial_workspace: lookup("FVOCI_INITIAL_WORKSPACE")
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "fvoci".to_owned()),
            zotero_api_url: Url::parse(
                lookup("FVOCI_ZOTERO_API_URL")
                    .filter(|v| !v.trim().is_empty())
                    .as_deref()
                    .unwrap_or("https://api.zotero.org"),
            )
            .map_err(|e| AppError::Config(format!("FVOCI_ZOTERO_API_URL is invalid: {e}")))?,
            session_ttl,
            collab_recheck,
            web_dir,
            limits,
            role,
            worker_interval: Duration::from_secs(worker_interval),
            audit_retention_days: retention_days(
                lookup("FVOCI_AUDIT_RETENTION_DAYS"),
                "FVOCI_AUDIT_RETENTION_DAYS",
            )?
            .or(Some(730)),
            activity_retention_days: retention_days(
                lookup("FVOCI_ACTIVITY_RETENTION_DAYS"),
                "FVOCI_ACTIVITY_RETENTION_DAYS",
            )?
            .or(Some(365)),
            search,
            storage,
            files,
        })
    }

    /// 쿠키에 `Secure`를 붙일지. 공개 원점이 https일 때만.
    pub fn secure_cookies(&self) -> bool {
        self.public_url.scheme() == "https"
    }

    /// 공개 원점 문자열(`scheme://host[:port]`). Origin 헤더 비교에 쓴다.
    pub fn public_origin(&self) -> String {
        self.public_url.origin().ascii_serialization()
    }
}

fn retention_days(raw: Option<String>, name: &str) -> Result<Option<i32>, AppError> {
    raw.map(|value| {
        let days = value
            .trim()
            .parse::<i32>()
            .map_err(|_| AppError::Config(format!("{name} must be between 1 and 3650")))?;
        if !(1..=3650).contains(&days) {
            return Err(AppError::Config(format!(
                "{name} must be between 1 and 3650"
            )));
        }
        Ok(days)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_db(extra: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            if k == "DATABASE_URL" {
                return Some("postgres://u:secret@h/db".to_owned());
            }
            extra
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn retention_is_optional_and_bounded() {
        let cfg = Config::from_lookup(with_db(&[])).unwrap();
        assert_eq!(cfg.audit_retention_days, Some(730));
        assert_eq!(cfg.activity_retention_days, Some(365));
        for key in [
            "FVOCI_AUDIT_RETENTION_DAYS",
            "FVOCI_ACTIVITY_RETENTION_DAYS",
        ] {
            for value in ["1", "3650", " 30 "] {
                let cfg = Config::from_lookup(|name| {
                    if name == key {
                        Some(value.into())
                    } else {
                        with_db(&[])(name)
                    }
                })
                .unwrap();
                let expected = Some(value.trim().parse::<i32>().unwrap());
                if key == "FVOCI_AUDIT_RETENTION_DAYS" {
                    assert_eq!(cfg.audit_retention_days, expected);
                } else {
                    assert_eq!(cfg.activity_retention_days, expected);
                }
            }
            for value in ["0", "-1", "3651", "", "abc", "2147483648"] {
                assert!(
                    Config::from_lookup(|name| {
                        if name == key {
                            Some(value.into())
                        } else {
                            with_db(&[])(name)
                        }
                    })
                    .is_err()
                );
            }
        }
    }

    #[test]
    fn requires_database_url() {
        let err = Config::from_lookup(|_| None).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn defaults_and_redaction() {
        let cfg = Config::from_lookup(with_db(&[])).unwrap();
        assert_eq!(cfg.bind, SocketAddr::from(([127, 0, 0, 1], 8080)));
        assert!(cfg.oidc.is_none());
        assert!(!cfg.secure_cookies());
        assert_eq!(cfg.public_origin(), "http://127.0.0.1:8080");
        assert!(!format!("{cfg:?}").contains("secret"));
    }

    #[test]
    fn issuer_requires_client_settings() {
        let err = Config::from_lookup(with_db(&[("FVOCI_OIDC_ISSUER", "http://kc/realms/x")]))
            .unwrap_err();
        assert!(matches!(err, AppError::Config(m) if m.contains("FVOCI_OIDC_CLIENT_ID")));
    }

    #[test]
    fn https_public_url_enables_secure_cookies() {
        let cfg = Config::from_lookup(with_db(&[(
            "FVOCI_PUBLIC_URL",
            "https://fvoci.example.test",
        )]))
        .unwrap();
        assert!(cfg.secure_cookies());
    }

    #[test]
    fn rejects_invalid_limit() {
        let err = Config::from_lookup(with_db(&[("FVOCI_MAX_TITLE_CHARS", "-1")])).unwrap_err();
        assert!(matches!(err, AppError::Config(m) if m.contains("FVOCI_MAX_TITLE_CHARS")));
    }

    #[test]
    fn rejects_depth_outside_range() {
        const BAD: [&[(&str, &str)]; 3] = [
            &[("FVOCI_MAX_DOCUMENT_DEPTH", "0")],
            &[("FVOCI_MAX_DOCUMENT_DEPTH", "1")],
            &[("FVOCI_MAX_DOCUMENT_DEPTH", "101")],
        ];
        for bad in BAD {
            let err = Config::from_lookup(with_db(bad)).unwrap_err();
            assert!(matches!(err, AppError::Config(m) if m.contains("FVOCI_MAX_DOCUMENT_DEPTH")));
        }
        assert!(Config::from_lookup(with_db(&[("FVOCI_MAX_DOCUMENT_DEPTH", "3")])).is_ok());
    }

    #[test]
    fn collab_recheck_defaults_to_30_and_accepts_1_to_3600() {
        let cfg = Config::from_lookup(with_db(&[])).unwrap();
        assert_eq!(cfg.collab_recheck, Duration::from_secs(30));
        const EDGES: [(&[(&str, &str)], u64); 2] = [
            (&[("FVOCI_COLLAB_RECHECK_SECS", "1")], 1),
            (&[("FVOCI_COLLAB_RECHECK_SECS", "3600")], 3600),
        ];
        for (env, secs) in EDGES {
            let cfg = Config::from_lookup(with_db(env)).unwrap();
            assert_eq!(cfg.collab_recheck, Duration::from_secs(secs));
        }
    }

    /// 0은 예전처럼 1로 올리지 않고 거절한다.
    #[test]
    fn rejects_collab_recheck_outside_range() {
        const BAD: [&[(&str, &str)]; 2] = [
            &[("FVOCI_COLLAB_RECHECK_SECS", "0")],
            &[("FVOCI_COLLAB_RECHECK_SECS", "3601")],
        ];
        for bad in BAD {
            let err = Config::from_lookup(with_db(bad)).unwrap_err();
            assert!(
                matches!(err, AppError::Config(m) if m.contains("FVOCI_COLLAB_RECHECK_SECS must be between"))
            );
        }
    }
}
