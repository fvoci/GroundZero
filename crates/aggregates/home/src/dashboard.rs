//! 홈 (D-19): Up Next(기한 초과·오늘·다가오는 업무), 최근 문서·최근 작업, 진행 중인 항목과 의미에 맞는 진행률,
//! 고정·즐겨찾기. 모두 볼 수 있는 것만(RLS) 나온다. 주간 달력은 달력 API, 실행 중인 스톱워치는 타이머 API,
//! 알림은 알림 API를 그대로 쓴다. 고정·즐겨찾기는 `crates/db/migrations/202610280008_activity.sql`의 `app_set_mark`가 처리한다.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::HomeState;
use crate::queries::{self, TaskRow};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, OrganizationId, WorkspaceId};
use fvoci_core::module::ModuleKind;
use fvoci_core::validate::format_date;
use fvoci_http::organization::OrgScope;

/// Up Next 구분.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DueBucket {
    Overdue,
    Today,
    Upcoming,
}

/// 홈의 업무 한 줄.
#[derive(Debug, Serialize, ToSchema)]
pub struct HomeTask {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub workspace_id: WorkspaceId,
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub due_date: Option<String>,
    /// Up Next에서만.
    pub bucket: Option<DueBucket>,
}

/// 홈의 문서 한 줄.
#[derive(Debug, Serialize, ToSchema)]
pub struct HomeDocument {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub title: String,
    pub reference: Option<String>,
    /// 개인 노트면 없다.
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 진행 중인 항목과 진행률. 진행률은 활동의 의미대로 계산하며 서로 다른 종류를 합산하지 않는다(D-19). 지금은
/// 업무 완료 비율(완료한 업무 수 / 업무 수)이며 강의 이수율·공부 목표 달성률은 각 모듈 조각에서 더한다.
#[derive(Debug, Serialize, ToSchema)]
pub struct HomeItem {
    pub id: ItemId,
    pub workspace_id: WorkspaceId,
    pub module_kind: ModuleKind,
    pub reference: Option<String>,
    pub title: String,
    pub tasks_done: i64,
    pub tasks_total: i64,
    /// 이 항목에 업무·일정을 더할 수 있는지(빠른 추가).
    pub can_write: bool,
}

/// 고정·즐겨찾기 대상.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkTarget {
    Item,
    Document,
    Task,
}

impl MarkTarget {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
            _ => None,
        }
    }
}

/// 표시한 대상(지금 볼 수 있는 것만).
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkedTarget {
    pub kind: MarkTarget,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub label: String,
    pub workspace_id: Option<WorkspaceId>,
    /// 업무·항목 문서면 그 항목.
    pub item_id: Option<ItemId>,
}

/// Up Next 묶음마다의 전체 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpNextCounts {
    pub overdue: i64,
    pub today: i64,
    pub upcoming: i64,
}

/// Up Next가 묶음마다 보이는 최대 수. 묶음마다 따로 세어, 밀린 업무가 많아도 오늘·다가오는 일이 빠지지 않는다.
pub const UP_NEXT_PER_BUCKET: i64 = 50;

/// 홈.
#[derive(Debug, Serialize, ToSchema)]
pub struct Home {
    /// 보는 사람 시간대의 오늘.
    pub today: String,
    /// 묶음(기한 지남·오늘·다가오는 일)마다 마감이 이른 것부터 50개까지(`UP_NEXT_PER_BUCKET`).
    pub up_next: Vec<HomeTask>,
    /// 묶음마다의 전체 수. `up_next`보다 많으면 잘린 것이다.
    pub up_next_counts: UpNextCounts,
    pub recent_tasks: Vec<HomeTask>,
    pub recent_documents: Vec<HomeDocument>,
    pub active_items: Vec<HomeItem>,
    pub pins: Vec<MarkedTarget>,
    pub favorites: Vec<MarkedTarget>,
}

/// 보는 사람 시간대.
#[derive(Debug, Deserialize, IntoParams)]
pub struct HomeQuery {
    pub tz: Option<String>,
}

fn to_task(row: TaskRow, today: Option<Date>) -> Result<HomeTask, AppError> {
    let (id, item_id, workspace_id, reference, title, status, status_category, due) = row;
    let bucket = match (today, due) {
        (Some(today), Some(due)) if due < today => Some(DueBucket::Overdue),
        (Some(today), Some(due)) if due == today => Some(DueBucket::Today),
        (Some(_), Some(_)) => Some(DueBucket::Upcoming),
        _ => None,
    };
    Ok(HomeTask {
        id,
        item_id,
        workspace_id,
        reference,
        title,
        status,
        status_category,
        due_date: format_date(due),
        bucket,
    })
}

async fn marks(
    tx: &mut fvoci_db::UserTx,
    organization: OrganizationId,
) -> Result<(Vec<MarkedTarget>, Vec<MarkedTarget>), AppError> {
    type MarkRow = (
        String,
        String,
        Uuid,
        Option<String>,
        Option<WorkspaceId>,
        Option<ItemId>,
    );
    let rows: Vec<MarkRow> = sqlx::query_as(
        "SELECT m.mark, m.target_kind, m.target_id, app_time_target_label(m.target_kind, m.target_id),
                    coalesce(t.workspace_id, d.workspace_id, i.workspace_id),
                    coalesce(t.item_id, d.item_id)
             FROM user_marks m
             LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
             LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
             LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
             WHERE m.mark IN ('pin', 'favorite')
               AND (coalesce(t.workspace_id, d.workspace_id, i.workspace_id) IN (SELECT app_my_workspace_ids($1))
                    OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id()))
             ORDER BY m.created_at, m.target_id",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;
    let mut pins = Vec::new();
    let mut favorites = Vec::new();
    // 한 문장 스냅숏으로 읽고, 각 목록의 created_at, target_id 순서를 유지한다.
    for (mark, kind, id, label, workspace_id, item_id) in rows {
        let (Some(kind), Some(label)) = (MarkTarget::parse(&kind), label) else {
            // 볼 수 없게 된 대상은 나오지 않는다.
            continue;
        };
        let target = MarkedTarget {
            kind,
            id,
            label,
            workspace_id,
            item_id,
        };
        match mark.as_str() {
            "pin" => pins.push(target),
            "favorite" => favorites.push(target),
            _ => {}
        }
    }
    Ok((pins, favorites))
}

/// 홈(로그인한 사람). 현재 조직에서 볼 수 있는 것만 모은다.
#[utoipa::path(get, path = "/api/me/home", operation_id = "home", params(HomeQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = Home), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "home")]
pub async fn home(
    State(state): State<HomeState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<HomeQuery>,
) -> Result<Json<Home>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let today = queries::today(&mut tx, query.tz).await?;
    // 묶음마다 따로 자른다. 한 번에 50개로 자르면 밀린 업무가 많을 때 오늘·다가오는 일이 빠졌다(감사 W23).
    let up_next = queries::up_next(&mut tx, today, organization).await?;
    let (overdue, due_today, upcoming) = queries::counts(&mut tx, today, organization).await?;
    let recent_tasks = queries::recent_tasks(&mut tx, organization).await?;
    let recent_documents = queries::documents(&mut tx, organization).await?;
    let active_items = queries::active_items(&mut tx, organization).await?;
    let (pins, favorites) = marks(&mut tx, organization).await?;
    tx.commit().await?;
    Ok(Json(Home {
        today: format_date(Some(today)).unwrap_or_default(),
        up_next: up_next
            .into_iter()
            .map(|r| to_task(r, Some(today)))
            .collect::<Result<_, _>>()?,
        up_next_counts: UpNextCounts {
            overdue,
            today: due_today,
            upcoming,
        },
        recent_tasks: recent_tasks
            .into_iter()
            .map(|r| to_task(r, None))
            .collect::<Result<_, _>>()?,
        recent_documents: recent_documents
            .into_iter()
            .map(
                |(id, title, reference, workspace_id, item_id, updated_at)| HomeDocument {
                    id,
                    title,
                    reference,
                    workspace_id,
                    item_id,
                    updated_at,
                },
            )
            .collect(),
        active_items: active_items
            .into_iter()
            .map(
                |(id, workspace_id, kind, reference, title, tasks_done, tasks_total, access)| {
                    Ok(HomeItem {
                        id,
                        workspace_id,
                        module_kind: ModuleKind::parse(&kind).ok_or_else(|| {
                            AppError::Config(format!("unknown module kind {kind}"))
                        })?,
                        reference,
                        title,
                        tasks_done,
                        tasks_total,
                        can_write: fvoci_core::access::Grade::parse(&access)?
                            .is_some_and(fvoci_core::access::Grade::can_write),
                    })
                },
            )
            .collect::<Result<_, AppError>>()?,
        pins,
        favorites,
    }))
}
