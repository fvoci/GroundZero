//! 댓글·알림·활동·감사·고정 도메인. 라우터와 OpenAPI 조각을 조립 crate에 제공한다.
#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]
pub mod activity;
pub mod audit;
pub mod comments;
pub mod marks;
pub mod notifications;
pub mod retention;
pub mod state;
use axum::{
    Router,
    extract::FromRef,
    routing::{get, post, put},
};
pub use state::ActivityState;
use utoipa::OpenApi;
/// 소통·활동 API 계약.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        comments::document_comments,
        comments::task_comments,
        comments::add,
        comments::edit,
        comments::delete,
        comments::resolve,
        notifications::notifications,
        notifications::mark_read,
        notifications::unread_count,
        activity::recent,
        activity::item,
        activity::document,
        activity::task,
        audit::workspace,
        audit::mine,
        marks::set_mark,
        marks::get_mark
    ),
    components(schemas(
        comments::Comment,
        comments::NewComment,
        comments::EditComment,
        comments::ResolveComment,
        comments::CommentCreated,
        notifications::Notification,
        notifications::NotificationPage,
        notifications::MarkRead,
        notifications::MarkedNotifications,
        notifications::UnreadCount,
        activity::Activity,
        activity::ActivityPage,
        audit::AuditEntry,
        marks::MarkTarget,
        marks::MarkKind,
        marks::SetMark,
        marks::MarkState
    ))
)]
pub struct ActivityApi;
/// 소통·활동 경로. 세션 상태는 조립 crate에서 추출한다.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    ActivityState: FromRef<S>,
    fvoci_http::session::SessionState: FromRef<S>,
{
    Router::new()
        .route(
            "/documents/{document_id}/comments",
            get(comments::document_comments),
        )
        .route("/tasks/{task_id}/comments", get(comments::task_comments))
        .route("/comments", post(comments::add))
        .route(
            "/comments/{comment_id}",
            put(comments::edit).delete(comments::delete),
        )
        .route("/comments/{comment_id}/resolve", post(comments::resolve))
        .route("/me/notifications", get(notifications::notifications))
        .route("/me/notifications/read", post(notifications::mark_read))
        .route(
            "/me/notifications/unread-count",
            get(notifications::unread_count),
        )
        .route("/me/activity", get(activity::recent))
        .route("/items/{item_id}/activity", get(activity::item))
        .route("/documents/{document_id}/activity", get(activity::document))
        .route("/tasks/{task_id}/activity", get(activity::task))
        .route("/workspaces/{workspace_id}/audit", get(audit::workspace))
        .route("/me/audit", get(audit::mine))
        .route(
            "/me/marks/{target_kind}/{target_id}",
            get(marks::get_mark).put(marks::set_mark),
        )
}
