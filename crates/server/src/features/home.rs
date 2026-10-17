//! 홈 (D-19): Up Next(기한 초과·오늘·다가오는 업무), 최근 문서·최근 작업, 진행 중인 항목과 의미에 맞는 진행률,
//! 고정·즐겨찾기. 모두 볼 수 있는 것만(RLS) 나온다. 주간 달력은 달력 API, 실행 중인 스톱워치는 타이머 API,
//! 알림은 알림 API를 그대로 쓴다. 처리는 `migrations/0017_home.sql`.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::modules::ModuleKind;
use crate::features::tasks::{TaskStatus, format_date};
use crate::http::AppState;
use crate::ids::{ItemId, WorkspaceId};

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
    pub status: TaskStatus,
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

/// 홈.
#[derive(Debug, Serialize, ToSchema)]
pub struct Home {
    /// 보는 사람 시간대의 오늘.
    pub today: String,
    pub up_next: Vec<HomeTask>,
    pub recent_tasks: Vec<HomeTask>,
    pub recent_documents: Vec<HomeDocument>,
    pub active_items: Vec<HomeItem>,
    pub pins: Vec<MarkedTarget>,
    pub favorites: Vec<MarkedTarget>,
}

/// 보는 사람 시간대.
#[derive(Debug, Deserialize, IntoParams)]
pub struct HomeQuery {
    pub tz: String,
}

type TaskRow = (
    Uuid,
    ItemId,
    WorkspaceId,
    Option<String>,
    String,
    String,
    Option<Date>,
);

fn to_task(row: TaskRow, today: Option<Date>) -> Result<HomeTask, AppError> {
    let (id, item_id, workspace_id, reference, title, status, due) = row;
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
        status: TaskStatus::parse(&status)
            .ok_or_else(|| AppError::Config(format!("unknown task status {status}")))?,
        due_date: format_date(due),
        bucket,
    })
}

async fn marks(tx: &mut crate::db::UserTx, mark: MarkKind) -> Result<Vec<MarkedTarget>, AppError> {
    type MarkRow = (
        String,
        Uuid,
        Option<String>,
        Option<WorkspaceId>,
        Option<ItemId>,
    );
    let rows: Vec<MarkRow> = sqlx::query_as(
        "SELECT m.target_kind, m.target_id, app_time_target_label(m.target_kind, m.target_id),
                    coalesce(t.workspace_id, d.workspace_id, i.workspace_id),
                    coalesce(t.item_id, d.item_id)
             FROM user_marks m
             LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
             LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
             LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
             WHERE m.mark = $1 ORDER BY m.created_at, m.target_id",
    )
    .bind(mark.as_str())
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(kind, id, label, workspace_id, item_id)| {
            Some(MarkedTarget {
                kind: MarkTarget::parse(&kind)?,
                id,
                // 볼 수 없게 된 대상은 나오지 않는다.
                label: label?,
                workspace_id,
                item_id,
            })
        })
        .collect())
}

/// 홈(로그인한 사람). 모든 워크스페이스에서 볼 수 있는 것만 모은다.
#[utoipa::path(get, path = "/api/me/home", operation_id = "home", params(HomeQuery),
    responses((status = 200, body = Home), (status = 400, body = ErrorBody)), tag = "home")]
pub async fn home(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<HomeQuery>,
) -> Result<Json<Home>, AppError> {
    let tz = query.tz.trim();
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let today: Date = sqlx::query_scalar("SELECT (now() AT TIME ZONE $1)::date")
        .bind(tz)
        .fetch_one(tx.conn())
        .await?;
    // 내 업무: 내가 담당이거나, 담당자가 없고 내가 만든 것. 보관된 항목의 업무는 뺀다.
    let up_next: Vec<TaskRow> = sqlx::query_as(
        "SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id), t.title, t.status, t.due_date
         FROM tasks t JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
         WHERE t.status <> 'done' AND t.due_date IS NOT NULL AND t.due_date <= $1 + 7
           AND i.archived_at IS NULL AND m.archived_at IS NULL
           AND (EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = app_user_id())
                OR (NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id)
                    AND t.created_by = app_user_id()))
         ORDER BY t.due_date, t.number LIMIT 50",
    )
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    let recent_tasks: Vec<TaskRow> = sqlx::query_as(
        "SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id), t.title, t.status, t.due_date
         FROM tasks t WHERE t.updated_by = app_user_id() ORDER BY t.updated_at DESC, t.id LIMIT 8",
    )
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
         FROM documents d WHERE d.updated_by = app_user_id() ORDER BY d.updated_at DESC, d.id LIMIT 8",
    )
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
    );
    let active_items: Vec<ItemRow> = sqlx::query_as(
        "SELECT i.id, i.workspace_id, m.kind, app_item_reference(i.id), i.title,
                (SELECT count(*) FROM tasks t WHERE t.item_id = i.id AND t.status = 'done'),
                (SELECT count(*) FROM tasks t WHERE t.item_id = i.id)
         FROM items i JOIN modules m ON m.id = i.module_id
         WHERE i.archived_at IS NULL AND m.archived_at IS NULL
         ORDER BY greatest(i.updated_at, (SELECT max(t.updated_at) FROM tasks t WHERE t.item_id = i.id)) DESC, i.id
         LIMIT 8",
    )
    .fetch_all(tx.conn())
    .await?;
    let pins = marks(&mut tx, MarkKind::Pin).await?;
    let favorites = marks(&mut tx, MarkKind::Favorite).await?;
    tx.commit().await?;
    Ok(Json(Home {
        today: format_date(Some(today)).unwrap_or_default(),
        up_next: up_next
            .into_iter()
            .map(|r| to_task(r, Some(today)))
            .collect::<Result<_, _>>()?,
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
                |(id, workspace_id, kind, reference, title, tasks_done, tasks_total)| {
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
    tx: &mut crate::db::UserTx,
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
