//! DB 연결 구성. 서버 실행 구성(서버의 `config`)은 여기서 연결 문자열 타입만 가져간다.

use fvoci_core::secret::Secret;

/// PostgreSQL 연결 문자열. 앱 역할(`fvoci_app`)로 접속한다.
pub type DatabaseUrl = Secret;

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
                .map(Secret::new)
        };
        Self {
            admin_url: get("DATABASE_ADMIN_URL"),
            migration_url: get("DATABASE_MIGRATION_URL"),
            migrator_password: get("FVOCI_DB_MIGRATOR_PASSWORD"),
            app_password: get("FVOCI_DB_APP_PASSWORD"),
        }
    }
}
