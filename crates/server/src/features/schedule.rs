//! 마일스톤·선행 관계·후속 재배치 미리보기 (D-31, D-33).
//!
//! 선행 관계는 같은 항목 안의 "끝나야 시작" 하나다. 날짜를 바꾸기 전에 미리보기로 영향받는 후속 업무와 제안
//! 날짜를 계산해 보여 주고, 사용자가 승인한 목록만 업무 고치기(`successor_shifts`)와 같은 트랜잭션에서 옮긴다.
//! 제안은 선행이 바뀐 만큼만 뒤로 미는 최소 이동이며 기간(시작~마감)을 유지한다. 앞당긴 경우에는 제안하지 않는다.
//! 처리는 `migrations/0012_task_schedule.sql`의 정의자 함수가 한다.

use std::collections::{HashMap, HashSet, VecDeque};

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use time::{Date, Duration};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::tasks::{TaskDetail, format_date, load_detail, parse_date};
use crate::http::AppState;

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

/// `origin`의 날짜를 `next`로 바꿀 때 뒤로 밀어야 하는 후속 업무와 새 날짜(위상 순서).
///
/// 후속 업무는 바뀐 선행(직접·간접)의 마감 다음 날 이후에 시작해야 한다. 이번에 옮겨진 선행만 보며, 그 선행이
/// 원래 날짜였을 때보다 더 요구하는 만큼만 민다(전부터 겹쳐 있던 만큼은 제안하지 않는다). 날짜가 없는 업무와
/// `fixed`(완료한 업무)는 옮기지 않고 그 뒤로 제약을 전하지 않는다.
#[must_use]
pub fn plan_shifts(
    spans: &HashMap<Uuid, Span>,
    edges: &[(Uuid, Uuid)],
    fixed: &HashSet<Uuid>,
    origin: Uuid,
    next: Span,
) -> Vec<(Uuid, Span)> {
    let mut successors: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for &(pred, succ) in edges {
        successors.entry(pred).or_default().push(succ);
    }
    // origin에서 닿는 업무.
    let mut affected: HashSet<Uuid> = HashSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some(id) = queue.pop_front() {
        for &succ in successors.get(&id).map_or(&[][..], Vec::as_slice) {
            if affected.insert(succ) {
                queue.push_back(succ);
            }
        }
    }
    // 영향 범위 안의 위상 순서(Kahn). DB가 순환을 막지만 남는 것이 있으면 건너뛴다.
    let inner: Vec<(Uuid, Uuid)> = edges
        .iter()
        .copied()
        .filter(|(p, s)| affected.contains(p) && affected.contains(s))
        .collect();
    let mut indegree: HashMap<Uuid, usize> = affected.iter().map(|&id| (id, 0)).collect();
    for &(_, s) in &inner {
        *indegree.entry(s).or_default() += 1;
    }
    let mut ready: VecDeque<Uuid> = indegree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut sorted_ready: Vec<Uuid> = ready.drain(..).collect();
    sorted_ready.sort_unstable();
    ready.extend(sorted_ready);
    let mut current: HashMap<Uuid, Span> = spans.clone();
    current.insert(origin, next);
    let mut moved_ids: HashSet<Uuid> = HashSet::from([origin]);
    let mut out = Vec::new();
    while let Some(id) = ready.pop_front() {
        if id != origin
            && !fixed.contains(&id)
            && let Some(span) = current.get(&id).copied()
            && let Some(begin) = span.begin()
        {
            // 옮겨진 선행이 요구하는 시작일(새 날짜 기준, 원래 날짜 기준).
            let required_by = |map: &HashMap<Uuid, Span>| {
                inner
                    .iter()
                    .filter(|&&(p, s)| s == id && moved_ids.contains(&p))
                    .filter_map(|&(p, _)| map.get(&p).and_then(|sp| sp.end()))
                    .filter_map(Date::next_day)
                    .max()
            };
            let base = required_by(spans).map_or(begin, |old| old.max(begin));
            if let Some(required) = required_by(&current)
                && base < required
                && let Some(moved) = span.shifted((required - base).whole_days())
            {
                moved_ids.insert(id);
                current.insert(id, moved);
                out.push((id, moved));
            }
        }
        let mut freed = Vec::new();
        for &(p, s) in &inner {
            if p == id
                && let Some(d) = indegree.get_mut(&s)
            {
                *d -= 1;
                if *d == 0 {
                    freed.push(s);
                }
            }
        }
        freed.sort_unstable();
        ready.extend(freed);
    }
    out
}

/// 미리보기 요청: 바꾸려는 날짜.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SchedulePreviewInput {
    pub start_date: Option<String>,
    pub due_date: Option<String>,
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
}

/// 후속 재배치 미리보기(읽기). 아무것도 바꾸지 않는다. 업무를 볼 수 없으면 404.
#[utoipa::path(post, path = "/api/tasks/{task_id}/schedule-preview", operation_id = "preview_task_schedule",
    params(("task_id" = String, Path, format = Uuid)), request_body = SchedulePreviewInput,
    responses((status = 200, body = [ShiftProposal]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn preview(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SchedulePreviewInput>,
) -> Result<Json<Vec<ShiftProposal>>, AppError> {
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
    let mut tx = state.db.begin_as(user).await?;
    let item: Option<Uuid> = sqlx::query_scalar("SELECT item_id FROM tasks WHERE id = $1")
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    let item = item.ok_or(AppError::NotFound)?;
    type Row = (
        Uuid,
        Option<String>,
        String,
        i32,
        Option<Date>,
        Option<Date>,
        bool,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, app_task_reference(id), title, version, start_date, due_date, status = 'done'
         FROM tasks WHERE item_id = $1",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let edges: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT predecessor_id, successor_id FROM task_dependencies WHERE item_id = $1",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let spans: HashMap<Uuid, Span> = rows
        .iter()
        .map(|r| {
            (
                r.0,
                Span {
                    start: r.4,
                    due: r.5,
                },
            )
        })
        .collect();
    // 완료한 업무는 옮기자고 하지 않는다.
    let done: HashSet<Uuid> = rows.iter().filter(|r| r.6).map(|r| r.0).collect();
    let by_id: HashMap<Uuid, &Row> = rows.iter().map(|r| (r.0, r)).collect();
    let proposals = plan_shifts(&spans, &edges, &done, id, next)
        .into_iter()
        .filter_map(|(task, moved)| {
            let row = by_id.get(&task)?;
            Some(ShiftProposal {
                task_id: task,
                reference: row.1.clone(),
                title: row.2.clone(),
                base_version: row.3,
                from_start_date: format_date(row.4),
                from_due_date: format_date(row.5),
                start_date: format_date(moved.start),
                due_date: format_date(moved.due),
            })
        })
        .collect();
    Ok(Json(proposals))
}

/// 선행 업무 추가(후속 업무 쓰기 권한). 같은 항목만, 순환이면 400. 이미 있으면 그대로.
#[utoipa::path(put, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}", operation_id = "add_predecessor",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid)),
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn add_predecessor(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_add_task_dependency($1, $2, $3)")
        .bind(predecessor)
        .bind(id)
        .bind(crate::db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 선행 업무 지우기(후속 업무 쓰기 권한).
#[utoipa::path(delete, path = "/api/tasks/{task_id}/predecessors/{predecessor_id}", operation_id = "remove_predecessor",
    params(("task_id" = String, Path, format = Uuid), ("predecessor_id" = String, Path, format = Uuid)),
    responses((status = 200, body = TaskDetail), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn remove_predecessor(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, predecessor)): Path<(Uuid, Uuid)>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_remove_task_dependency($1, $2, $3)")
        .bind(predecessor)
        .bind(id)
        .bind(crate::db::new_ids(1))
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

    fn ids(n: u128) -> Vec<Uuid> {
        (1..=n).map(Uuid::from_u128).collect()
    }

    #[test]
    fn pushes_successors_by_the_minimum_and_keeps_their_length() {
        let t = ids(4);
        let spans = HashMap::from([
            (t[0], span(Some(d("2026-10-01")), Some(d("2026-10-05")))),
            (t[1], span(Some(d("2026-10-06")), Some(d("2026-10-08")))),
            (t[2], span(Some(d("2026-10-20")), Some(d("2026-10-21")))),
            (t[3], span(None, Some(d("2026-10-09")))),
        ]);
        // 0 → 1 → 3, 0 → 2 (2는 충분히 뒤라 그대로).
        let edges = [(t[0], t[1]), (t[1], t[3]), (t[0], t[2])];
        let plan = plan_shifts(
            &spans,
            &edges,
            &HashSet::new(),
            t[0],
            span(Some(d("2026-10-01")), Some(d("2026-10-10"))),
        );
        assert_eq!(
            plan,
            vec![
                (t[1], span(Some(d("2026-10-11")), Some(d("2026-10-13")))),
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
        let edges = [(t[0], t[1]), (t[1], t[2])];
        assert!(
            plan_shifts(
                &spans,
                &edges,
                &HashSet::new(),
                t[0],
                span(None, Some(d("2026-10-01")))
            )
            .is_empty()
        );
        // 날짜 없는 업무는 제약을 전하지 않는다.
        assert!(
            plan_shifts(
                &spans,
                &edges,
                &HashSet::new(),
                t[0],
                span(None, Some(d("2026-12-01")))
            )
            .is_empty()
        );
    }

    #[test]
    fn existing_overlaps_and_done_tasks_are_left_alone() {
        let t = ids(4);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-01")))),
            // 1은 0의 후속인데 전부터 0과 겹친다(0의 마감 10-01, 1의 시작 10-01).
            (t[1], span(Some(d("2026-10-01")), Some(d("2026-10-02")))),
            (t[2], span(Some(d("2026-10-03")), Some(d("2026-10-03")))),
            (t[3], span(Some(d("2026-10-02")), Some(d("2026-10-02")))),
        ]);
        let edges = [(t[0], t[1]), (t[1], t[2]), (t[0], t[3])];
        // 0을 앞당기면 아무것도 제안하지 않는다(전부터 겹친 1도 그대로).
        assert!(
            plan_shifts(
                &spans,
                &edges,
                &HashSet::new(),
                t[0],
                span(None, Some(d("2026-09-20")))
            )
            .is_empty()
        );
        // 0을 이틀 미루면 1은 늘어난 이틀만큼, 3은 완료라 그대로.
        let plan = plan_shifts(
            &spans,
            &edges,
            &HashSet::from([t[3]]),
            t[0],
            span(None, Some(d("2026-10-03"))),
        );
        assert_eq!(
            plan,
            vec![
                (t[1], span(Some(d("2026-10-03")), Some(d("2026-10-04")))),
                (t[2], span(Some(d("2026-10-05")), Some(d("2026-10-05")))),
            ]
        );
    }

    #[test]
    fn a_task_with_two_moved_predecessors_takes_the_later_one() {
        let t = ids(3);
        let spans = HashMap::from([
            (t[0], span(None, Some(d("2026-10-01")))),
            (t[1], span(None, Some(d("2026-10-02")))),
            (t[2], span(Some(d("2026-10-03")), Some(d("2026-10-03")))),
        ]);
        // 0 → 1 → 2, 0 → 2.
        let edges = [(t[0], t[1]), (t[1], t[2]), (t[0], t[2])];
        let plan = plan_shifts(
            &spans,
            &edges,
            &HashSet::new(),
            t[0],
            span(None, Some(d("2026-10-04"))),
        );
        assert_eq!(
            plan,
            vec![
                (t[1], span(None, Some(d("2026-10-05")))),
                (t[2], span(Some(d("2026-10-06")), Some(d("2026-10-06")))),
            ]
        );
    }
}
