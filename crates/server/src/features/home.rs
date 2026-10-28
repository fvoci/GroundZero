//! 홈 (D-19): Up Next(기한 초과·오늘·다가오는 업무), 최근 문서·최근 작업, 진행 중인 항목과 의미에 맞는 진행률,
//! 고정·즐겨찾기. 모두 볼 수 있는 것만(RLS) 나온다. 주간 달력은 달력 API, 실행 중인 스톱워치는 타이머 API,
//! 알림은 알림 API를 그대로 쓴다. 고정·즐겨찾기는 `crates/db/migrations/202610250001_initial_state.sql`의 `app_set_mark`가 처리한다.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::features::modules::ModuleKind;
use crate::features::organizations::OrgScope;
use crate::features::tasks::format_date;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, OrganizationId, WorkspaceId};
use fvoci_http::session::CurrentUser;

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
    fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Document => "document",
            Self::Task => "task",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
            _ => None,
        }
    }
}

/// 고정 또는 즐겨찾기.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Pin,
    Favorite,
}

impl MarkKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pin => "pin",
            Self::Favorite => "favorite",
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct MyTasksQuery {
    /// YYYY-MM-DD. 없으면 저장된 사용자 시간대의 오늘(미설정은 UTC).
    pub today: Option<String>,
}

/// SQL bucket: overdue, today, upcoming, none(날짜 없음), done.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct MyTask {
    pub id: Uuid,
    pub item_id: ItemId,
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub priority: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub bucket: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct MyTaskCounts {
    pub overdue: i32,
    pub today: i32,
    pub upcoming: i32,
    pub undated: i32,
    pub done: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceTasks {
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    pub tasks: Vec<MyTask>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyTasks {
    pub counts: MyTaskCounts,
    pub workspaces: Vec<WorkspaceTasks>,
}

#[derive(sqlx::FromRow)]
struct MyTaskRow {
    workspace_id: WorkspaceId,
    workspace_name: String,
    #[sqlx(flatten)]
    task: MyTask,
}

#[utoipa::path(get, path = "/api/me/tasks", operation_id = "my_tasks",
    params(MyTasksQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = MyTasks), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "home")]
pub async fn my_tasks(
    State(state): State<AppState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<MyTasksQuery>,
) -> Result<Json<MyTasks>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let today = match query.today {
        Some(raw) => Date::parse(&raw, time::macros::format_description!("[year]-[month]-[day]"))
            .map_err(|_| AppError::BadRequest("today must be YYYY-MM-DD".to_owned()))?,
        None => sqlx::query_scalar(
            "SELECT (now() AT TIME ZONE coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC'))::date",
        ).fetch_one(tx.conn()).await?,
    };
    let rows: Vec<MyTaskRow> = sqlx::query_as(
        "SELECT workspace_id, workspace_name, task_id AS id, item_id, reference, title, status,
                status_category, priority, start_date::text, due_date::text, bucket, updated_at
         FROM app_my_tasks($1, $2)",
    )
    .bind(organization)
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    let counts = sqlx::query_as("SELECT * FROM app_my_task_counts($1, $2)")
        .bind(organization)
        .bind(today)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    let mut workspaces: Vec<WorkspaceTasks> = Vec::new();
    for row in rows {
        if let Some(group) = workspaces
            .last_mut()
            .filter(|g| g.workspace_id == row.workspace_id)
        {
            group.tasks.push(row.task);
        } else {
            workspaces.push(WorkspaceTasks {
                workspace_id: row.workspace_id,
                workspace_name: row.workspace_name,
                tasks: vec![row.task],
            });
        }
    }
    Ok(Json(MyTasks { counts, workspaces }))
}

/// Up Next 후보: 내 업무(내가 담당이거나, 담당자가 없고 내가 만든 것) 중 이번 주($1 + 7)까지 마감인 미완료.
/// 보관된 항목·모듈의 업무는 뺀다.
const UP_NEXT_MINE: &str = "SELECT t.id, t.item_id, t.workspace_id, t.title, s.name AS status, s.category AS status_category, t.due_date, t.number
     FROM tasks t JOIN task_statuses s ON s.id = t.status_id JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
     WHERE s.category <> 'done' AND t.due_date IS NOT NULL AND t.due_date <= $1 + 7
       AND i.archived_at IS NULL AND m.archived_at IS NULL
       AND t.workspace_id IN (SELECT app_my_workspace_ids($2))
       AND (EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = app_user_id())
            OR (NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id)
                AND t.created_by = app_user_id()))";

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

type TaskRow = (
    Uuid,
    ItemId,
    WorkspaceId,
    Option<String>,
    String,
    String,
    String,
    Option<Date>,
);

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
    State(state): State<AppState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<HomeQuery>,
) -> Result<Json<Home>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 빈 `tz=`는 없는 것과 같다(검색과 같은 규칙): 저장된 시간대, 없으면 UTC.
    let tz = match query.tz.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(tz) => tz.to_owned(),
        None => sqlx::query_scalar(
            "SELECT coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC')",
        ).fetch_one(tx.conn()).await?,
    };
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(&tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let today: Date = sqlx::query_scalar("SELECT (now() AT TIME ZONE $1)::date")
        .bind(&tz)
        .fetch_one(tx.conn())
        .await?;
    // 묶음마다 따로 자른다. 한 번에 50개로 자르면 밀린 업무가 많을 때 오늘·다가오는 일이 빠졌다(감사 W23).
    let up_next: Vec<TaskRow> = sqlx::query_as(&format!(
        "WITH mine AS ({UP_NEXT_MINE}),
         ranked AS (
           SELECT mine.*, row_number() OVER (
                    PARTITION BY CASE WHEN due_date < $1 THEN 0 WHEN due_date = $1 THEN 1 ELSE 2 END
                    ORDER BY due_date, number, id) AS rn
           FROM mine
         )
         SELECT id, item_id, workspace_id, app_task_reference(id), title, status, status_category, due_date
         FROM ranked WHERE rn <= $3 ORDER BY due_date, number, id"
    ))
    .bind(today)
    .bind(organization)
    .bind(UP_NEXT_PER_BUCKET)
    .fetch_all(tx.conn())
    .await?;
    let (overdue, due_today, upcoming): (i64, i64, i64) = sqlx::query_as(&format!(
        "WITH mine AS ({UP_NEXT_MINE})
         SELECT count(*) FILTER (WHERE due_date < $1), count(*) FILTER (WHERE due_date = $1),
                count(*) FILTER (WHERE due_date > $1)
         FROM mine"
    ))
    .bind(today)
    .bind(organization)
    .fetch_one(tx.conn())
    .await?;
    let recent_tasks: Vec<TaskRow> = sqlx::query_as(
        "SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id), t.title, s.name AS status, s.category AS status_category, t.due_date
         FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.updated_by = app_user_id() AND t.workspace_id IN (SELECT app_my_workspace_ids($1)) ORDER BY t.updated_at DESC, t.id LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;
    type DocRow = (
        Uuid,
        String,
        Option<String>,
        Option<WorkspaceId>,
        Option<ItemId>,
        OffsetDateTime,
    );
    let recent_documents: Vec<DocRow> = sqlx::query_as(
        "SELECT d.id, d.title, app_document_reference(d.id), d.workspace_id, d.item_id, d.updated_at
         FROM documents d WHERE d.updated_by = app_user_id() AND (d.workspace_id IN (SELECT app_my_workspace_ids($1)) OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id())) ORDER BY d.updated_at DESC, d.id LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;
    type ItemRow = (
        ItemId,
        WorkspaceId,
        String,
        Option<String>,
        String,
        i64,
        i64,
        String,
    );
    let active_items: Vec<ItemRow> = sqlx::query_as(
        "SELECT i.id, i.workspace_id, m.kind, app_item_reference(i.id), i.title, c.done, c.total,
                app_access('item', i.id)
         FROM items i JOIN modules m ON m.id = i.module_id
         CROSS JOIN LATERAL (
           SELECT count(*) FILTER (WHERE s.category = 'done') AS done, count(*) AS total, max(t.updated_at) AS active
           FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.item_id = i.id
         ) c
         WHERE i.archived_at IS NULL AND m.archived_at IS NULL
           AND i.workspace_id IN (SELECT app_my_workspace_ids($1))
         ORDER BY greatest(i.updated_at, c.active) DESC, i.id
         LIMIT 8",
    )
    .bind(organization)
    .fetch_all(tx.conn())
    .await?;
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
                        can_write: crate::access::Grade::parse(&access)?
                            .is_some_and(crate::access::Grade::can_write),
                    })
                },
            )
            .collect::<Result<_, AppError>>()?,
        pins,
        favorites,
    }))
}

/// 고정·즐겨찾기 켜기·끄기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetMark {
    pub mark: MarkKind,
    pub on: bool,
}

/// 한 대상의 표시 상태.
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkState {
    pub pin: bool,
    pub favorite: bool,
}

/// 대상의 고정·즐겨찾기 켜기·끄기(본인). 켤 때 볼 수 없으면 404.
#[utoipa::path(put, path = "/api/me/marks/{target_kind}/{target_id}", operation_id = "set_mark",
    params(("target_kind" = MarkTarget, Path), ("target_id" = String, Path, format = Uuid)),
    request_body = SetMark,
    responses((status = 200, body = MarkState), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "home")]
pub async fn set_mark(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((kind, id)): Path<(MarkTarget, Uuid)>,
    Json(input): Json<SetMark>,
) -> Result<Json<MarkState>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_mark($1, $2, $3, $4)")
        .bind(input.mark.as_str())
        .bind(kind.as_str())
        .bind(id)
        .bind(input.on)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let marks = mark_state(&mut tx, kind, id).await?;
    tx.commit().await?;
    Ok(Json(marks))
}

async fn mark_state(
    tx: &mut fvoci_db::UserTx,
    kind: MarkTarget,
    id: Uuid,
) -> Result<MarkState, AppError> {
    let (pin, favorite): (bool, bool) = sqlx::query_as(
        "SELECT bool_or(mark = 'pin') IS TRUE, bool_or(mark = 'favorite') IS TRUE
         FROM user_marks WHERE target_kind = $1 AND target_id = $2",
    )
    .bind(kind.as_str())
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(MarkState { pin, favorite })
}

/// 한 대상의 내 표시 상태.
#[utoipa::path(get, path = "/api/me/marks/{target_kind}/{target_id}", operation_id = "get_mark",
    params(("target_kind" = MarkTarget, Path), ("target_id" = String, Path, format = Uuid)),
    responses((status = 200, body = MarkState)), tag = "home")]
pub async fn get_mark(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((kind, id)): Path<(MarkTarget, Uuid)>,
) -> Result<Json<MarkState>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let marks = mark_state(&mut tx, kind, id).await?;
    tx.commit().await?;
    Ok(Json(marks))
}
