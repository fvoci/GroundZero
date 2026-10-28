//! 업무 고치기의 저장 한 번(D-94, D-91·D-125): 칸 전체를 기준 판 조건으로 저장하고, 승인한 후속 재배치를 같은 트랜잭션에서
//! 옮긴다. 옛 `PUT /api/tasks/{id}`와 새 `PATCH /api/tasks/{id}`가 함께 쓴다.

use std::collections::HashMap;

use time::Date;
use uuid::Uuid;

use super::model::{SuccessorShift, TaskPriority};
use crate::schedule::{self, Change, Span};
use crate::state::WorkState;
use fvoci_core::error::AppError;
use fvoci_core::ids::UserId;
use fvoci_db::needed_ids;

/// 확인을 마친 업무 칸 전체(저장할 모습).
#[derive(Debug)]
pub(super) struct Edit {
    pub(super) title: String,
    pub(super) description: String,
    /// 상태 이름(검증한 값).
    pub(super) status: String,
    pub(super) priority: TaskPriority,
    pub(super) start: Option<Date>,
    pub(super) due: Option<Date>,
    pub(super) estimate_minutes: Option<i32>,
    pub(super) tags: Vec<String>,
    pub(super) assignee_ids: Vec<UserId>,
    pub(super) milestone: bool,
}

/// 저장 결과. 충돌이면 아무것도 바뀌지 않았다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Saved {
    Updated,
    /// 이 업무의 기준 판이 지났다(`conflict`).
    Conflict,
    /// 승인한 후속 재배치가 그사이 맞지 않게 됐다(`shift_conflict`).
    ShiftConflict,
}

/// 기준 판 `base_version`에서 `edit`로 저장한다. 반복 업무를 고쳤으면 그 반복의 회차를 별도 트랜잭션에서 맞춘다(실패해도
/// 고치기는 이미 끝났으므로 기록만 한다).
pub(super) async fn save_edit(
    state: &WorkState,
    user: UserId,
    id: Uuid,
    base_version: i32,
    edit: &Edit,
    successor_shifts: &[SuccessorShift],
) -> Result<Saved, AppError> {
    let (shifts, approved) = schedule::approved_shifts(successor_shifts)?;
    let mut tx = state.db.begin_as(user).await?;
    // 승인 목록을 다시 계산해 볼 때 쓰는 옮기기 전 날짜(이 업무와 옮길 업무).
    let before = if approved.is_empty() {
        HashMap::new()
    } else {
        let ids: Vec<Uuid> = std::iter::once(id)
            .chain(approved.iter().map(|&(t, _)| t))
            .collect();
        schedule::spans_before(&mut tx, &ids).await?
    };
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_update_task_ids($1, $2, $3)")
            .bind(id)
            .bind(&edit.assignee_ids)
            .bind(&shifts),
    )
    .await?;
    let outcome: String = sqlx::query_scalar(
        "SELECT app_update_task($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(id)
    .bind(base_version)
    .bind(&edit.title)
    .bind(&edit.description)
    .bind(&edit.status)
    .bind(edit.priority.as_str())
    .bind(edit.start)
    .bind(edit.due)
    .bind(edit.estimate_minutes)
    .bind(&edit.tags)
    .bind(&edit.assignee_ids)
    .bind(edit.milestone)
    .bind(&shifts)
    .bind(ids)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    match outcome.as_str() {
        "conflict" => return Ok(Saved::Conflict),
        // 승인할 때 본 후속 업무가 그사이 바뀌었다. 아무것도 바꾸지 않았으니 다시 미리 본다.
        "shift_conflict" => return Ok(Saved::ShiftConflict),
        _ => {}
    }
    // 승인한 재배치가 지금(잠근 뒤) 상태로 다시 세운 계획과 같아야 한다. 그사이 관계·종류·지연일·다른 업무의
    // 날짜나 완료가 바뀌어 다르면 아무것도 바꾸지 않는다(D-91). 승인 목록이 비면(옮기지 않기로 함) 보지 않는다.
    if !approved.is_empty() {
        let next = Span {
            start: edit.start,
            due: edit.due,
        };
        let change = Change::Move { task: id, next };
        if !schedule::approved_still_planned(&mut tx, id, &before, None, change, &approved).await? {
            return Ok(Saved::ShiftConflict);
        }
    }
    let series: Option<Uuid> = sqlx::query_scalar(
        "SELECT s.id FROM tasks t JOIN task_series s ON s.id = t.series_id
         WHERE t.id = $1 AND s.rule IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    if let Some(series) = series
        && let Err(e) = crate::recurrence::sync_series(state, user, series).await
    {
        tracing::warn!(event = "recurrence.sync_failed", error_code = e.code(), detail = fvoci_core::logging::safe_error(&e), %series, "recurrence sync after task update failed");
    }
    Ok(Saved::Updated)
}
