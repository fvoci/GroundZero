//! 오류 응답과 운영 로그(D-83)가 함께 쓰는 값: 요청 id, 응답 오류 코드, 로그에 남겨도 되는 오류 분류.
//! 거르기·출력 형식은 서버의 `logging`이 정한다.

use crate::error::AppError;

/// 응답에 붙여 접근 줄이 읽는 오류 코드(응답 본문의 `error`와 같은 값).
#[derive(Debug, Clone, Copy)]
pub struct ErrorCode(pub &'static str);

tokio::task_local! {
    /// 지금 처리 중인 요청의 id. 오류 응답과 로그 줄에 같은 값을 넣는다.
    pub static REQUEST_ID: String;
}

/// 지금 요청의 id(요청 밖이면 없음).
#[must_use]
pub fn current_request_id() -> Option<String> {
    // 요청 밖에서 넘겨받은 빈 값(요청 id 없음)은 없음으로 본다.
    REQUEST_ID
        .try_with(Clone::clone)
        .ok()
        .filter(|id| !id.is_empty())
}

/// 로그에 남겨도 되는 오류 분류. 외부 서비스·DB·OS가 준 원문(주소·쿼리·입력값·서명이 섞일 수 있음)은 버리고,
/// 우리가 붙인 앞머리(`:` 앞)와 SQLSTATE·제약 이름·오류 종류만 남긴다.
#[must_use]
pub fn safe_error(err: &AppError) -> String {
    fn ours(msg: &str) -> &str {
        msg.split(':').next().unwrap_or_default().trim()
    }
    match err {
        AppError::Database(e) | AppError::Migrate(sqlx::migrate::MigrateError::Execute(e)) => {
            safe_sqlx(e)
        }
        AppError::Migrate(sqlx::migrate::MigrateError::ExecuteMigration(e, version)) => {
            format!("migration {version} failed: {}", safe_sqlx(e))
        }
        AppError::Migrate(sqlx::migrate::MigrateError::VersionMissing(version)) => {
            format!("migration {version} is applied but missing")
        }
        AppError::Migrate(sqlx::migrate::MigrateError::VersionMismatch(version)) => {
            format!("migration {version} was changed after it was applied")
        }
        AppError::Migrate(sqlx::migrate::MigrateError::Dirty(version)) => {
            format!("migration {version} is partially applied")
        }
        AppError::Migrate(_) => "migration failed".into(),
        AppError::Io(e) => format!("io {:?}", e.kind()),
        AppError::Oidc(msg) => format!("oidc {}", ours(msg)),
        AppError::Storage(msg) => format!("storage {}", ours(msg)),
        AppError::Upstream(msg) => format!("upstream {}", ours(msg)),
        AppError::Config(msg) => ours(msg).to_owned(),
        other => other.code().to_owned(),
    }
}

/// DB 오류의 분류(SQLSTATE·제약 이름, 또는 연결 오류의 종류). 메시지·쿼리·바인딩 값은 남기지 않는다.
#[must_use]
pub fn safe_sqlx(e: &sqlx::Error) -> String {
    if let Some(db) = e.as_database_error() {
        let code = db.code().map(|c| c.into_owned()).unwrap_or_default();
        return match db.constraint() {
            Some(c) => format!("database {code} constraint {c}"),
            None => format!("database {code}"),
        };
    }
    let kind = match e {
        sqlx::Error::PoolTimedOut => "pool timed out",
        sqlx::Error::PoolClosed => "pool closed",
        sqlx::Error::RowNotFound => "row not found",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "decode",
        _ => "other",
    };
    format!("database {kind}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_error_text_is_not_logged() {
        let err = AppError::Oidc("token endpoint: code=SECRET123 rejected".into());
        assert_eq!(safe_error(&err), "oidc token endpoint");
        let err = AppError::Storage("s3: https://bucket/key?X-Amz-Signature=abc".into());
        assert_eq!(safe_error(&err), "storage s3");
        assert_eq!(safe_error(&AppError::Forbidden), "forbidden");
    }
}
