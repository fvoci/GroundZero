//! 환경 변수에서 읽는 서버 구성. 비밀 값은 로그에 출력하지 않는다.

use std::net::SocketAddr;

use crate::error::AppError;

/// 서버 실행 구성.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP 바인드 주소. `FVOCI_BIND`, 기본 `127.0.0.1:8080`.
    pub bind: SocketAddr,
    /// PostgreSQL 연결 문자열. `DATABASE_URL`, 필수.
    pub database_url: DatabaseUrl,
}

/// 연결 문자열. `Debug` 출력에서 값을 가린다.
#[derive(Clone)]
pub struct DatabaseUrl(String);

impl DatabaseUrl {
    /// 연결에 쓸 원문.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatabaseUrl(<redacted>)")
    }
}

impl Config {
    /// 프로세스 환경 변수에서 구성을 읽는다.
    pub fn from_env() -> Result<Self, AppError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// 조회 함수로 구성을 읽는다. 테스트에서 환경 변수 없이 쓴다.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, AppError> {
        let bind = match lookup("FVOCI_BIND") {
            Some(raw) => raw.parse().map_err(|e| {
                AppError::Config(format!("FVOCI_BIND is not a socket address: {e}"))
            })?,
            None => SocketAddr::from(([127, 0, 0, 1], 8080)),
        };
        let database_url = lookup("DATABASE_URL")
            .filter(|v| !v.trim().is_empty())
            .map(DatabaseUrl)
            .ok_or_else(|| AppError::Config("DATABASE_URL is required".to_owned()))?;
        Ok(Self { bind, database_url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_database_url() {
        let err = Config::from_lookup(|_| None).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn defaults_bind_and_redacts_url() {
        let cfg = Config::from_lookup(|k| {
            (k == "DATABASE_URL").then(|| "postgres://u:secret@h/db".to_owned())
        })
        .unwrap();
        assert_eq!(cfg.bind, SocketAddr::from(([127, 0, 0, 1], 8080)));
        assert!(!format!("{cfg:?}").contains("secret"));
    }
}
