//! 재배치 계획 계산(쓰기·DB 없음). 선행 관계의 종류·지연일로 후속 업무가 더 요구받는 만큼만 뒤로 민다(D-91).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use time::{Date, Duration};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::tasks::MAX_SHIFTS;
use fvoci_core::validate::DATE_YEARS;

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
    pub(crate) fn pair(self) -> (Uuid, Uuid) {
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
    // 바꾼 뒤 관계.
    let after: Vec<Link> = match changed {
        None => links.to_vec(),
        Some(new) => links
            .iter()
            .copied()
            .filter(|l| l.pair() != new.pair())
            .chain([new])
            .collect(),
    };
    let order = affected_order(&after, start);
    let mut walk = Walk {
        before: spans,
        after: &after,
        change,
        changed: changed.map(Link::pair),
        current: spans.clone(),
        moved: HashSet::new(),
        plan: Plan::default(),
    };
    if let Change::Move { task, next } = change {
        walk.current.insert(task, next);
        walk.moved.insert(task);
    }
    for id in order {
        if !walk.moved.contains(&id)
            && !fixed.contains(&id)
            && let Some(span) = walk.current.get(&id).copied()
        {
            walk.visit(id, span);
        }
    }
    walk.plan
}

/// `start`에서 관계를 따라 닿는 업무의 위상 순서(Kahn, 같은 차례는 id 순). DB가 순환을 막지만 남는 것이 있으면
/// 그 업무는 순서에 넣지 않는다.
fn affected_order(after: &[Link], start: Uuid) -> Vec<Uuid> {
    let mut successors: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for l in after {
        successors
            .entry(l.predecessor)
            .or_default()
            .push(l.successor);
    }
    let mut affected: HashSet<Uuid> = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        for &succ in successors.get(&id).map_or(&[][..], Vec::as_slice) {
            if affected.insert(succ) {
                queue.push_back(succ);
            }
        }
    }
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
    let mut order = Vec::with_capacity(affected.len());
    while let Some(id) = ready.pop_front() {
        order.push(id);
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
    order
}

/// 위상 순서로 업무를 하나씩 보며 계획을 쌓는 상태.
struct Walk<'a> {
    /// 바꾸기 전 날짜.
    before: &'a HashMap<Uuid, Span>,
    /// 바꾼 뒤 관계.
    after: &'a [Link],
    change: Change,
    /// 이번에 고친 관계 쌍(날짜 옮기기면 없음).
    changed: Option<(Uuid, Uuid)>,
    /// 지금까지 옮긴 뒤의 날짜.
    current: HashMap<Uuid, Span>,
    moved: HashSet<Uuid>,
    plan: Plan,
}

impl Walk<'_> {
    /// 관계의 바꾸기 전 모습(새로 만든 관계는 없음).
    fn before_of(&self, l: Link) -> Option<Link> {
        match self.change {
            Change::Link { old, new } if l.pair() == new.pair() => old,
            _ => Some(l),
        }
    }

    fn visit(&mut self, id: Uuid, span: Span) {
        // 이번 변경이 닿은 관계: 선행이 옮겨졌거나 관계 자체를 고쳤다.
        let touched: Vec<Link> = self
            .after
            .iter()
            .copied()
            .filter(|l| {
                l.successor == id
                    && (self.moved.contains(&l.predecessor) || Some(l.pair()) == self.changed)
            })
            .collect();
        if span.begin().is_none() {
            // 날짜가 없으면 옮기지 않는다. 바뀐 관계가 전보다 늦은 날을 요구하면 알린다.
            if touched.iter().any(|&l| self.asks_later(l)) {
                self.plan.undated.push(id);
            }
        } else {
            self.push_back(id, span, &touched);
        }
    }

    /// 날짜 없는 후속에 이 관계가 전보다 늦은 날을 요구하는지.
    fn asks_later(&self, l: Link) -> bool {
        let now = self.current.get(&l.predecessor).and_then(|&p| l.bound(p));
        let was = self
            .before_of(l)
            .and_then(|b| self.before.get(&l.predecessor).and_then(|&p| b.bound(p)));
        now.is_some() && (Some(l.pair()) == self.changed || now > was)
    }

    /// 관계마다 더 미뤄야 하는 날(지금 요구 - 전부터 요구하던 것, 전에 채웠으면 0) 가운데 가장 큰 만큼 민다.
    fn push_back(&mut self, id: Uuid, span: Span, touched: &[Link]) {
        let wants: Vec<(Link, i64)> = touched
            .iter()
            .filter_map(|&l| {
                let now = l.need(*self.current.get(&l.predecessor)?, span)?;
                let was = self
                    .before_of(l)
                    .and_then(|b| b.need(*self.before.get(&l.predecessor)?, span))
                    .map_or(0, |w| w.max(0));
                Some((l, now - was))
            })
            .collect();
        if let Some(days) = wants.iter().map(|&(_, d)| d).max().filter(|&d| d > 0)
            && let Some(next) = span.shifted(days)
        {
            self.moved.insert(id);
            self.current.insert(id, next);
            self.plan.shifts.push(Planned {
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

/// 승인 목록이 계획과 같은지(업무·날짜 집합). 승인은 미리보기 전체를 한꺼번에 하므로 같아야 한다.
pub(super) fn same_shifts(plan: &Plan, approved: &[(Uuid, Span)]) -> bool {
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

/// 저장할 수 없는 계획의 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanBlockReason {
    /// 옮길 후속 업무가 한 번에 옮길 수 있는 수(`max_shifts`)보다 많다.
    TooManyShifts,
    /// 제안 날짜가 저장할 수 있는 범위(1900~2200년) 밖이다.
    DateOutOfRange,
}

/// 계획을 저장할 수 있는지: 건수 한도(승인 저장과 같은 `MAX_SHIFTS`)를 먼저, 그다음 제안 날짜의 범위(저장과 같은
/// `DATE_YEARS`)를 본다. 막히면 (까닭, 범위 밖 날짜를 받은 업무와 그 날짜).
pub(super) fn plan_block(plan: &Plan) -> Option<(PlanBlockReason, Option<(Uuid, Date)>)> {
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

/// `from`에서 관계를 따라 `to`에 닿는지.
pub(super) fn reaches(links: &[Link], from: Uuid, to: Uuid) -> bool {
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

#[cfg(test)]
mod tests;
