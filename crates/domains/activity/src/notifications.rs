//! 조직 범위 알림함(D-118). 초대는 SQL 계약에 따라 모든 조직 문맥에 보인다.
use crate::ActivityState;
use axum::Json;
use axum::extract::{Query, State};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, ItemId, OrganizationId, WorkspaceId};
use fvoci_http::organization::OrgScope;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
const NOTIFICATION_PAGE: i32 = 50;
/// 알림. invite는 초대 ID만 있고 대상 제목·참조는 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Notification {
    pub organization_id: Option<OrganizationId>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub invitation_id: Option<Uuid>,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// `mention`(문서에서 언급)·`comment`(내 문서·업무에 댓글)·`reply`(참여한 스레드에 답글)·`due`(업무 기한: 내일·오늘)·
    /// `expiry`(강좌 수강권 만료·목표 완료일)·`review`(복습일)·`deadline`(투고 제출·수정 마감, 산출물 마감).
    pub kind: String,
    /// 대상 문서(문서 알림).
    pub document_id: Option<DocumentId>,
    /// 대상 업무(업무 알림).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub task_id: Option<Uuid>,
    /// 대상 제목(문서 또는 업무).
    pub document_title: Option<String>,
    /// 대상 번호(문서 또는 업무).
    pub document_reference: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
    /// 댓글·답글이면 그 댓글.
    #[schema(value_type = Option<String>, format = Uuid)]
    pub comment_id: Option<Uuid>,
    /// 댓글 첫 부분(최대 140자).
    pub excerpt: Option<String>,
    /// 기한·만료·복습·마감 알림의 날짜(YYYY-MM-DD).
    pub due_date: Option<String>,
    /// 항목 알림의 대상 이름(기록 제목이나 강좌 이름).
    pub subject: Option<String>,
    /// 한 사람. 워크스페이스를 함께 쓰지 않거나(공유받은 개인 노트 등) 자동 알림(기한)이면 없다.
    pub actor_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub detail: serde_json::Value,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub read_at: Option<OffsetDateTime>,
    pub read: bool,
}

/// 읽음 표시 요청. `id`가 없으면 모두 읽음.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkRead {
    #[schema(value_type = Option<String>, format = Uuid)]
    pub id: Option<Uuid>,
}

type NotificationRow = (
    Uuid,
    Option<OrganizationId>,
    Option<Uuid>,
    String,
    Option<DocumentId>,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    serde_json::Value,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<String>,
    Option<String>,
    Option<WorkspaceId>,
    Option<ItemId>,
);

/// 알림 범위와 페이지 커서.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CursorQuery {
    pub limit: Option<i32>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[param(value_type = Option<String>, format = DateTime)]
    pub before: Option<OffsetDateTime>,
    pub before_id: Option<Uuid>,
}

/// 내 알림(최근 순). 대상을 볼 수 없게 되면 나오지 않는다.
#[utoipa::path(get, path = "/api/me/notifications", operation_id = "list_my_notifications",
    params(CursorQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = NotificationPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "notifications")]
pub async fn notifications(
    State(state): State<ActivityState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<CursorQuery>,
) -> Result<Json<NotificationPage>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    query.validate()?;
    let limit = query.limit.unwrap_or(NOTIFICATION_PAGE).clamp(1, 200);
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT n.id, n.organization_id, n.invitation_id, n.kind, n.document_id, n.task_id, n.comment_id, n.actor_name, n.detail, n.created_at, n.read_at,
                coalesce(d.title, t.title, i.title),
                CASE WHEN d.id IS NOT NULL THEN app_document_reference(d.id)
                     WHEN t.id IS NOT NULL THEN app_task_reference(t.id)
                     ELSE app_item_reference(i.id) END,
                coalesce(d.workspace_id, t.workspace_id, i.workspace_id),
                CASE WHEN app_access('item', coalesce(d.item_id, t.item_id, i.id)) <> 'none'
                     THEN coalesce(d.item_id, t.item_id, i.id) END
         FROM app_my_notifications($1, $2, $3, $4) n
         LEFT JOIN documents d ON d.id = n.document_id
         LEFT JOIN tasks t ON t.id = n.task_id
         LEFT JOIN items i ON i.id = n.item_id
         ORDER BY n.created_at DESC, n.id DESC",
    )
    .bind(organization)
    .bind(limit)
    .bind(query.before)
    .bind(query.before_id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let notifications: Vec<Notification> = rows
        .into_iter()
        .map(
            |(
                id,
                organization_id,
                invitation_id,
                kind,
                document_id,
                task_id,
                comment_id,
                actor_name,
                detail,
                created_at,
                read_at,
                title,
                reference,
                workspace_id,
                item_id,
            )| Notification {
                id,
                organization_id,
                invitation_id,
                kind,
                document_id,
                task_id,
                document_title: title,
                document_reference: reference,
                workspace_id,
                item_id,
                comment_id,
                excerpt: detail
                    .get("excerpt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                due_date: detail
                    .get("due_date")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                subject: detail
                    .get("subject")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                actor_name,
                created_at,
                read: read_at.is_some(),
                read_at,
                detail,
            },
        )
        .collect();
    let next = notifications
        .last()
        .filter(|_| i32::try_from(notifications.len()).is_ok_and(|n| n == limit));
    let (next_before, next_before_id) = (next.map(|n| n.created_at), next.map(|n| n.id));
    Ok(Json(NotificationPage {
        notifications,
        next_before,
        next_before_id,
    }))
}

/// 알림 읽음 표시. 내 알림만 바뀐다.
#[utoipa::path(post, path = "/api/me/notifications/read", operation_id = "mark_my_notifications_read",
    params(("organization_id" = OrganizationId, Query)), request_body = MarkRead,
    responses((status = 200, body = MarkedNotifications), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "notifications")]
pub async fn mark_read(
    State(state): State<ActivityState>,
    OrgScope { user, organization }: OrgScope,
    Json(input): Json<MarkRead>,
) -> Result<Json<MarkedNotifications>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let marked = sqlx::query_scalar("SELECT app_mark_notifications_read($1, $2)")
        .bind(input.id)
        .bind(organization)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(MarkedNotifications { marked }))
}

impl CursorQuery {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.before.is_some() != self.before_id.is_some() {
            return Err(AppError::BadRequest(
                "before and before_id must be provided together".to_owned(),
            ));
        }
        Ok(())
    }
}

/// 목록 끝에서 두 커서가 null이면 더 읽을 페이지가 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationPage {
    pub notifications: Vec<Notification>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub next_before: Option<OffsetDateTime>,
    pub next_before_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarkedNotifications {
    pub marked: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCount {
    pub unread_count: i32,
}

#[utoipa::path(get, path = "/api/me/notifications/unread-count", operation_id = "my_notification_unread_count",
    params(("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = UnreadCount), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "notifications")]
pub async fn unread_count(
    State(state): State<ActivityState>,
    OrgScope { user, organization }: OrgScope,
) -> Result<Json<UnreadCount>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let unread_count = sqlx::query_scalar("SELECT app_notification_unread_count($1)")
        .bind(organization)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(Json(UnreadCount { unread_count }))
}
