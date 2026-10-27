//! 마일스톤·선행 관계·후속 재배치 미리보기 (D-31, D-33, D-91).
//!
//! 선행 관계는 같은 항목 안의 업무 쌍마다 하나이고 종류(FS·SS·FF·SF)와 지연일(0 이상, 달력 날짜)을 가진다. 날짜나
//! 관계를 바꾸기 전에 미리보기로 영향받는 후속 업무와 제안 날짜, 그 원인 관계를 계산해 보여 주고, 사용자가 승인한
//! 목록만 같은 트랜잭션에서 옮긴다. 제안은 바뀐 것(옮겨진 선행 또는 고친 관계)이 전보다 더 요구하는 만큼만 뒤로 미는
//! 최소 이동이며 기간(시작~마감)을 유지한다. 앞당긴 경우에는 제안하지 않는다. 승인할 때는 잠근 뒤 같은 계산을 다시
//! 해서 승인 목록과 다르면(그사이 관계·날짜·완료가 바뀜) 아무것도 바꾸지 않는다.
//! 관계의 종류·지연일과 앱 공급 UUIDv7(D-88)은
//! `crates/db/migrations/202610250001_initial_state.sql`의 `app_set_task_dependency` 등이 처리한다.

use std::collections::{HashMap, HashSet, VecDeque};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::{Date, Duration};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::accounts::{CurrentUser, ExpectedUser};
use crate::features::tasks::{
    DATE_YEARS, EXPECTED_TASK_USER_DOC, MAX_SHIFTS, SuccessorShift, TaskConflict, TaskDetail,
    format_date, load_detail, parse_date,
};
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::UserId;
use fvoci_db::UserTx;

/// 업무의 날짜 칸.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Option<Date>,
    pub due: Option<Date>,
}

impl Span {
    fn begin(self) -> Option<Date> {
        self.start.or(self.due)
    }

    fn end(self) -> Option<Date> {
        self.due.or(self.start)
    }

    fn shifted(self, days: i64) -> Option<Self> {
        let by = Duration::days(days);
        Some(Self {
            start: match self.start {
                Some(d) => Some(d.checked_add(by)?),
                None => None,
            },
            due: match self.due {
                Some(d) => Some(d.checked_add(by)?),
                None => None,
            },
        })
    }
}

/// 선행 관계의 종류(D-91). 선행 A, 후속 B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum LinkKind {
    /// 종료 후 시작: B는 A가 끝난 다음 날(+지연일) 이후에 시작한다.
    #[serde(rename = "FS")]
    FinishToStart,
    /// 시작 후 시작: B는 A가 시작한 날(+지연일) 이후에 시작한다.
    #[serde(rename = "SS")]
    StartToStart,
    /// 종료 후 종료: B는 A가 끝난 날(+지연일) 이후에 끝난다.
    #[serde(rename = "FF")]
    FinishToFinish,
    /// 시작 후 종료: B는 A가 시작하기 전날(+지연일) 이후에 끝난다(A가 시작할 때 B가 끝나 있어도 된다).
    #[serde(rename = "SF")]
    StartToFinish,
}

impl LinkKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FinishToStart => "FS",
            Self::StartToStart => "SS",
            Self::FinishToFinish => "FF",
            Self::StartToFinish => "SF",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "FS" => Some(Self::FinishToStart),
            "SS" => Some(Self::StartToStart),
            "FF" => Some(Self::FinishToFinish),
            "SF" => Some(Self::StartToFinish),
            _ => None,
        }
    }
}

/// 지연일 상한(DB 제약과 같다).
pub const MAX_LAG_DAYS: i32 = 3650;

/// 선행 관계 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub predecessor: Uuid,
    pub successor: Uuid,
    pub kind: LinkKind,
    pub lag_days: i32,
}

impl Link {
    fn pair(self) -> (Uuid, Uuid) {
        (self.predecessor, self.successor)
    }

    /// 선행 `pred`의 날짜로 이 관계가 후속에 요구하는 가장 이른 날(FS·SS는 후속 시작일, FF·SF는 후속 마감일).
    fn bound(self, pred: Span) -> Option<Date> {
        let lag = Duration::days(i64::from(self.lag_days));
        match self.kind {
            LinkKind::FinishToStart => pred.end()?.checked_add(lag + Duration::days(1)),
            LinkKind::StartToStart => pred.begin()?.checked_add(lag),
            LinkKind::FinishToFinish => pred.end()?.checked_add(lag),
            LinkKind::StartToFinish => pred.begin()?.checked_add(lag - Duration::days(1)),
        }
    }

    /// 후속 `succ`가 이 관계를 채우려면 더 미뤄야 하는 날 수(이미 채우면 0 이하). 어느 쪽이든 날짜가 없으면 없음.
    fn need(self, pred: Span, succ: Span) -> Option<i64> {
        let bound = self.bound(pred)?;
        let at = match self.kind {
            LinkKind::FinishToStart | LinkKind::StartToStart => succ.begin()?,
            LinkKind::FinishToFinish | LinkKind::StartToFinish => succ.end()?,
        };
        Some((bound - at).whole_days())
    }
}

/// 미리 볼 변경.
#[derive(Debug, Clone, Copy)]
pub enum Change {
    /// 업무 `task`의 날짜를 `next`로 바꾼다.
    Move { task: Uuid, next: Span },
    /// 관계를 만들거나(`old` 없음) 종류·지연일을 바꾼다.
    Link { old: Option<Link>, new: Link },
}

/// 옮길 후속 업무 한 건과 그렇게 옮기게 한 관계(가장 많이 요구한 것, 여럿이면 모두).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub task: Uuid,
    pub span: Span,
    pub causes: Vec<Link>,
}

/// 재배치 계획: 옮길 업무(위상 순서)와, 바뀐 관계가 더 요구하지만 날짜가 없어 확인하지 못한 업무.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub shifts: Vec<Planned>,
    pub undated: Vec<Uuid>,
}

/// `change` 때문에 뒤로 밀어야 하는 후속 업무와 새 날짜.
///
/// `links`는 바꾸기 전 관계다. 후속 업무는 모든 선행 관계를 채워야 하지만, 이번에 옮겨진 선행이나 이번에 고친 관계만
/// 보며 그 관계가 전보다 더 요구하는 만큼만 민다(전부터 어긋나 있던 만큼은 제안하지 않는다). 다른 관계는 하한이라
/// 뒤로 미는 것이 어긋나게 하지 않는다. 여러 선행이 요구하면 가장 많이 요구한 만큼 민다. 날짜가 없는 업무와
/// `fixed`(완료한 업무)는 옮기지 않고 그 뒤로 제약을 전하지 않는다.
#[must_use]
pub fn plan_shifts(
    spans: &HashMap<Uuid, Span>,
    links: &[Link],
    fixed: &HashSet<Uuid>,
    change: Change,
) -> Plan {
    let (start, changed) = match change {
        Change::Move { task, .. } => (task, None),
        Change::Link { new, .. } => (new.successor, Some(new)),
    };
    // 바꾼 뒤 관계와, 각 관계의 바꾸기 전 모습(새로 만든 관계는 없음).
    let after: Vec<Link> = match changed {
        None => links.to_vec(),
        Some(new) => links
            .iter()
            .copied()
            .filter(|l| l.pair() != new.pair())
            .chain([new])
            .collect(),
    };
    let before_of = |l: Link| -> Option<Link> {
        match change {
            Change::Link { old, new } if l.pair() == new.pair() => old,
            _ => Some(l),
        }
    };
    let mut successors: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for l in &after {
        successors
            .entry(l.predecessor)
            .or_default()
            .push(l.successor);
    }
    // 바뀐 곳에서 닿는 업무.
    let mut affected: HashSet<Uuid> = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        for &succ in successors.get(&id).map_or(&[][..], Vec::as_slice) {
            if affected.insert(succ) {
                queue.push_back(succ);
            }
        }
    }
    // 영향 범위 안의 위상 순서(Kahn). DB가 순환을 막지만 남는 것이 있으면 건너뛴다.
    let inner: Vec<Link> = after
        .iter()
        .copied()
        .filter(|l| affected.contains(&l.predecessor) && affected.contains(&l.successor))
        .collect();
    let mut indegree: HashMap<Uuid, usize> = affected.iter().map(|&id| (id, 0)).collect();
    for l in &inner {
        *indegree.entry(l.successor).or_default() += 1;
    }
    let mut ready: Vec<Uuid> = indegree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();
    ready.sort_unstable();
    let mut ready = VecDeque::from(ready);
    let mut current: HashMap<Uuid, Span> = spans.clone();
    let mut moved: HashSet<Uuid> = HashSet::new();
    if let Change::Move { task, next } = change {
        current.insert(task, next);
        moved.insert(task);
    }
    let mut plan = Plan::default();
    while let Some(id) = ready.pop_front() {
        if !moved.contains(&id)
            && !fixed.contains(&id)
            && let Some(span) = current.get(&id).copied()
        {
            // 이번 변경이 닿은 관계: 선행이 옮겨졌거나 관계 자체를 고쳤다.
            let touched: Vec<Link> = after
                .iter()
                .copied()
                .filter(|l| {
                    l.successor == id
                        && (moved.contains(&l.predecessor)
                            || Some(l.pair()) == changed.map(Link::pair))
                })
                .collect();
            if span.begin().is_none() {
                // 날짜가 없으면 옮기지 않는다. 바뀐 관계가 전보다 늦은 날을 요구하면 알린다.
                let later = touched.iter().any(|&l| {
                    let now = current.get(&l.predecessor).and_then(|&p| l.bound(p));
                    let was = before_of(l)
                        .and_then(|b| spans.get(&l.predecessor).and_then(|&p| b.bound(p)));
                    now.is_some() && (Some(l.pair()) == changed.map(Link::pair) || now > was)
                });
                if later {
                    plan.undated.push(id);
                }
            } else {
                // 관계마다 더 미뤄야 하는 날: 지금 요구 - 전부터 요구하던 것(전에 채웠으면 0).
                let wants: Vec<(Link, i64)> = touched
                    .iter()
                    .filter_map(|&l| {
                        let now = l.need(*current.get(&l.predecessor)?, span)?;
                        let was = before_of(l)
                            .and_then(|b| b.need(*spans.get(&l.predecessor)?, span))
                            .map_or(0, |w| w.max(0));
                        Some((l, now - was))
                    })
                    .collect();
                if let Some(days) = wants.iter().map(|&(_, d)| d).max().filter(|&d| d > 0)
                    && let Some(next) = span.shifted(days)
                {
                    moved.insert(id);
                    current.insert(id, next);
                    plan.shifts.push(Planned {
                        task: id,
                        span: next,
                        causes: wants
                            .iter()
                            .filter(|&&(_, d)| d == days)
                            .map(|&(l, _)| l)
                            .collect(),
                    });
                }
            }
        }
        let mut freed = Vec::new();
        for l in &inner {
            if l.predecessor == id
                && let Some(d) = indegree.get_mut(&l.successor)
            {
                *d -= 1;
                if *d == 0 {
                    freed.push(l.successor);
                }
            }
        }
        freed.sort_unstable();
        ready.extend(freed);
    }
    plan
}

/// 승인 목록이 계획과 같은지(업무·날짜 집합). 승인은 미리보기 전체를 한꺼번에 하므로 같아야 한다.
fn same_shifts(plan: &Plan, approved: &[(Uuid, Span)]) -> bool {
    let mut planned: Vec<(Uuid, Option<Date>, Option<Date>)> = plan
        .shifts
        .iter()
        .map(|p| (p.task, p.span.start, p.span.due))
        .collect();
    let mut sent: Vec<(Uuid, Option<Date>, Option<Date>)> = approved
        .iter()
        .map(|&(id, s)| (id, s.start, s.due))
        .collect();
    planned.sort_unstable();
    sent.sort_unstable();
    planned == sent
}

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

/// 저장할 수 없는 계획의 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanBlockReason {
    /// 옮길 후속 업무가 한 번에 옮길 수 있는 수(`max_shifts`)보다 많다.
    TooManyShifts,
    /// 제안 날짜가 저장할 수 있는 범위(1900~2200년) 밖이다.
    DateOutOfRange,
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

/// 계획을 저장할 수 있는지: 건수 한도(승인 저장과 같은 `MAX_SHIFTS`)를 먼저, 그다음 제안 날짜의 범위(저장과 같은
/// `DATE_YEARS`)를 본다. 막히면 (까닭, 범위 밖 날짜를 받은 업무와 그 날짜).
fn plan_block(plan: &Plan) -> Option<(PlanBlockReason, Option<(Uuid, Date)>)> {
    if plan.shifts.len() > MAX_SHIFTS {
        return Some((PlanBlockReason::TooManyShifts, None));
    }
    plan.shifts.iter().find_map(|p| {
        [p.span.start, p.span.due]
            .into_iter()
            .flatten()
            .find(|d| !DATE_YEARS.contains(&d.year()))
            .map(|d| (PlanBlockReason::DateOutOfRange, Some((p.task, d))))
    })
}

/// 관계 미리보기·설정 입력.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkInput {
    pub kind: LinkKind,
    /// 0 이상의 달력 날짜 수.
    pub lag_days: i32,
}

/// 관계 설정. 종류·지연일이 바뀌어 후속 업무를 옮겨야 하면 미리보기에서 승인한 목록을 함께 보낸다(같은 트랜잭션).
/// 비우면 관계만 바꾼다. `base`는 화면이 편집을 시작한 관계의 종류·지연일이다(새 관계면 `null`). 지금 관계와 다르면
/// 아무것도 바꾸지 않고 409 `link_conflict`(최신본 포함)다. 다른 사람이 그사이 바꾼 종류·지연일을 말없이 덮지 않는다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetPredecessor {
    pub kind: LinkKind,
    pub lag_days: i32,
    #[serde(default)]
    pub base: Option<LinkBase>,
    #[serde(default)]
    pub successor_shifts: Vec<SuccessorShift>,
}

/// 관계 편집을 시작한 때의 종류·지연일(관계 설정의 기준 조건).
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
pub struct LinkBase {
    pub kind: LinkKind,
    pub lag_days: i32,
}

/// 업무의 선행 관계 하나.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskLink {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub kind: LinkKind,
    pub lag_days: i32,
}

struct Row {
    reference: Option<String>,
    title: String,
    version: i32,
    span: Span,
    done: bool,
}

/// 항목의 업무 날짜와 관계(사용자 권한으로 읽는다).
async fn load_graph(
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

fn plan_graph(rows: &HashMap<Uuid, Row>, links: &[Link], change: Change) -> Plan {
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

async fn item_of(tx: &mut UserTx, task: Uuid) -> Result<Uuid, AppError> {
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

fn check_lag(lag_days: i32) -> Result<(), AppError> {
    if (0..=MAX_LAG_DAYS).contains(&lag_days) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "lag_days must be between 0 and {MAX_LAG_DAYS}"
        )))
    }
}

async fn date_plan(
    state: &AppState,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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

/// `from`에서 관계를 따라 `to`에 닿는지.
fn reaches(links: &[Link], from: Uuid, to: Uuid) -> bool {
    let mut seen = HashSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(id) = queue.pop_front() {
        if id == to {
            return true;
        }
        for l in links.iter().filter(|l| l.predecessor == id) {
            if seen.insert(l.successor) {
                queue.push_back(l.successor);
            }
        }
    }
    false
}

/// 승인한 재배치 목록: DB에 넘길 JSON과 확인용 (업무, 날짜).
pub(crate) fn approved_shifts(
    input: &[SuccessorShift],
) -> Result<(serde_json::Value, Vec<(Uuid, Span)>), AppError> {
    if input.len() > MAX_SHIFTS {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_SHIFTS} successor shifts"
        )));
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

/// 승인 목록이 그사이 맞지 않게 됐을 때의 409 응답(아무것도 바꾸지 않았다). 최신본은 새 트랜잭션에서 읽는다.
pub(crate) async fn shift_conflict(
    state: &AppState,
    user: UserId,
    task: Uuid,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let current = load_detail(&mut tx, task).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CONFLICT,
        Json(TaskConflict {
            error: "shift_conflict",
            current,
        }),
    )
        .into_response())
}

/// 편집을 시작한 관계(`base`)가 그사이 바뀌었을 때의 409 응답(아무것도 바꾸지 않았다). 최신본은 새 트랜잭션에서 읽는다.
pub(crate) async fn link_conflict(
    state: &AppState,
    user: UserId,
    task: Uuid,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let current = load_detail(&mut tx, task).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CONFLICT,
        Json(TaskConflict {
            error: "link_conflict",
            current,
        }),
    )
        .into_response())
}

/// 관계 설정의 409 본문(계약 문서용, D-91). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `TaskConflict`(`error: "shift_conflict"` 또는 `"link_conflict"`, `current`): 승인한 재배치가 그사이 맞지 않게
///   됨, 또는 편집을 시작한 관계(`base`)가 그사이 바뀌거나 생기거나 지워짐.
/// - `ErrorBody`(`current` 없음): 기대 사용자 불일치(`account_changed`), 보관된 항목,
///   동시 변경 재시도(40001·40P01), 새 id 부족(FV001, `invalid_state`).
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum PredecessorConflict {
    Shift(Box<TaskConflict>),
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
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = PredecessorConflict,
               description = "`shift_conflict`·`link_conflict`면 TaskConflict(최신본 포함), 기대 사용자 불일치면 ErrorBody(`account_changed`), 보관·동시 변경·id 부족이면 ErrorBody(`invalid_state`)")),
    tag = "tasks")]
pub async fn add_predecessor(
    State(state): State<AppState>,
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
        return shift_conflict(&state, user, id).await;
    }
    if outcome == "link_conflict" {
        drop(tx);
        return link_conflict(&state, user, id).await;
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
            return shift_conflict(&state, user, id).await;
        }
    }
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail).into_response())
}

/// 선행 업무 지우기(후속 업무 쓰기 권한). 관계를 지우면 제약이 줄 뿐이라 옮길 것이 없다.
#[utoipa::path(delete, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}", operation_id = "remove_predecessor",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "tasks")]
pub async fn remove_predecessor(
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_remove_task_dependency($1, $2, $3)")
        .bind(predecessor)
        .bind(id)
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::format_description::well_known::Iso8601;

    fn d(raw: &str) -> Date {
        Date::parse(raw, &Iso8601::DATE).unwrap()
    }

    fn span(start: Option<Date>, due: Option<Date>) -> Span {
        Span { start, due }
    }

    fn days(start: &str, due: &str) -> Span {
        span(Some(d(start)), Some(d(due)))
    }

    fn ids(n: u128) -> Vec<Uuid> {
        (1..=n).map(Uuid::from_u128).collect()
    }

    fn fs(p: Uuid, s: Uuid) -> Link {
        link(p, s, LinkKind::FinishToStart, 0)
    }

    fn link(predecessor: Uuid, successor: Uuid, kind: LinkKind, lag_days: i32) -> Link {
        Link {
            predecessor,
            successor,
            kind,
            lag_days,
        }
    }

    /// 날짜 옮기기 계획에서 (업무, 새 날짜)만.
    fn moves(
        spans: &HashMap<Uuid, Span>,
        links: &[Link],
        fixed: &HashSet<Uuid>,
        task: Uuid,
        next: Span,
    ) -> Vec<(Uuid, Span)> {
        plan_shifts(spans, links, fixed, Change::Move { task, next })
            .shifts
            .into_iter()
            .map(|p| (p.task, p.span))
            .collect()
    }

    #[test]
    fn pushes_successors_by_the_minimum_and_keeps_their_length() {
        let t = ids(4);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-05")),
            (t[1], days("2026-10-06", "2026-10-08")),
            (t[2], days("2026-10-20", "2026-10-21")),
            (t[3], span(None, Some(d("2026-10-09")))),
        ]);
        // 0 → 1 → 3, 0 → 2 (2는 충분히 뒤라 그대로).
        let links = [fs(t[0], t[1]), fs(t[1], t[3]), fs(t[0], t[2])];
        assert_eq!(
            moves(
                &spans,
                &links,
                &HashSet::new(),
                t[0],
                days("2026-10-01", "2026-10-10")
            ),
            vec![
                (t[1], days("2026-10-11", "2026-10-13")),
                (t[3], span(None, Some(d("2026-10-14")))),
            ]
        );
    }

    #[test]
    fn pulling_earlier_or_undated_tasks_proposes_nothing() {
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-05")))),
            (t[1], span(None, None)),
            (t[2], span(Some(d("2026-10-06")), None)),
        ]);
        let links = [fs(t[0], t[1]), fs(t[1], t[2])];
        let earlier = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: span(None, Some(d("2026-10-01"))),
            },
        );
        assert_eq!(earlier, Plan::default());
        // 날짜 없는 업무는 옮기지 않고 제약을 전하지 않는다. 더 늦은 날을 요구받으면 알린다(날짜를 지어내지 않는다).
        let later = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: span(None, Some(d("2026-12-01"))),
            },
        );
        assert_eq!(
            later,
            Plan {
                shifts: vec![],
                undated: vec![t[1]]
            }
        );
    }

    #[test]
    fn existing_overlaps_and_done_tasks_are_left_alone() {
        let t = ids(4);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-01")))),
            // 1은 0의 후속인데 전부터 0과 겹친다(0의 마감 10-01, 1의 시작 10-01).
            (t[1], days("2026-10-01", "2026-10-02")),
            (t[2], days("2026-10-03", "2026-10-03")),
            (t[3], days("2026-10-02", "2026-10-02")),
        ]);
        let links = [fs(t[0], t[1]), fs(t[1], t[2]), fs(t[0], t[3])];
        // 0을 앞당기면 아무것도 제안하지 않는다(전부터 겹친 1도 그대로).
        assert!(
            moves(
                &spans,
                &links,
                &HashSet::new(),
                t[0],
                span(None, Some(d("2026-09-20")))
            )
            .is_empty()
        );
        // 0을 이틀 미루면 1은 늘어난 이틀만큼, 3은 완료라 그대로.
        assert_eq!(
            moves(
                &spans,
                &links,
                &HashSet::from([t[3]]),
                t[0],
                span(None, Some(d("2026-10-03")))
            ),
            vec![
                (t[1], days("2026-10-03", "2026-10-04")),
                (t[2], days("2026-10-05", "2026-10-05")),
            ]
        );
    }

    #[test]
    fn a_task_with_two_moved_predecessors_takes_the_later_one() {
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-01")))),
            (t[1], span(None, Some(d("2026-10-02")))),
            (t[2], days("2026-10-03", "2026-10-03")),
        ]);
        // 0 → 1 → 2, 0 → 2.
        let links = [fs(t[0], t[1]), fs(t[1], t[2]), fs(t[0], t[2])];
        let plan = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: span(None, Some(d("2026-10-04"))),
            },
        );
        assert_eq!(
            plan.shifts
                .iter()
                .map(|p| (p.task, p.span))
                .collect::<Vec<_>>(),
            vec![
                (t[1], span(None, Some(d("2026-10-05")))),
                (t[2], days("2026-10-06", "2026-10-06")),
            ]
        );
        // 2를 민 것은 1 → 2(1이 10-05에 끝나 10-06 시작). 0 → 2는 10-05만 요구해 원인이 아니다.
        assert_eq!(plan.shifts[1].causes, vec![fs(t[1], t[2])]);
    }

    /// 선행 A(10-05 ~ 10-09), 후속 B(10-01 ~ 10-03, 3일)를 종류·지연일마다 이어 만들 때 B의 새 날짜.
    fn linked(kind: LinkKind, lag: i32) -> Vec<(Uuid, Span)> {
        let t = ids(2);
        let spans = HashMap::from([
            (t[0], days("2026-10-05", "2026-10-09")),
            (t[1], days("2026-10-01", "2026-10-03")),
        ]);
        let new = link(t[0], t[1], kind, lag);
        let plan = plan_shifts(
            &spans,
            &[],
            &HashSet::new(),
            Change::Link { old: None, new },
        );
        assert!(plan.shifts.iter().all(|p| p.causes == vec![new]));
        plan.shifts.into_iter().map(|p| (p.task, p.span)).collect()
    }

    #[test]
    fn each_kind_anchors_the_right_ends_with_and_without_lag() {
        let b = ids(2)[1];
        // FS: B 시작 ≥ A 마감 + 1 + 지연.
        assert_eq!(
            linked(LinkKind::FinishToStart, 0),
            vec![(b, days("2026-10-10", "2026-10-12"))]
        );
        assert_eq!(
            linked(LinkKind::FinishToStart, 2),
            vec![(b, days("2026-10-12", "2026-10-14"))]
        );
        // SS: B 시작 ≥ A 시작 + 지연.
        assert_eq!(
            linked(LinkKind::StartToStart, 0),
            vec![(b, days("2026-10-05", "2026-10-07"))]
        );
        assert_eq!(
            linked(LinkKind::StartToStart, 3),
            vec![(b, days("2026-10-08", "2026-10-10"))]
        );
        // FF: B 마감 ≥ A 마감 + 지연.
        assert_eq!(
            linked(LinkKind::FinishToFinish, 0),
            vec![(b, days("2026-10-07", "2026-10-09"))]
        );
        assert_eq!(
            linked(LinkKind::FinishToFinish, 1),
            vec![(b, days("2026-10-08", "2026-10-10"))]
        );
        // SF: B 마감 ≥ A 시작 - 1 + 지연. 지연 0이면 A 시작 전날(10-04)에 끝나도 된다.
        assert_eq!(
            linked(LinkKind::StartToFinish, 0),
            vec![(b, days("2026-10-02", "2026-10-04"))]
        );
        assert_eq!(
            linked(LinkKind::StartToFinish, 1),
            vec![(b, days("2026-10-03", "2026-10-05"))]
        );
    }

    #[test]
    fn a_link_that_is_already_satisfied_moves_nothing() {
        let t = ids(2);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-03")),
            (t[1], days("2026-10-10", "2026-10-12")),
        ]);
        for kind in [
            LinkKind::FinishToStart,
            LinkKind::StartToStart,
            LinkKind::FinishToFinish,
            LinkKind::StartToFinish,
        ] {
            let new = link(t[0], t[1], kind, 2);
            assert_eq!(
                plan_shifts(
                    &spans,
                    &[],
                    &HashSet::new(),
                    Change::Link { old: None, new }
                ),
                Plan::default(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn changing_a_link_pushes_only_what_the_change_adds() {
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-05")),
            (t[1], days("2026-10-06", "2026-10-07")),
            (t[2], days("2026-10-08", "2026-10-08")),
        ]);
        let old = fs(t[0], t[1]);
        let links = [old, fs(t[1], t[2])];
        // FS·0 → FS·3: 1은 사흘, 2는 그 뒤로 사흘.
        let lag3 = link(t[0], t[1], LinkKind::FinishToStart, 3);
        let plan = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Link {
                old: Some(old),
                new: lag3,
            },
        );
        assert_eq!(
            plan.shifts
                .iter()
                .map(|p| (p.task, p.span, p.causes.clone()))
                .collect::<Vec<_>>(),
            vec![
                (t[1], days("2026-10-09", "2026-10-10"), vec![lag3]),
                (t[2], days("2026-10-11", "2026-10-11"), vec![fs(t[1], t[2])]),
            ]
        );
        // FS → SS는 요구가 줄어 아무것도 옮기지 않는다.
        let ss = link(t[0], t[1], LinkKind::StartToStart, 0);
        assert_eq!(
            plan_shifts(
                &spans,
                &links,
                &HashSet::new(),
                Change::Link {
                    old: Some(old),
                    new: ss
                }
            ),
            Plan::default()
        );
    }

    #[test]
    fn a_join_waits_for_every_predecessor_across_kinds() {
        // 0 →(FS) 2, 1 →(FF·2) 2. 0을 미루면 2는 두 관계를 모두 채워야 한다.
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-02")),
            (t[1], days("2026-10-01", "2026-10-10")),
            (t[2], days("2026-10-11", "2026-10-12")),
        ]);
        let ff = link(t[1], t[2], LinkKind::FinishToFinish, 2);
        let links = [fs(t[0], t[2]), ff];
        // 0이 10-12까지면 FS가 2를 10-13 시작으로 민다. FF·2(10-12 마감)는 이미 채운다.
        let plan = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: days("2026-10-11", "2026-10-12"),
            },
        );
        assert_eq!(
            plan.shifts
                .iter()
                .map(|p| (p.task, p.span))
                .collect::<Vec<_>>(),
            vec![(t[2], days("2026-10-13", "2026-10-14"))]
        );
        // 1을 미루면 FF·2만 닿는다: 1이 10-15에 끝나면 2는 10-17에 끝나야 한다.
        let plan = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[1],
                next: days("2026-10-06", "2026-10-15"),
            },
        );
        assert_eq!(
            plan.shifts
                .iter()
                .map(|p| (p.task, p.span, p.causes.clone()))
                .collect::<Vec<_>>(),
            vec![(t[2], days("2026-10-16", "2026-10-17"), vec![ff])]
        );
    }

    #[test]
    fn forks_and_mixed_chains_follow_each_kind() {
        // 0 →(SS·1) 1 →(SF) 2, 0 →(FF) 3. 0을 사흘 미룬다.
        let t = ids(4);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-04")),
            (t[1], days("2026-10-02", "2026-10-03")),
            (t[2], days("2026-10-01", "2026-10-01")),
            (t[3], days("2026-10-03", "2026-10-04")),
        ]);
        let links = [
            link(t[0], t[1], LinkKind::StartToStart, 1),
            link(t[1], t[2], LinkKind::StartToFinish, 0),
            link(t[0], t[3], LinkKind::FinishToFinish, 0),
        ];
        assert_eq!(
            moves(
                &spans,
                &links,
                &HashSet::new(),
                t[0],
                days("2026-10-04", "2026-10-07")
            ),
            // 위상 순서: 0 다음 1·3(id 순), 1 다음 2.
            vec![
                // SS·1: 0이 10-04 시작 → 1은 10-05 시작.
                (t[1], days("2026-10-05", "2026-10-06")),
                // FF: 0이 10-07 마감 → 3은 10-07 마감.
                (t[3], days("2026-10-06", "2026-10-07")),
                // SF: 1이 10-05 시작 → 2는 10-04 이후 마감. 전에는 10-01 요구(이미 채움)라 사흘.
                (t[2], days("2026-10-04", "2026-10-04")),
            ]
        );
    }

    #[test]
    fn milestones_and_one_sided_dates_use_the_date_they_have() {
        // 마일스톤(마감만) → 시작만 있는 업무.
        let t = ids(2);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-05")))),
            (t[1], span(Some(d("2026-10-03")), None)),
        ]);
        let new = link(t[0], t[1], LinkKind::StartToStart, 0);
        assert_eq!(
            plan_shifts(
                &spans,
                &[],
                &HashSet::new(),
                Change::Link { old: None, new }
            )
            .shifts
            .into_iter()
            .map(|p| (p.task, p.span))
            .collect::<Vec<_>>(),
            vec![(t[1], span(Some(d("2026-10-05")), None))]
        );
    }

    #[test]
    fn a_new_link_to_an_undated_successor_is_reported_not_invented() {
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], days("2026-10-01", "2026-10-05")),
            (t[1], span(None, None)),
            (t[2], days("2026-10-02", "2026-10-02")),
        ]);
        let links = [fs(t[1], t[2])];
        let plan = plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Link {
                old: None,
                new: fs(t[0], t[1]),
            },
        );
        assert_eq!(
            plan,
            Plan {
                shifts: vec![],
                undated: vec![t[1]]
            }
        );
    }

    /// 0에서 갈라지는 후속 `n`개를 모두 하루 미는 계획.
    fn fork_plan(n: u128) -> Plan {
        let t = ids(n + 1);
        let mut spans = HashMap::from([(t[0], days("2026-10-01", "2026-10-05"))]);
        let links: Vec<Link> = t[1..]
            .iter()
            .map(|&s| {
                spans.insert(s, days("2026-10-06", "2026-10-06"));
                fs(t[0], s)
            })
            .collect();
        plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: days("2026-10-01", "2026-10-06"),
            },
        )
    }

    #[test]
    fn the_shift_limit_matches_approval_at_499_500_and_501() {
        assert_eq!(fork_plan(499).shifts.len(), 499);
        assert_eq!(plan_block(&fork_plan(499)), None);
        assert_eq!(plan_block(&fork_plan(500)), None);
        let over = fork_plan(501);
        assert_eq!(over.shifts.len(), 501, "the plan is not cut short");
        assert_eq!(
            plan_block(&over),
            Some((PlanBlockReason::TooManyShifts, None))
        );
    }

    #[test]
    fn proposed_dates_outside_the_saveable_years_block_the_plan() {
        let t = ids(2);
        let spans = HashMap::from([
            (t[0], days("2200-12-20", "2200-12-29")),
            (t[1], days("2200-12-30", "2200-12-31")),
        ]);
        let plan = plan_shifts(
            &spans,
            &[fs(t[0], t[1])],
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: days("2200-12-20", "2200-12-30"),
            },
        );
        // 기간을 자르지 않고 그대로 제안하되 저장할 수 없다고 알린다.
        assert_eq!(plan.shifts[0].span, days("2200-12-31", "2201-01-01"));
        assert_eq!(
            plan_block(&plan),
            Some((
                PlanBlockReason::DateOutOfRange,
                Some((t[1], d("2201-01-01")))
            ))
        );
        // 범위 끝(2200-12-31)에 딱 맞게 밀리면 저장할 수 있다.
        let spans = HashMap::from([
            (t[0], days("2200-12-20", "2200-12-28")),
            (t[1], days("2200-12-29", "2200-12-30")),
        ]);
        let edge = plan_shifts(
            &spans,
            &[fs(t[0], t[1])],
            &HashSet::new(),
            Change::Move {
                task: t[0],
                next: days("2200-12-20", "2200-12-29"),
            },
        );
        assert_eq!(edge.shifts[0].span, days("2200-12-30", "2200-12-31"));
        assert_eq!(plan_block(&edge), None);
    }

    #[test]
    fn the_approved_list_must_match_the_plan_exactly() {
        let t = ids(2);
        let plan = Plan {
            shifts: vec![Planned {
                task: t[0],
                span: days("2026-10-02", "2026-10-03"),
                causes: vec![],
            }],
            undated: vec![],
        };
        assert!(same_shifts(
            &plan,
            &[(t[0], days("2026-10-02", "2026-10-03"))]
        ));
        assert!(!same_shifts(
            &plan,
            &[(t[0], days("2026-10-03", "2026-10-04"))]
        ));
        assert!(!same_shifts(&plan, &[]));
        assert!(!same_shifts(
            &plan,
            &[
                (t[0], days("2026-10-02", "2026-10-03")),
                (t[1], days("2026-10-02", "2026-10-03"))
            ]
        ));
    }

    #[test]
    fn link_kinds_round_trip_their_codes() {
        for kind in [
            LinkKind::FinishToStart,
            LinkKind::StartToStart,
            LinkKind::FinishToFinish,
            LinkKind::StartToFinish,
        ] {
            assert_eq!(LinkKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
        assert_eq!(LinkKind::parse("fs"), None);
    }
}
