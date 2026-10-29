//! 공유 편집·개인 입력·보관을 구분하는 학습 자원의 상태.

use fvoci_core::{access::Grade, error::AppError, ids::ItemId};
use fvoci_db::UserTx;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LearningAccess {
    Write,
    Read,
    Archived,
}

pub(crate) enum LearningKind {
    Course,
    Study,
}

pub(crate) async fn access(
    tx: &mut UserTx,
    item: ItemId,
    kind: LearningKind,
) -> Result<LearningAccess, AppError> {
    let sql = match kind {
        LearningKind::Course => "SELECT app_course_access($1)",
        LearningKind::Study => "SELECT app_study_access($1)",
    };
    let raw: String = sqlx::query_scalar(sql)
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let grade = Grade::parse_present(&raw)?;
    let archived: bool = sqlx::query_scalar(
        "SELECT i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL
         FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_one(tx.conn())
    .await?;
    Ok(if archived {
        LearningAccess::Archived
    } else if grade.can_write() {
        LearningAccess::Write
    } else {
        LearningAccess::Read
    })
}
