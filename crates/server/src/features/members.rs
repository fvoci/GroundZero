//! 멤버·초대·역할·소유권 인계·그룹 (D-20, D-25, D-51, D-57).
//!
//! 변경은 모두 정의자 함수(`migrations/0003_members.sql`)가 권한 확인·불변식·감사 기록을 한 번에 처리한다.
//! 이 모듈은 입력 검증과 응답 변환만 한다. 함수 오류 코드는 `AppError::from_db_function`이 응답으로 바꾼다.

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
use crate::features::workspaces::{member_role, validate_title};
use crate::http::AppState;
use crate::ids::{Role, UserId, WorkspaceId};

/// 초대 유효 기간(초). 운영 한도로 옮길 수 있게 한곳에 둔다.
const INVITATION_TTL_SECS: i32 = 14 * 24 * 3600;
/// 이메일 길이 상한(RFC 5321).
const MAX_EMAIL: usize = 254;

fn role_str(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Writer => "writer",
        Role::Reader => "reader",
    }
}

fn parse_role(raw: &str) -> Result<Role, AppError> {
    Role::parse(raw).ok_or_else(|| AppError::Config(format!("unknown role in database: {raw}")))
}

async fn call(
    tx: &mut UserTx,
    sql: &str,
    binds: impl FnOnce(
        sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<(), AppError> {
    binds(sqlx::query(sql))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 멤버
// ---------------------------------------------------------------------------

/// 워크스페이스 멤버.
#[derive(Debug, Serialize, ToSchema)]
pub struct Member {
    pub user_id: UserId,
    pub display_name: String,
    /// 관리자에게만 보인다.
    pub email: Option<String>,
    pub role: Role,
    pub is_owner: bool,
}

/// 멤버 화면 데이터.
#[derive(Debug, Serialize, ToSchema)]
pub struct MembersView {
    pub members: Vec<Member>,
    /// 현재 사용자의 역할.
    pub my_role: Role,
    pub i_am_owner: bool,
    /// 대기 중인 소유권 인계 수신자.
    pub pending_owner: Option<UserId>,
}

/// 멤버 목록. 멤버라면 누구나 볼 수 있고 이메일은 관리자에게만 보인다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/members",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = MembersView), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn list_members(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<MembersView>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let my_role = member_role(&mut tx, ws).await?;
    let rows: Vec<(UserId, String, Option<String>, String, bool)> = sqlx::query_as(
        "SELECT u.id, u.display_name, u.email, m.role, w.owner_id = u.id
         FROM workspace_members m
         JOIN users u ON u.id = m.user_id
         JOIN workspaces w ON w.id = m.workspace_id
         WHERE m.workspace_id = $1
         ORDER BY w.owner_id = u.id DESC, u.display_name, u.id",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    let owner: UserId = sqlx::query_scalar("SELECT owner_id FROM workspaces WHERE id = $1")
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    let pending_owner: Option<UserId> =
        sqlx::query_scalar("SELECT to_user FROM ownership_offers WHERE workspace_id = $1")
            .bind(ws)
            .fetch_optional(tx.conn())
            .await?;
    tx.commit().await?;
    let is_admin = my_role == Role::Admin;
    let members = rows
        .into_iter()
        .map(|(user_id, display_name, email, role, is_owner)| {
            Ok(Member {
                user_id,
                display_name,
                email: if is_admin { email } else { None },
                role: parse_role(&role)?,
                is_owner,
            })
        })
        .collect::<Result<_, AppError>>()?;
    Ok(Json(MembersView {
        members,
        my_role,
        i_am_owner: owner == user,
        pending_owner,
    }))
}

/// 역할 변경 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRole {
    pub role: Role,
}

/// 멤버 역할 변경(관리자). 소유자는 관리자에서 내릴 수 없다.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/members/{user_id}",
    params(("workspace_id" = String, Path, format = Uuid), ("user_id" = String, Path, format = Uuid)),
    request_body = SetRole,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "members")]
pub async fn set_role(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((ws, member)): Path<(WorkspaceId, UserId)>,
    Json(input): Json<SetRole>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_set_member_role_ids($1, $2)")
            .bind(ws)
            .bind(member),
    )
    .await?;
    call(&mut tx, "SELECT app_set_member_role($1, $2, $3, $4)", |q| {
        q.bind(ws).bind(member).bind(role_str(input.role)).bind(ids)
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 멤버 제거(관리자). 소유자는 제거할 수 없다. 팀 문서는 워크스페이스에 남는다 (D-25).
#[utoipa::path(delete, path = "/api/workspaces/{workspace_id}/members/{user_id}",
    params(("workspace_id" = String, Path, format = Uuid), ("user_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "members")]
pub async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((ws, member)): Path<(WorkspaceId, UserId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_remove_member_ids($1, $2)")
            .bind(ws)
            .bind(member),
    )
    .await?;
    call(&mut tx, "SELECT app_remove_member($1, $2, $3)", |q| {
        q.bind(ws).bind(member).bind(ids)
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 탈퇴. 소유자는 인계 없이 탈퇴할 수 없다 (D-51).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/leave",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "members")]
pub async fn leave(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_leave_workspace_ids($1)").bind(ws),
    )
    .await?;
    call(&mut tx, "SELECT app_leave_workspace($1, $2)", |q| {
        q.bind(ws).bind(ids)
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 초대
// ---------------------------------------------------------------------------

/// 초대 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInvitation {
    pub email: String,
    pub role: Role,
}

/// 관리자가 보는 초대.
#[derive(Debug, Serialize, ToSchema)]
pub struct Invitation {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub email: String,
    pub role: Role,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

pub fn validate_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= MAX_EMAIL
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    if valid {
        Ok(email)
    } else {
        Err(AppError::BadRequest("email is not valid".into()))
    }
}

/// 초대(관리자). 수락은 그 이메일을 제공자가 검증한 사용자만 할 수 있다 (D-51).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/invitations",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = CreateInvitation,
    responses((status = 201, body = Invitation), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "members")]
pub async fn invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<CreateInvitation>,
) -> Result<(StatusCode, Json<Invitation>), AppError> {
    let email = validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_invite($1, $2, $3, $4, $5)")
        .bind(ws)
        .bind(&email)
        .bind(role_str(input.role))
        .bind(INVITATION_TTL_SECS)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (email, role, expires_at): (String, String, OffsetDateTime) =
        sqlx::query_as("SELECT email, role, expires_at FROM invitations WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Invitation {
            id,
            email,
            role: parse_role(&role)?,
            expires_at,
        }),
    ))
}

/// 열린 초대 목록(관리자).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/invitations",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Invitation]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "members")]
pub async fn list_invitations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Vec<Invitation>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    if member_role(&mut tx, ws).await? != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let rows: Vec<(Uuid, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, email, role, expires_at FROM invitations
         WHERE workspace_id = $1 AND closed_at IS NULL AND expires_at > now() ORDER BY created_at",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(id, email, role, expires_at)| {
            Ok(Invitation {
                id,
                email,
                role: parse_role(&role)?,
                expires_at,
            })
        })
        .collect::<Result<_, AppError>>()
        .map(Json)
}

/// 초대 취소(관리자).
#[utoipa::path(delete, path = "/api/invitations/{invitation_id}",
    params(("invitation_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    call(&mut tx, "SELECT app_revoke_invitation($1, $2)", |q| {
        q.bind(id).bind(new_ids(1))
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 내가 받은 초대.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReceivedInvitation {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    pub role: Role,
    pub invited_by: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

/// 내가 받은 열린 초대(검증된 이메일 기준).
#[utoipa::path(get, path = "/api/invitations",
    responses((status = 200, body = [ReceivedInvitation]), (status = 401, body = ErrorBody)), tag = "members")]
pub async fn my_invitations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<ReceivedInvitation>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(Uuid, WorkspaceId, String, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, workspace_id, workspace_name, role, invited_by_name, expires_at FROM app_my_invitations()",
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(
            |(id, workspace_id, workspace_name, role, invited_by, expires_at)| {
                Ok(ReceivedInvitation {
                    id,
                    workspace_id,
                    workspace_name,
                    role: parse_role(&role)?,
                    invited_by,
                    expires_at,
                })
            },
        )
        .collect::<Result<_, AppError>>()
        .map(Json)
}

async fn respond(
    state: &AppState,
    user: UserId,
    id: Uuid,
    accept: bool,
) -> Result<WorkspaceId, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ws: WorkspaceId = sqlx::query_scalar("SELECT app_respond_invitation($1, $2, $3)")
        .bind(id)
        .bind(accept)
        .bind(new_ids(1))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(ws)
}

/// 수락 결과.
#[derive(Debug, Serialize, ToSchema)]
pub struct Accepted {
    pub workspace_id: WorkspaceId,
}

/// 초대 수락.
#[utoipa::path(post, path = "/api/invitations/{invitation_id}/accept",
    params(("invitation_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Accepted), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "members")]
pub async fn accept_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Accepted>, AppError> {
    let workspace_id = respond(&state, user, id, true).await?;
    Ok(Json(Accepted { workspace_id }))
}

/// 초대 거절.
#[utoipa::path(post, path = "/api/invitations/{invitation_id}/decline",
    params(("invitation_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "members")]
pub async fn decline_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    respond(&state, user, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 소유권 인계
// ---------------------------------------------------------------------------

/// 인계 제안 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OfferOwnership {
    pub to_user: UserId,
}

/// 소유권 인계 제안(소유자). 수신자가 수락해야 완료된다.
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/ownership",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = OfferOwnership,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "members")]
pub async fn offer_ownership(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<OfferOwnership>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    call(&mut tx, "SELECT app_offer_ownership($1, $2, $3)", |q| {
        q.bind(ws).bind(input.to_user).bind(new_ids(1))
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 인계 제안 취소(소유자).
#[utoipa::path(delete, path = "/api/workspaces/{workspace_id}/ownership",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn cancel_ownership_offer(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    call(&mut tx, "SELECT app_cancel_ownership_offer($1, $2)", |q| {
        q.bind(ws).bind(new_ids(1))
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 인계 수락(수신자).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/ownership/accept",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn accept_ownership(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    call(&mut tx, "SELECT app_accept_ownership($1, $2)", |q| {
        q.bind(ws).bind(new_ids(1))
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 그룹
// ---------------------------------------------------------------------------

/// 그룹.
#[derive(Debug, Serialize, ToSchema)]
pub struct Group {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub name: String,
    pub members: Vec<UserId>,
}

/// 그룹 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGroup {
    pub name: String,
}

/// 그룹 목록(멤버).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/groups",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Group]), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn list_groups(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Vec<Group>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    member_role(&mut tx, ws).await?;
    let rows: Vec<(Uuid, String, Vec<Uuid>)> = sqlx::query_as(
        "SELECT g.id, g.name, coalesce(array_agg(gm.user_id ORDER BY gm.user_id)
                                       FILTER (WHERE gm.user_id IS NOT NULL), '{}')
         FROM groups g LEFT JOIN group_members gm ON gm.group_id = g.id
         WHERE g.workspace_id = $1 GROUP BY g.id ORDER BY g.name",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, members)| Group {
                id,
                name,
                members: members.into_iter().map(UserId).collect(),
            })
            .collect(),
    ))
}

/// 그룹 생성(관리자).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/groups",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = CreateGroup,
    responses((status = 201, body = Group), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "members")]
pub async fn create_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<CreateGroup>,
) -> Result<(StatusCode, Json<Group>), AppError> {
    let name = validate_title(&input.name, state.config.limits.max_title_chars, "name")?;
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_create_group($1, $2, $3)")
        .bind(ws)
        .bind(&name)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Group {
            id,
            name,
            members: Vec::new(),
        }),
    ))
}

/// 그룹 삭제(관리자).
#[utoipa::path(delete, path = "/api/groups/{group_id}",
    params(("group_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn delete_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_delete_group_ids($1)").bind(id),
    )
    .await?;
    call(&mut tx, "SELECT app_delete_group($1, $2)", |q| {
        q.bind(id).bind(ids)
    })
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 그룹에 멤버 추가(관리자).
#[utoipa::path(put, path = "/api/groups/{group_id}/members/{user_id}",
    params(("group_id" = String, Path, format = Uuid), ("user_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "members")]
pub async fn add_group_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((group, member)): Path<(Uuid, UserId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    call(
        &mut tx,
        "SELECT app_set_group_member($1, $2, true, $3)",
        |q| q.bind(group).bind(member).bind(new_ids(1)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 그룹에서 멤버 제외(관리자).
#[utoipa::path(delete, path = "/api/groups/{group_id}/members/{user_id}",
    params(("group_id" = String, Path, format = Uuid), ("user_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "members")]
pub async fn remove_group_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((group, member)): Path<(Uuid, UserId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_set_group_member_ids($1, $2, false)")
            .bind(group)
            .bind(member),
    )
    .await?;
    call(
        &mut tx,
        "SELECT app_set_group_member($1, $2, false, $3)",
        |q| q.bind(group).bind(member).bind(ids),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::validate_email;

    #[test]
    fn email_validation() {
        assert_eq!(
            validate_email("  Alice@Example.TEST ").unwrap(),
            "alice@example.test"
        );
        assert!(validate_email("no-at-sign").is_err());
        assert!(validate_email("@example.test").is_err());
        assert!(validate_email("a@localhost").is_err());
        assert!(validate_email("a b@example.test").is_err());
    }
}
