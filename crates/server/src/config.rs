//! 환경 변수에서 읽는 서버 구성. 비밀 값은 `Secret`으로 감싸 로그에 출력하지 않는다.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;

use crate::error::AppError;

/// 로그·`Debug` 출력에서 값을 가리는 문자열.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// 원문. 연결·요청에만 쓴다.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// PostgreSQL 연결 문자열. 앱 역할(`fvoci_app`)로 접속한다.
pub type DatabaseUrl = Secret;

/// OIDC 제공자 설정 (D-45). 표준 OIDC만 쓴다.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// 발급자 URL. 디스커버리 문서의 `issuer`와 정확히 같아야 한다.
    pub issuer: Url,
    pub client_id: String,
    pub client_secret: Secret,
}

/// 운영 한도 (D-52). 기본값은 `O-26`에서 사용량·테스트 결과로 조정한다.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// 문서 본문 JSON 직렬화 크기 상한(바이트). `FVOCI_MAX_DOCUMENT_BYTES`.
    pub max_document_bytes: usize,
    /// 제목·이름 글자 수 상한. `FVOCI_MAX_TITLE_CHARS`.
    pub max_title_chars: usize,
    /// 문서 하나에 넣을 수 있는 참조(@ 링크·언급) 노드 수 상한. `FVOCI_MAX_REFERENCES_PER_DOCUMENT`.
    pub max_references_per_document: usize,
}

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
    /// 세션 절대 만료. `FVOCI_SESSION_TTL_SECS`, 기본 43200(12시간).
    pub session_ttl: Duration,
    /// 정적 웹 자산 디렉터리. `FVOCI_WEB_DIR`. 없으면 API만 서빙한다.
    pub web_dir: Option<PathBuf>,
    pub limits: Limits,
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
        let database_url = Secret(required(&lookup, "DATABASE_URL")?);
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
                client_secret: Secret(required(&lookup, "FVOCI_OIDC_CLIENT_SECRET")?),
            }),
            None => None,
        };

        let session_ttl = Duration::from_secs(parse_or(&lookup, "FVOCI_SESSION_TTL_SECS", 43_200)?);
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
        };

        Ok(Self {
            bind,
            database_url,
            public_url,
            oidc,
            session_ttl,
            web_dir,
            limits,
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

/// `db bootstrap`·`db migrate` 명령 구성. 서버 실행 구성과 분리한다.
#[derive(Debug, Clone)]
pub struct DbCommandConfig {
    /// 관리자(역할 생성 권한) 연결. `DATABASE_ADMIN_URL`. bootstrap에만 필요.
    pub admin_url: Option<Secret>,
    /// 마이그레이션 역할 연결. `DATABASE_MIGRATION_URL`.
    pub migration_url: Option<Secret>,
    /// bootstrap이 설정할 마이그레이션 역할 비밀번호. `FVOCI_DB_MIGRATOR_PASSWORD`.
    pub migrator_password: Option<Secret>,
    /// bootstrap이 설정할 앱 역할 비밀번호. `FVOCI_DB_APP_PASSWORD`.
    pub app_password: Option<Secret>,
}

impl DbCommandConfig {
    pub fn from_env() -> Self {
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(Secret)
        };
        Self {
            admin_url: get("DATABASE_ADMIN_URL"),
            migration_url: get("DATABASE_MIGRATION_URL"),
            migrator_password: get("FVOCI_DB_MIGRATOR_PASSWORD"),
            app_password: get("FVOCI_DB_APP_PASSWORD"),
        }
    }
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
}
