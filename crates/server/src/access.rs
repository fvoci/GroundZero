//! 접근 등급을 읽는 질의 도우미(D-113). 등급 타입은 `fvoci_core::access`에 있다. 쓰기의 404(볼 수 없음)·409(보관된
//! 항목)·403(권한 부족) 구분도 SQL(`app_require_document` 등)이 정하고, 변경 함수는 스스로 검사하므로 서버의 사전 검사는
//! 읽기 경로와 응답 코드 구분에만 쓴다. DB 정의는 `crates/db/migrations/202610280002_organizations.sql`.

pub use fvoci_core::access::{Grade, Kind, Target};
use fvoci_core::error::AppError;
use fvoci_db::UserTx;

/// 현재 사용자의 등급. 볼 수 없으면 `None`.
pub async fn grade<T: Target>(
    tx: &mut UserTx,
    kind: Kind,
    id: T,
) -> Result<Option<Grade>, AppError> {
    let raw: String = sqlx::query_scalar("SELECT app_access($1, $2)")
        .bind(kind.as_str())
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    Grade::parse(&raw)
}

/// 볼 수 있어야 한다. 없으면 존재를 숨기고 404.
pub async fn require_visible<T: Target>(
    tx: &mut UserTx,
    kind: Kind,
    id: T,
) -> Result<Grade, AppError> {
    grade(tx, kind, id).await?.ok_or(AppError::NotFound)
}

/// 쓸 수 있어야 한다. 문서는 SQL `app_require_document`가 404/보관 409/403을 정하고, 그 밖의 종류는 등급으로
/// 404(없음)·403(부족)을 낸다(보관된 항목 아래 쓰기의 409는 변경 함수가 낸다).
pub async fn require_write<T: Target>(
    tx: &mut UserTx,
    kind: Kind,
    id: T,
) -> Result<Grade, AppError> {
    if kind == Kind::Document {
        let raw: String = sqlx::query_scalar("SELECT app_require_document($1, 'write')")
            .bind(id)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        return Grade::parse_present(&raw);
    }
    match grade(tx, kind, id).await? {
        None => Err(AppError::NotFound),
        Some(g) if g.can_write() => Ok(g),
        Some(_) => Err(AppError::Forbidden),
    }
}

/// 소유 권한이어야 한다(설정·공유·관리). 문서는 SQL이 정하고, 그 밖은 등급으로 404·403.
pub async fn require_owner<T: Target>(tx: &mut UserTx, kind: Kind, id: T) -> Result<(), AppError> {
    if kind == Kind::Document {
        sqlx::query("SELECT app_require_document($1, 'owner')")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        return Ok(());
    }
    match grade(tx, kind, id).await? {
        None => Err(AppError::NotFound),
        Some(Grade::Owner) => Ok(()),
        Some(_) => Err(AppError::Forbidden),
    }
}
