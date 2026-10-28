//! 선행 관계 명령(D-31, D-91): 관계를 만들거나 바꾸며 승인한 재배치를 같은 트랜잭션에서 옮기기, 관계 지우기.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use super::planner::{Change, Link, LinkKind, Span, same_shifts};
use super::preview::{check_lag, item_of, load_graph, plan_graph};
use crate::state::WorkState;
use crate::tasks::{
    EXPECTED_TASK_USER_DOC, MAX_SHIFTS, SuccessorShift, TaskDependency, load_dependency,
};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::UserId;
use fvoci_core::validate::{format_date, parse_date};
use fvoci_db::UserTx;
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::ExpectedUser;

/// 관계 미리보기·설정 입력.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkInput {
    pub kind: LinkKind,
    /// 0 이상의 달력 날짜 수.
    #[schema(minimum = 0, maximum = 3650)]
    pub lag_days: i32,
}

/// 관계 설정. 종류·지연일이 바뀌어 후속 업무를 옮겨야 하면 미리보기에서 승인한 목록을 함께 보낸다(같은 트랜잭션).
/// 비우면 관계만 바꾼다. `base`는 화면이 편집을 시작한 관계의 종류·지연일이다(새 관계면 `null`). 지금 관계와 다르면
/// 아무것도 바꾸지 않고 409 `link_conflict`(최신본 포함)다. 다른 사람이 그사이 바꾼 종류·지연일을 말없이 덮지 않는다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetPredecessor {
    pub kind: LinkKind,
    #[schema(minimum = 0, maximum = 3650)]
    pub lag_days: i32,
    #[serde(default)]
    pub base: Option<LinkBase>,
    #[serde(default)]
    #[schema(max_items = 500)]
    pub successor_shifts: Vec<SuccessorShift>,
}

/// 관계 편집을 시작한 때의 종류·지연일(관계 설정의 기준 조건).
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
pub struct LinkBase {
    pub kind: LinkKind,
    #[schema(minimum = 0, maximum = 3650)]
    pub lag_days: i32,
}

/// 승인한 재배치 목록: DB에 넘길 JSON과 확인용 (업무, 날짜).
pub(crate) fn approved_shifts(
    input: &[SuccessorShift],
) -> Result<(serde_json::Value, Vec<(Uuid, Span)>), AppError> {
    if input.len() > MAX_SHIFTS {
        return Err(AppError::invalid(
            "successor_shifts",
            InvalidReason::TooMany,
            Some(MAX_SHIFTS as u64),
            format!("at most {MAX_SHIFTS} successor shifts"),
        ));
    }
    let mut json = Vec::with_capacity(input.len());
    let mut spans = Vec::with_capacity(input.len());
    for shift in input {
        // 날짜 형식은 업무 날짜와 같은 규칙으로 확인한 뒤 DB에 넘긴다.
        let span = Span {
            start: parse_date(shift.start_date.as_deref(), "start_date")?,
            due: parse_date(shift.due_date.as_deref(), "due_date")?,
        };
        json.push(serde_json::json!({
            "task_id": shift.task_id,
            "base_version": shift.base_version,
            "start_date": format_date(span.start),
            "due_date": format_date(span.due),
        }));
        spans.push((shift.task_id, span));
    }
    Ok((serde_json::Value::Array(json), spans))
}

/// 옮기기 전 날짜(승인 목록과 바꾸는 업무). 잠그기 전에 읽지만, DB 함수가 이 업무들의 기준 버전을 잠근 뒤 확인하므로
/// 저장이 성공하면 잠근 때의 값과 같다.
pub(crate) async fn spans_before(
    tx: &mut UserTx,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Span>, AppError> {
    let rows: Vec<(Uuid, Option<Date>, Option<Date>)> =
        sqlx::query_as("SELECT id, start_date, due_date FROM tasks WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(tx.conn())
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, start, due)| (id, Span { start, due }))
        .collect())
}

/// 저장한 뒤(같은 트랜잭션, 관계·업무 잠금 안) 바꾸기 전 상태로 계획을 다시 세워 승인 목록과 같은지 본다.
/// `before`는 옮긴 업무의 이전 날짜, `undo`는 이번에 바꾼 관계 쌍과 그 이전 관계(없었으면 `None`).
pub(crate) async fn approved_still_planned(
    tx: &mut UserTx,
    task: Uuid,
    before: &HashMap<Uuid, Span>,
    undo: Option<((Uuid, Uuid), Option<Link>)>,
    change: Change,
    approved: &[(Uuid, Span)],
) -> Result<bool, AppError> {
    let item = item_of(tx, task).await?;
    let (mut rows, mut links) = load_graph(tx, item).await?;
    for (id, span) in before {
        if let Some(row) = rows.get_mut(id) {
            row.span = *span;
        }
    }
    if let Some((pair, old)) = undo {
        links.retain(|l| l.pair() != pair);
        links.extend(old);
    }
    Ok(same_shifts(&plan_graph(&rows, &links, change), approved))
}

/// 관계·승인 목록 충돌은 쓰기를 버린 뒤 현재 관계를 읽어 공통 409로 돌려준다.
async fn dependency_conflict(
    state: &WorkState,
    user: UserId,
    task: Uuid,
    predecessor: Uuid,
    error: &'static str,
    input: &SetPredecessor,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let current = load_dependency(&mut tx, predecessor, task).await?;
    tx.commit().await?;
    Ok(StaleWrite::new(
        error,
        "the dependency or approved schedule changed",
        current,
        &serde_json::json!({ "kind": input.kind, "lag_days": input.lag_days }),
    )
    .into_response())
}

/// 관계 설정의 409: 지금 관계(지워졌으면 null)와 다른 칸, 또는 계정·보관 등의 일반 오류.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DependencyWriteConflict {
    Stale(Box<StaleWrite<Option<TaskDependency>>>),
    State(ErrorBody),
}

/// 선행 업무 추가·설정(후속 업무 쓰기 권한). 같은 항목만, 자기 자신·순환·잘못된 종류나 지연일은 400.
///
/// 본문(`SetPredecessor`)의 종류·지연일로 만들거나 바꾸고, 승인한 재배치를 함께 옮긴다. 편집을 시작한 관계(`base`)가
/// 지금과 다르면 409 `link_conflict`, 승인 목록의 기준 버전이 지났거나 지금 다시 계산한 계획과 다르면 409
/// `shift_conflict`(둘 다 아무것도 바꾸지 않음).
#[utoipa::path(put, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}", operation_id = "add_predecessor",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    request_body = SetPredecessor,
    responses((status = 200, body = TaskDependency), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = DependencyWriteConflict,
               description = "`shift_conflict`·`link_conflict`면 StaleWrite(현재 관계 포함), 기대 사용자 불일치면 ErrorBody(`account_changed`), 보관·동시 변경·id 부족이면 ErrorBody(`invalid_state`)")),
    tag = "tasks")]
pub async fn add_predecessor(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
    Json(input): Json<SetPredecessor>,
) -> Result<Response, AppError> {
    check_lag(input.lag_days)?;
    let (shifts, approved) = approved_shifts(&input.successor_shifts)?;
    let mut tx = state.db.begin_as(user).await?;
    let ids: Vec<Uuid> = approved.iter().map(|&(id, _)| id).collect();
    let before = spans_before(&mut tx, &ids).await?;
    let (outcome, old_kind, old_lag): (String, Option<String>, Option<i32>) =
        sqlx::query_as("SELECT * FROM app_set_task_dependency($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(predecessor)
            .bind(id)
            .bind(input.kind.as_str())
            .bind(input.lag_days)
            .bind(input.base.map(|b| b.kind.as_str()))
            .bind(input.base.map(|b| b.lag_days))
            .bind(&shifts)
            .bind(fvoci_db::new_ids(1 + approved.len()))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    if outcome == "shift_conflict" {
        drop(tx);
        return dependency_conflict(&state, user, id, predecessor, "shift_conflict", &input).await;
    }
    if outcome == "link_conflict" {
        drop(tx);
        return dependency_conflict(&state, user, id, predecessor, "link_conflict", &input).await;
    }
    if !approved.is_empty() {
        let old = match (old_kind.as_deref().and_then(LinkKind::parse), old_lag) {
            (Some(kind), Some(lag_days)) => Some(Link {
                predecessor,
                successor: id,
                kind,
                lag_days,
            }),
            _ => None,
        };
        let new = Link {
            predecessor,
            successor: id,
            kind: input.kind,
            lag_days: input.lag_days,
        };
        let change = Change::Link { old, new };
        if !approved_still_planned(
            &mut tx,
            id,
            &before,
            Some((new.pair(), old)),
            change,
            &approved,
        )
        .await?
        {
            drop(tx);
            return dependency_conflict(&state, user, id, predecessor, "shift_conflict", &input)
                .await;
        }
    }
    let dependency = load_dependency(&mut tx, predecessor, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(Json(dependency).into_response())
}

/// 선행 업무 지우기(후속 업무 쓰기 권한). 관계를 지우면 제약이 줄 뿐이라 옮길 것이 없다.
#[utoipa::path(delete, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}", operation_id = "remove_predecessor",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn remove_predecessor(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_remove_task_dependency($1, $2, $3)")
        .bind(predecessor)
        .bind(id)
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
