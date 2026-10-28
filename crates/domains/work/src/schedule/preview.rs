//! 재배치 미리보기(쓰기 없음, D-91·D-125): 날짜를 바꿀 때와 관계를 만들거나 바꿀 때 옮길 후속 업무.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use super::links::LinkInput;
use super::planner::{
    Change, Link, LinkKind, MAX_LAG_DAYS, Plan, PlanBlockReason, Span, plan_block, plan_shifts,
    reaches,
};
use crate::state::WorkState;
use crate::tasks::MAX_SHIFTS;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::UserId;
use fvoci_core::validate::{format_date, parse_date};
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;

/// 미리보기 요청: 바꾸려는 날짜.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SchedulePreviewInput {
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 재배치의 원인 관계: 선행 업무와 관계 종류·지연일.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShiftCause {
    #[schema(value_type = String, format = Uuid)]
    pub predecessor_id: Uuid,
    pub predecessor_reference: Option<String>,
    pub predecessor_title: String,
    pub kind: LinkKind,
    pub lag_days: i32,
}

/// 옮겨야 하는 후속 업무 한 건(현재 → 제안).
#[derive(Debug, Serialize, ToSchema)]
pub struct ShiftProposal {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub reference: Option<String>,
    pub title: String,
    /// 승인할 때 그대로 보내는 기준 버전.
    pub base_version: i32,
    pub from_start_date: Option<String>,
    pub from_due_date: Option<String>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    /// 이 업무를 옮기게 한 관계(가장 많이 요구한 것, 여럿이면 모두).
    pub causes: Vec<ShiftCause>,
}

/// 날짜가 없어 옮기지 못하는 업무.
#[derive(Debug, Serialize, ToSchema)]
pub struct UndatedTask {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub reference: Option<String>,
    pub title: String,
}

/// 재배치 미리보기: 옮길 후속 업무와, 바뀐 관계가 닿지만 날짜가 없어 계산하지 않은 업무(날짜를 지어내지 않는다).
#[derive(Debug, Serialize, ToSchema)]
pub struct SchedulePlan {
    pub shifts: Vec<ShiftProposal>,
    pub undated: Vec<UndatedTask>,
    /// 이 계획대로는 함께 저장할 수 없는 까닭(승인 전에 알린다). 있으면 "옮겨 저장"은 거절되고, 옮기지 않고 저장하기나
    /// 취소만 된다. 계획을 자르거나 일부 업무를 빼지 않는다.
    pub blocked: Option<PlanBlocked>,
}

/// 저장할 수 없는 계획의 까닭과 그 근거(건수 또는 범위 밖 날짜를 받은 업무).
#[derive(Debug, Serialize, ToSchema)]
pub struct PlanBlocked {
    pub reason: PlanBlockReason,
    pub shift_count: usize,
    pub max_shifts: usize,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub task_id: Option<Uuid>,
    pub reference: Option<String>,
    /// 범위 밖의 제안 날짜(`YYYY-MM-DD`).
    pub date: Option<String>,
}

pub(super) struct Row {
    reference: Option<String>,
    title: String,
    version: i32,
    pub(super) span: Span,
    done: bool,
}

/// 항목의 업무 날짜와 관계(사용자 권한으로 읽는다).
pub(super) async fn load_graph(
    tx: &mut UserTx,
    item: Uuid,
) -> Result<(HashMap<Uuid, Row>, Vec<Link>), AppError> {
    type Raw = (
        Uuid,
        Option<String>,
        String,
        i32,
        Option<Date>,
        Option<Date>,
        bool,
    );
    let rows: Vec<Raw> = sqlx::query_as(
        "SELECT t.id, app_task_reference(t.id), t.title, t.version, t.start_date, t.due_date, s.category = 'done'
         FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.item_id = $1",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let raw: Vec<(Uuid, Uuid, String, i32)> = sqlx::query_as(
        "SELECT predecessor_id, successor_id, kind, lag_days FROM task_dependencies WHERE item_id = $1",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let links = raw
        .into_iter()
        .map(|(predecessor, successor, kind, lag_days)| {
            Ok(Link {
                predecessor,
                successor,
                kind: LinkKind::parse(&kind)
                    .ok_or_else(|| AppError::Config(format!("unknown dependency kind {kind}")))?,
                lag_days,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let rows = rows
        .into_iter()
        .map(|(id, reference, title, version, start, due, done)| {
            (
                id,
                Row {
                    reference,
                    title,
                    version,
                    span: Span { start, due },
                    done,
                },
            )
        })
        .collect();
    Ok((rows, links))
}

pub(super) fn plan_graph(rows: &HashMap<Uuid, Row>, links: &[Link], change: Change) -> Plan {
    let spans: HashMap<Uuid, Span> = rows.iter().map(|(&id, r)| (id, r.span)).collect();
    // 완료한 업무는 옮기자고 하지 않는다.
    let done: HashSet<Uuid> = rows
        .iter()
        .filter(|(_, r)| r.done)
        .map(|(&id, _)| id)
        .collect();
    plan_shifts(&spans, links, &done, change)
}

fn plan_view(rows: &HashMap<Uuid, Row>, plan: Plan) -> SchedulePlan {
    let blocked = plan_block(&plan).map(|(reason, at)| PlanBlocked {
        reason,
        shift_count: plan.shifts.len(),
        max_shifts: MAX_SHIFTS,
        task_id: at.map(|(id, _)| id),
        reference: at.and_then(|(id, _)| rows.get(&id)?.reference.clone()),
        date: at.and_then(|(_, d)| format_date(Some(d))),
    });
    let shifts = plan
        .shifts
        .into_iter()
        .filter_map(|p| {
            let row = rows.get(&p.task)?;
            Some(ShiftProposal {
                task_id: p.task,
                reference: row.reference.clone(),
                title: row.title.clone(),
                base_version: row.version,
                from_start_date: format_date(row.span.start),
                from_due_date: format_date(row.span.due),
                start_date: format_date(p.span.start),
                due_date: format_date(p.span.due),
                causes: p
                    .causes
                    .iter()
                    .map(|l| {
                        let pred = rows.get(&l.predecessor);
                        ShiftCause {
                            predecessor_id: l.predecessor,
                            predecessor_reference: pred.and_then(|r| r.reference.clone()),
                            predecessor_title: pred.map(|r| r.title.clone()).unwrap_or_default(),
                            kind: l.kind,
                            lag_days: l.lag_days,
                        }
                    })
                    .collect(),
            })
        })
        .collect();
    let undated = plan
        .undated
        .into_iter()
        .filter_map(|id| {
            let row = rows.get(&id)?;
            Some(UndatedTask {
                task_id: id,
                reference: row.reference.clone(),
                title: row.title.clone(),
            })
        })
        .collect();
    SchedulePlan {
        shifts,
        undated,
        blocked,
    }
}

pub(super) async fn item_of(tx: &mut UserTx, task: Uuid) -> Result<Uuid, AppError> {
    let item: Option<Uuid> = sqlx::query_scalar("SELECT item_id FROM tasks WHERE id = $1")
        .bind(task)
        .fetch_optional(tx.conn())
        .await?;
    item.ok_or(AppError::NotFound)
}

fn date_input(input: &SchedulePreviewInput) -> Result<Span, AppError> {
    let next = Span {
        start: parse_date(input.start_date.as_deref(), "start_date")?,
        due: parse_date(input.due_date.as_deref(), "due_date")?,
    };
    if let (Some(start), Some(due)) = (next.start, next.due)
        && start > due
    {
        return Err(AppError::BadRequest(
            "the start date must not be after the due date".into(),
        ));
    }
    Ok(next)
}

pub(super) fn check_lag(lag_days: i32) -> Result<(), AppError> {
    if (0..=MAX_LAG_DAYS).contains(&lag_days) {
        Ok(())
    } else {
        Err(AppError::invalid(
            "lag_days",
            InvalidReason::OutOfRange,
            Some(MAX_LAG_DAYS.unsigned_abs().into()),
            format!("lag_days must be between 0 and {MAX_LAG_DAYS}"),
        ))
    }
}

async fn date_plan(
    state: &WorkState,
    user: UserId,
    id: Uuid,
    input: &SchedulePreviewInput,
) -> Result<SchedulePlan, AppError> {
    let next = date_input(input)?;
    let mut tx = state.db.begin_as(user).await?;
    let item = item_of(&mut tx, id).await?;
    let (rows, links) = load_graph(&mut tx, item).await?;
    tx.commit().await?;
    let plan = plan_graph(&rows, &links, Change::Move { task: id, next });
    Ok(plan_view(&rows, plan))
}

/// 날짜를 바꿀 때의 재배치 계획(읽기). 옮길 후속 업무와 원인 관계, 날짜가 없어 계산하지 않은 업무.
#[utoipa::path(post, path = "/api/tasks/{task_id}/schedule-plan", operation_id = "plan_task_schedule",
    params(("task_id" = String, Path, format = Uuid)), request_body = SchedulePreviewInput,
    responses((status = 200, body = SchedulePlan), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn plan(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SchedulePreviewInput>,
) -> Result<Json<SchedulePlan>, AppError> {
    Ok(Json(date_plan(&state, user, id, &input).await?))
}

/// 관계를 만들거나 바꿀 때의 재배치 계획(읽기). 같은 항목만, 자기 자신·순환·잘못된 지연일은 400, 볼 수 없으면 404.
#[utoipa::path(post, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}/schedule-plan",
    operation_id = "plan_predecessor_schedule",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid)),
    request_body = LinkInput,
    responses((status = 200, body = SchedulePlan), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn plan_link(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
    Json(input): Json<LinkInput>,
) -> Result<Json<SchedulePlan>, AppError> {
    check_lag(input.lag_days)?;
    if id == predecessor {
        return Err(AppError::BadRequest(
            "a task cannot depend on itself".into(),
        ));
    }
    let mut tx = state.db.begin_as(user).await?;
    let item = item_of(&mut tx, id).await?;
    if item_of(&mut tx, predecessor).await? != item {
        return Err(AppError::BadRequest(
            "dependencies must stay within one item".into(),
        ));
    }
    let (rows, links) = load_graph(&mut tx, item).await?;
    tx.commit().await?;
    let old = links
        .iter()
        .copied()
        .find(|l| l.pair() == (predecessor, id));
    if old.is_none() && reaches(&links, id, predecessor) {
        return Err(AppError::BadRequest(
            "the dependency would create a cycle".into(),
        ));
    }
    let new = Link {
        predecessor,
        successor: id,
        kind: input.kind,
        lag_days: input.lag_days,
    };
    let plan = plan_graph(&rows, &links, Change::Link { old, new });
    Ok(Json(plan_view(&rows, plan)))
}
