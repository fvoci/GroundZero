//! 접근 등급(D-113). SQL `app_access(kind, id)` 하나가 none/read/write/owner를 계산하고 서버는 그 결과만 쓴다.
//! 역할·보관·개별 공유·조직 경계로 등급을 다시 계산하지 않는다. 쓰기의 404(볼 수 없음)·409(보관된 항목)·403(권한
//! 부족) 구분도 SQL(`app_require_document` 등)이 정하고, 변경 함수는 스스로 검사하므로 서버의 사전 검사는 읽기 경로와
//! 응답 코드 구분에만 쓴다. DB 정의는 `crates/db/migrations/202610280002_organizations.sql`.

use serde::{Deserialize, Serialize};
use sqlx::Postgres;
use utoipa::ToSchema;

use fvoci_core::error::AppError;
use fvoci_db::UserTx;

/// `app_access`가 아는 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Organization,
    Workspace,
    Module,
    Item,
    Document,
    Task,
    Event,
    File,
    Record,
    Comment,
    Episode,
    StudyUnit,
}

impl Kind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Organization => "organization",
            Self::Workspace => "workspace",
            Self::Module => "module",
            Self::Item => "item",
            Self::Document => "document",
            Self::Task => "task",
            Self::Event => "event",
            Self::File => "file",
            Self::Record => "record",
            Self::Comment => "comment",
            Self::Episode => "episode",
            Self::StudyUnit => "study_unit",
        }
    }
}

/// 볼 수 있는 대상에 대한 등급. 볼 수 없으면(`none`) `Option`의 `None`이다. 응답에서는 `access` 칸으로 나간다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Access)]
pub enum Grade {
    Read,
    Write,
    /// 소유 권한: 관리자, 개인 노트 소유자, 항목 리더 등. 설정·공유·이동·상태 정의를 할 수 있다.
    Owner,
}

impl Grade {
    /// `app_access` 결과 문자열. `none`은 `None`, 모르는 값은 DB 계약 위반(500)이다.
    pub fn parse(raw: &str) -> Result<Option<Self>, AppError> {
        match raw {
            "none" => Ok(None),
            "read" => Ok(Some(Self::Read)),
            "write" => Ok(Some(Self::Write)),
            "owner" => Ok(Some(Self::Owner)),
            other => Err(AppError::Config(format!(
                "unknown access grade from the database: {other}"
            ))),
        }
    }

    /// 볼 수 있어야 하는 행(RLS를 지난 SELECT 결과)의 등급. `none`이면 DB 계약 위반이다.
    pub fn parse_present(raw: &str) -> Result<Self, AppError> {
        Self::parse(raw)?
            .ok_or_else(|| AppError::Config("a visible row reported access 'none'".to_owned()))
    }

    pub const fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::Owner)
    }

    pub const fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// `app_access`에 넘길 수 있는 id(uuid 새 타입 전부).
pub trait Target:
    for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + Send + Sync + Copy
{
}

impl<T> Target for T where
    T: for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + Send + Sync + Copy
{
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_parse_from_the_database_words() {
        assert_eq!(Grade::parse("owner").unwrap(), Some(Grade::Owner));
        assert_eq!(Grade::parse("none").unwrap(), None);
        assert!(Grade::Write.can_write());
        assert!(!Grade::Read.can_write());
        assert!(Grade::Owner.is_owner() && !Grade::Write.is_owner());
        assert!(Grade::parse("other").is_err());
        assert!(Grade::parse_present("none").is_err());
    }

    #[test]
    fn kinds_match_the_sql_names() {
        assert_eq!(Kind::StudyUnit.as_str(), "study_unit");
        assert_eq!(Kind::Organization.as_str(), "organization");
    }
}
