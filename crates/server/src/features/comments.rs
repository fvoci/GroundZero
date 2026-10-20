//! 댓글 (D-27, D-20). 문서와 업무에 다는 한 단계 스레드. 대상을 볼 수 있으면 읽고 쓴다(독자·공유받은 사람 포함).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, needed_ids, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{DocumentId, UserId};

/// 댓글 하나.
#[derive(Debug, Serialize, ToSchema)]
pub struct Comment {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// 답글이면 스레드의 첫 댓글.
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
    pub body: String,
    pub author_id: UserId,
    /// 워크스페이스를 함께 쓰지 않으면(공유받은 사람 등) 없을 수 있다.
    pub author_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub edited: bool,
    /// 답글이 남아 있어 "지운 댓글"로 보이는 스레드 첫 댓글.
    pub deleted: bool,
    /// 스레드 첫 댓글의 해결 표시.
    pub resolved: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    /// 스레드를 해결·해제할 수 있는지(첫 댓글에서만).
    pub can_resolve: bool,
}

/// 댓글 달기. `document_id`나 `task_id` 하나, 답글이면 `parent_id`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewComment {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub document_id: Option<DocumentId>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub task_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
    pub body: String,
}

/// 댓글 고치기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EditComment {
    pub body: String,
}

/// 스레드 해결 표시.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveComment {
    pub resolved: bool,
}

/// 만든 댓글.
#[derive(Debug, Serialize, ToSchema)]
pub struct CommentCreated {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

type Row = (
    Uuid,
    Option<Uuid>,
    String,
    UserId,
    Option<String>,
    OffsetDateTime,
    bool,
    bool,
    bool,
);

async fn list(
    tx: &mut UserTx,
    document: Option<Uuid>,
    task: Option<Uuid>,
) -> Result<Vec<Comment>, AppError> {
    let access: Option<String> = sqlx::query_scalar("SELECT app_comment_target_access($1, $2)")
        .bind(document)
        .bind(task)
        .fetch_one(tx.conn())
        .await?;
    let Some(access) = access else {
        return Err(AppError::NotFound);
    };
    let me = tx.user();
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT c.id, c.parent_id, c.body, c.created_by, u.display_name, c.created_at, c.edited,
                c.deleted_at IS NOT NULL, c.resolved_at IS NOT NULL
         FROM comments c LEFT JOIN users u ON u.id = c.created_by
         WHERE c.document_id IS NOT DISTINCT FROM $1 AND c.task_id IS NOT DISTINCT FROM $2
           AND ($1::uuid IS NOT NULL OR $2::uuid IS NOT NULL)
         ORDER BY coalesce(c.parent_id, c.id), c.parent_id NULLS FIRST, c.created_at, c.id",
    )
    .bind(document)
    .bind(task)
    .fetch_all(tx.conn())
    .await?;
    let owner = access == "owner";
    let writer = matches!(access.as_str(), "owner" | "write");
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                parent_id,
                body,
                author_id,
                author_name,
                created_at,
                edited,
                deleted,
                resolved,
            )| {
                let mine = author_id == me;
                Comment {
                    id,
                    parent_id,
                    body,
                    author_id,
                    author_name,
                    created_at,
                    edited,
                    deleted,
                    resolved,
                    can_edit: mine && !deleted,
                    can_delete: (mine || owner) && !deleted,
                    can_resolve: parent_id.is_none() && (mine || writer),
                }
            },
        )
        .collect())
}

/// 문서의 댓글(스레드 순, 첫 댓글 다음에 답글).
#[utoipa::path(get, path = "/api/documents/{document_id}/comments", operation_id = "document_comments",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Comment]), (status = 404, body = ErrorBody)), tag = "comments")]
pub async fn document_comments(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(doc): Path<DocumentId>,
) -> Result<Json<Vec<Comment>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let out = list(&mut tx, Some(doc.0), None).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// 업무의 댓글.
#[utoipa::path(get, path = "/api/tasks/{task_id}/comments", operation_id = "task_comments",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Comment]), (status = 404, body = ErrorBody)), tag = "comments")]
pub async fn task_comments(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(task): Path<Uuid>,
) -> Result<Json<Vec<Comment>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let out = list(&mut tx, None, Some(task)).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// 댓글 달기(대상을 볼 수 있는 사람). 같은 요청 id면 같은 댓글. 대상을 만든 사람·담당자, 답글이면 스레드 참여자에게 알린다.
#[utoipa::path(post, path = "/api/comments", operation_id = "add_comment", request_body = NewComment,
    responses((status = 201, body = CommentCreated), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "comments")]
pub async fn add(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<NewComment>,
) -> Result<(StatusCode, Json<CommentCreated>), AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_add_comment_ids($1, $2, $3)")
            .bind(input.document_id)
            .bind(input.task_id)
            .bind(input.parent_id),
    )
    .await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_add_comment($1, $2, $3, $4, $5, $6)")
        .bind(input.request_id)
        .bind(input.document_id)
        .bind(input.task_id)
        .bind(input.parent_id)
        .bind(&input.body)
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CommentCreated { id })))
}

/// 고치기(쓴 사람만).
#[utoipa::path(put, path = "/api/comments/{comment_id}", operation_id = "edit_comment",
    params(("comment_id" = String, Path, format = Uuid)), request_body = EditComment,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "comments")]
pub async fn edit(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<EditComment>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_edit_comment($1, $2)")
        .bind(id)
        .bind(&input.body)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 지우기(쓴 사람 또는 대상의 소유 권한).
#[utoipa::path(delete, path = "/api/comments/{comment_id}", operation_id = "delete_comment",
    params(("comment_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "comments")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_comment($1, $2)")
        .bind(id)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 스레드 해결 표시·해제(스레드를 연 사람 또는 대상에 쓸 수 있는 사람).
#[utoipa::path(post, path = "/api/comments/{comment_id}/resolve", operation_id = "resolve_comment",
    params(("comment_id" = String, Path, format = Uuid)), request_body = ResolveComment,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "comments")]
pub async fn resolve(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ResolveComment>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_resolve_comment($1, $2)")
        .bind(id)
        .bind(input.resolved)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
