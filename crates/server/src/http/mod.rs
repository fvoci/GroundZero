//! 라우팅과 HTTP 공통 경계: 상태, Origin 검사, 본문 크기 제한, 정적 웹 서빙, OpenAPI 계약.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{OpenApi, ToSchema};

use crate::config::Config;
use crate::db::Db;
use crate::error::{AppError, ErrorBody};
use crate::features::{accounts, documents, members, workspaces};

/// 핸들러가 공유하는 상태.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub oidc: accounts::SharedProvider,
}

/// 서비스 상태 응답.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// 전체 상태. `ok` 또는 `degraded`.
    pub status: &'static str,
    /// 데이터베이스 왕복 결과. `ok` 또는 `error`.
    pub database: &'static str,
}

/// 서비스와 데이터베이스 연결 상태를 돌려준다.
#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "모두 정상", body = Health),
        (status = 503, description = "데이터베이스 연결 실패", body = Health),
    ),
    tag = "system"
)]
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match state.db.ping().await {
        Ok(()) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                database: "ok",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = %err, "database ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "error",
                }),
            )
        }
    }
}

/// 상태를 바꾸는 요청은 브라우저의 `Origin`이 공개 원점과 같아야 한다 (CSRF 방지).
/// SameSite=Lax 쿠키만으로는 JSON API의 교차 출처 요청을 다 막지 못한다.
async fn require_same_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(state.config.public_origin().as_str()) {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "bad_origin",
                    message: "cross-origin request rejected".into(),
                }),
            )
                .into_response();
        }
    }
    next.run(request).await
}

/// 알 수 없는 API 경로는 웹 fallback이 아니라 404 JSON.
async fn api_not_found() -> AppError {
    AppError::NotFound
}

/// 공개 API 계약. `contracts/openapi.json`은 여기서 생성한다.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "fvoci API", version = "0.1.0"),
    paths(
        healthz,
        accounts::login,
        accounts::callback,
        accounts::logout,
        accounts::me,
        accounts::auth_info,
        workspaces::list,
        workspaces::create,
        documents::list_wiki,
        documents::create_wiki,
        documents::get,
        documents::save,
        documents::my_notes,
        documents::create_note,
        documents::shared_with_me,
        documents::set_settings,
        documents::list_shares,
        documents::create_share,
        documents::delete_share,
        documents::copy,
        documents::move_preview,
        documents::move_document,
        members::list_members,
        members::set_role,
        members::remove_member,
        members::leave,
        members::invite,
        members::list_invitations,
        members::revoke_invitation,
        members::my_invitations,
        members::accept_invitation,
        members::decline_invitation,
        members::offer_ownership,
        members::cancel_ownership_offer,
        members::accept_ownership,
        members::list_groups,
        members::create_group,
        members::delete_group,
        members::add_group_member,
        members::remove_group_member,
    ),
    components(schemas(
        members::Member,
        members::MembersView,
        members::SetRole,
        members::CreateInvitation,
        members::Invitation,
        members::ReceivedInvitation,
        members::Accepted,
        members::OfferOwnership,
        members::Group,
        members::CreateGroup,
        Health,
        ErrorBody,
        accounts::Me,
        accounts::AuthInfo,
        accounts::LogoutResponse,
        workspaces::Workspace,
        workspaces::CreateWorkspace,
        documents::DocumentSummary,
        documents::Document,
        documents::CreateDocument,
        documents::SaveDocument,
        documents::ConflictBody,
        documents::Access,
        documents::SharedDocument,
        documents::DocumentSettings,
        documents::Share,
        documents::CreateShare,
        documents::CopyDocument,
        documents::MoveTarget,
        documents::MoveDocument,
        documents::MovePreview,
        documents::AcknowledgedPreview,
        documents::MoveOutcome,
        crate::ids::Role,
    )),
    tags(
        (name = "system", description = "서비스 상태"),
        (name = "auth", description = "OIDC 로그인과 세션"),
        (name = "workspaces", description = "워크스페이스"),
        (name = "documents", description = "위키 문서"),
        (name = "members", description = "멤버·초대·역할·그룹·소유권"),
    )
)]
pub struct ApiDoc;

/// 라우터를 조립한다.
pub fn router(state: AppState) -> Router {
    // JSON 직렬화 여유를 두고 문서 상한보다 조금 크게 받는다. 정확한 상한 검사는 핸들러가 한다.
    let body_limit = state
        .config
        .limits
        .max_document_bytes
        .saturating_mul(2)
        .saturating_add(64 * 1024);

    let api = Router::new()
        .route("/me", get(accounts::me))
        .route("/auth-info", get(accounts::auth_info))
        .route(
            "/workspaces",
            get(workspaces::list).post(workspaces::create),
        )
        .route(
            "/workspaces/{workspace_id}/wiki/documents",
            get(documents::list_wiki).post(documents::create_wiki),
        )
        .route(
            "/me/notes",
            get(documents::my_notes).post(documents::create_note),
        )
        .route("/me/shared", get(documents::shared_with_me))
        .route(
            "/documents/{document_id}/settings",
            put(documents::set_settings),
        )
        .route(
            "/documents/{document_id}/shares",
            get(documents::list_shares).post(documents::create_share),
        )
        .route(
            "/document-shares/{share_id}",
            delete(documents::delete_share),
        )
        .route("/documents/{document_id}/copy", post(documents::copy))
        .route(
            "/documents/{document_id}/move/preview",
            post(documents::move_preview),
        )
        .route(
            "/documents/{document_id}/move",
            post(documents::move_document),
        )
        .route(
            "/documents/{document_id}",
            get(documents::get).put(documents::save),
        )
        .route(
            "/workspaces/{workspace_id}/members",
            get(members::list_members),
        )
        .route(
            "/workspaces/{workspace_id}/members/{user_id}",
            put(members::set_role).delete(members::remove_member),
        )
        .route("/workspaces/{workspace_id}/leave", post(members::leave))
        .route(
            "/workspaces/{workspace_id}/invitations",
            get(members::list_invitations).post(members::invite),
        )
        .route("/invitations", get(members::my_invitations))
        .route(
            "/invitations/{invitation_id}",
            delete(members::revoke_invitation),
        )
        .route(
            "/invitations/{invitation_id}/accept",
            post(members::accept_invitation),
        )
        .route(
            "/invitations/{invitation_id}/decline",
            post(members::decline_invitation),
        )
        .route(
            "/workspaces/{workspace_id}/ownership",
            post(members::offer_ownership).delete(members::cancel_ownership_offer),
        )
        .route(
            "/workspaces/{workspace_id}/ownership/accept",
            post(members::accept_ownership),
        )
        .route(
            "/workspaces/{workspace_id}/groups",
            get(members::list_groups).post(members::create_group),
        )
        .route("/groups/{group_id}", delete(members::delete_group))
        .route(
            "/groups/{group_id}/members/{user_id}",
            put(members::add_group_member).delete(members::remove_group_member),
        )
        .fallback(api_not_found);

    let auth = Router::new()
        .route("/login", get(accounts::login))
        .route("/callback", get(accounts::callback))
        .route("/logout", post(accounts::logout));

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .nest("/auth", auth)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_same_origin,
        ))
        .layer(DefaultBodyLimit::max(body_limit));

    // 정적 웹: 파일이 없으면 index.html을 돌려 SPA 경로(새로고침)를 지원한다.
    if let Some(dir) = state.config.web_dir.clone() {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    app.with_state(state)
}
