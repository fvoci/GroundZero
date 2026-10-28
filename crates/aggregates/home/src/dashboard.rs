//! 홈 (D-19): Up Next(기한 초과·오늘·다가오는 업무), 최근 문서·최근 작업, 진행 중인 항목과 의미에 맞는 진행률,
//! 고정·즐겨찾기. 모두 볼 수 있는 것만(RLS) 나온다. 주간 달력은 달력 API, 실행 중인 스톱워치는 타이머 API,
//! 알림은 알림 API를 그대로 쓴다. 고정·즐겨찾기는 `crates/db/migrations/202610280008_activity.sql`의 `app_set_mark`가 처리한다.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{HomeState, progress::HomeProgress};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, OrganizationId, WorkspaceId};
use fvoci_core::marks::MarkTarget;
use fvoci_core::module::ModuleKind;
use fvoci_http::organization::OrgScope;

/// Up Next 구분.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DueBucket {
    Overdue,
    Today,
    Upcoming,
}

/// 홈의 업무 한 줄.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

/// 진행 중인 항목. 업무·강의·스터디의 진행률은 각각의 의미와 단위를 유지한다(D-19).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HomeItem {
    pub id: ItemId,
    pub workspace_id: WorkspaceId,
    pub module_kind: ModuleKind,
    pub reference: Option<String>,
    pub title: String,
    pub progress: HomeProgress,
    /// 이 항목에 업무·일정을 더할 수 있는지(빠른 추가).
    pub can_write: bool,
}

/// 표시한 대상(지금 볼 수 있는 것만).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpNextCounts {
    pub overdue: i64,
    pub today: i64,
    pub upcoming: i64,
}

/// 홈.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Home {
    /// 홈의 내 업무 요약. 독립 내 업무 화면과 같은 집계 계약이다.
    pub my_tasks: crate::tasks::MyTasks,
    /// 보는 사람 시간대의 오늘.
    pub today: String,
    /// 묶음(기한 지남·오늘·다가오는 일)마다 마감이 이른 것부터 50개까지.
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

/// 현재 조직의 홈 집계를 호출자 권한으로 읽는다.
#[utoipa::path(get, path = "/api/me/home", operation_id = "home", params(HomeQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = Home), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "home")]
pub async fn home(
    State(state): State<HomeState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<HomeQuery>,
) -> Result<Json<Home>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let sqlx::types::Json(home) = sqlx::query_scalar("SELECT app_home($1, $2)")
        .bind(organization)
        .bind(query.tz)
        .fetch_one(tx.conn())
        .await
        .map_err(home_error)?;
    tx.commit().await?;
    Ok(Json(home))
}

fn home_error(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .is_some_and(|db| db.code().as_deref() == Some("22023"))
    {
        AppError::invalid(
            "tz",
            InvalidReason::InvalidFormat,
            None,
            "tz must be a known time zone",
        )
    } else {
        AppError::from_db_function(error)
    }
}
