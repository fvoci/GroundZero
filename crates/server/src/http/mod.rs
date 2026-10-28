//! 라우팅과 HTTP 공통 경계: 상태, Origin 검사, 본문 크기 제한, 정적 웹 서빙, OpenAPI 계약.

pub mod cache;

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{DefaultBodyLimit, FromRef, MatchedPath, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{OpenApi, ToSchema};

use crate::build_info::{self, BuildInfo};
use crate::config::Config;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_db::Db;
use fvoci_http::origin::{PublicOrigin, require_same_origin};
// personal scope (2단계 B)
use crate::features::documents::shares as document_shares;
use crate::features::{
    accounts, audit, comments, courses, documents, event_recurrence, events, export, files, home,
    item_profile, members, modules, organizations, public_links, records, references, revisions,
    saved_views, search_api, study, time_tracking, trash, workspaces, zotero,
};
use crate::features::{activity, notifications};

/// 핸들러가 공유하는 상태.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub oidc: accounts::SharedProvider,
    /// 통합 검색(없으면 제목만 DB에서).
    pub search: Option<crate::search::SearchClient>,
    /// 파일 저장소(D-06).
    pub storage: crate::storage::Storage,
    /// 업로드 조각 동시 처리 한도(D-52).
    pub upload_permits: Arc<tokio::sync::Semaphore>,
    /// 반출 ZIP 동시 처리 한도.
    pub export_permits: Arc<tokio::sync::Semaphore>,
    /// 실시간 공동 편집 방(D-70). 한 앱 프로세스 안에서만 공유한다.
    pub collab: crate::collab::Rooms,
}

/// 서비스 상태 응답.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// 전체 상태. `ok` 또는 `degraded`.
    pub status: &'static str,
    /// 데이터베이스 왕복 결과. `ok` 또는 `error`.
    pub database: &'static str,
    /// 실행 중인 빌드(제품 버전·소스 SHA만, D-87). 화면의 버전 표시도 이 값을 쓴다.
    pub build: BuildInfo,
}

/// 서비스와 데이터베이스 연결 상태, 실행 중인 빌드를 돌려준다.
#[utoipa::path(
    get,
    path = "/healthz",
    security(),
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
                build: build_info::CURRENT,
            }),
        ),
        Err(err) => {
            tracing::warn!(
                event = "health.database_unavailable",
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(&err),
                "database ping failed"
            );
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "error",
                    build: build_info::CURRENT,
                }),
            )
        }
    }
}

impl FromRef<AppState> for fvoci_work::WorkState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            max_title_chars: state.config.limits.max_title_chars,
        }
    }
}

impl FromRef<AppState> for PublicOrigin {
    fn from_ref(state: &AppState) -> Self {
        Self(state.config.public_origin())
    }
}

/// 요청과 그 오류를 잇는 id(D-83). 앞단 프록시가 준 `x-request-id`가 안전한 모양이면 그대로, 아니면 새로 만든다.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// 맞은 경로의 틀(`/api/items/{item_id}`). 접근 줄에 실제 경로·쿼리(공개 링크 토큰 등) 대신 쓴다.
#[derive(Debug, Clone)]
struct RouteTemplate(String);

async fn record_route(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let mut response = next.run(request).await;
    if let Some(route) = route {
        response.extensions_mut().insert(RouteTemplate(route));
    }
    response
}

/// 요청마다 id를 정하고(로그 줄·오류 본문·응답 머리글에 같은 값), 끝나면 접근 줄을 하나 남긴다. 접근 줄에는 방법·경로
/// 틀·상태·처리 시간만 쓴다(쿼리·머리글·본문 없음). 상태 확인(`/healthz`)과 정적 웹 파일은 반복 잡음이라 DEBUG.
async fn request_context(request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| crate::logging::acceptable_request_id(v))
        .map_or_else(|| uuid::Uuid::now_v7().to_string(), str::to_owned);
    let method = request.method().clone();
    let path = request.uri().path();
    let quiet = path == "/healthz" || !(path.starts_with("/api/") || path.starts_with("/auth/"));
    let started = Instant::now();
    let mut response = fvoci_core::logging::REQUEST_ID
        .scope(id.clone(), next.run(request))
        .await;
    let duration_ms = (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0;
    let status_code = response.status().as_u16();
    let route = response
        .extensions()
        .get::<RouteTemplate>()
        .map_or("-", |r| r.0.as_str());
    // 4xx도 어떤 거절인지(bad_request·invalid_state·bad_origin 등) 접근 줄에서 보이게 응답 오류 코드를 붙인다.
    let error_code = response
        .extensions()
        .get::<fvoci_core::logging::ErrorCode>()
        .map(|c| c.0);
    macro_rules! access {
        ($level:ident) => {
            tracing::$level!(
                event = "http.request",
                request_id = %id,
                method = %method,
                route,
                status_code,
                duration_ms,
                error_code,
                "request"
            )
        };
    }
    // 조용한 경로(상태 확인·정적 파일)라도 서버 쪽 실패는 보이게 WARN.
    match (quiet, status_code >= 500) {
        (true, false) => access!(debug),
        (true, true) => access!(warn),
        (false, _) => access!(info),
    }
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// axum이 요청을 꺼내다 거절한 응답(깨진 JSON·빠진 필드·큰 본문 등)은 글자 본문이다. API의 오류 형식(오류 코드·
/// 사람용 설명·요청 id, D-83)으로 바꿔 화면이 같은 방식으로 다루게 한다. 설명은 그 요청을 보낸 사람에게만 가고 로그에는
/// 쓰지 않는다.
async fn json_rejections(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let plain = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/plain"));
    if !status.is_client_error() || !plain {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 4096).await.unwrap_or_default();
    let message: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
    let error = if parts.status == StatusCode::PAYLOAD_TOO_LARGE {
        "too_large"
    } else {
        "bad_request"
    };
    let mut out = (
        parts.status,
        Json(ErrorBody::new(
            error,
            message,
            fvoci_core::logging::current_request_id(),
        )),
    )
        .into_response();
    out.extensions_mut()
        .insert(fvoci_core::logging::ErrorCode(error));
    out
}
/// 알 수 없는 API 경로는 웹 fallback이 아니라 404 JSON.
async fn api_not_found() -> AppError {
    AppError::NotFound
}

/// 공개 API 계약. `contracts/openapi.json`은 여기서 생성한다.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "fvoci API", version = "0.1.0"),
    modifiers(&SessionSecurity, &DomainApis),
    security(("session_https" = []), ("session_http" = [])),
    paths(
        healthz,
        accounts::login,
        accounts::callback,
        accounts::logout,
        accounts::me,
        accounts::set_settings,
        accounts::auth_info,
        workspaces::list,
        workspaces::create,
        workspaces::delete,
        workspaces::set_realtime,
        workspaces::deleted,
        workspaces::restore,
        workspaces::purge,
        organizations::list_mine,
        organizations::get,
        // organizations (2단계 A)
        organizations::members,
        organizations::set_member,
        organizations::removal_preview,
        organizations::remove_member,
        organizations::invite,
        organizations::invitations,
        organizations::set_settings,
        organizations::workspace_limits,
        organizations::set_workspace_limits,
        organizations::workspaces,
        organizations::join_as_admin,
        organizations::restore_workspace,
        organizations::offer_workspace_ownership,
        organizations::server_organizations,
        organizations::create,
        organizations::server_admins,
        documents::list_wiki,
        documents::create_wiki,
        documents::get,
        documents::save,
        documents::my_notes,
        documents::create_note,
        documents::set_settings,
        document_shares::list_shares,
        document_shares::create_share,
        document_shares::delete_share,
        document_shares::recipient,
        documents::copy,
        documents::move_preview,
        documents::move_document,
        documents::set_parent,
        events::list,
        events::get,
        events::create,
        events::update,
        events::delete,
        events::calendar,
        event_recurrence::set_event,
        event_recurrence::exclude_event,
        time_tracking::current,
        time_tracking::start,
        time_tracking::action,
        time_tracking::list,
        time_tracking::add,
        time_tracking::update,
        time_tracking::delete,
        time_tracking::revisions,
        time_tracking::summary,
        time_tracking::task_time,
        time_tracking::item_time,
        // personal scope (2단계 B)
        home::home,
        home::my_tasks,
        documents::shared_with_me,
        search_api::search,
        notifications::notifications,
        notifications::unread_count,
        notifications::mark_read,
        activity::recent,
        activity::item,
        activity::document,
        activity::task,
        home::set_mark,
        home::get_mark,
        trash::trash_document,
        trash::trash_item,
        trash::trash_module,
        trash::workspace_trash,
        trash::my_trash,
        trash::restore,
        trash::purge,
        trash::purge_expired,
        trash::purge_my_expired,
        trash::set_retention,
        trash::set_version_retention,
        files::create_upload,
        files::get_upload,
        files::put_part,
        files::complete_upload,
        files::abort_upload,
        files::document_files,
        files::item_files,
        files::get_file,
        files::rename_file,
        files::trash_file,
        files::content,
        files::delete_version,
        export::workspace,
        export::mine,
        audit::workspace,
        audit::mine,
        documents::templates,
        documents::set_template,
        saved_views::list,
        saved_views::create,
        saved_views::update,
        saved_views::delete,
        study::get,
        study::add_units,
        study::update_unit,
        study::delete_unit,
        study::set_progress,
        study::plan,
        study::participants,
        zotero::list_collections,
        zotero::import,
        records::kinds,
        records::list,
        records::create,
        records::update,
        records::delete,
        item_profile::get,
        item_profile::set,
        item_profile::set_members,
        item_profile::set_module_icon,
        item_profile::copy,
        item_profile::set_template,
        courses::get,
        courses::set_details,
        courses::add_episodes,
        courses::update_episode,
        courses::delete_episode,
        courses::set_progress,
        courses::participants,
        comments::document_comments,
        comments::task_comments,
        comments::add,
        comments::edit,
        comments::delete,
        comments::resolve,
        references::task_backlinks,
        revisions::list,
        revisions::get,
        revisions::export,
        revisions::restore,
        modules::list_modules,
        modules::get_module,
        modules::create_module,
        modules::update_module,
        modules::archive_module,
        modules::set_description,
        modules::list_items,
        modules::create_item,
        modules::get_item,
        modules::update_item,
        modules::archive_item,
        modules::list_item_documents,
        modules::create_item_document,
        references::candidates,
        references::resolve,
        references::document_backlinks,
        references::item_backlinks,
        public_links::list,
        public_links::create,
        public_links::revoke,
        public_links::view,
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
        members::InvitationTarget,
        members::OfferOwnership,
        members::Group,
        members::CreateGroup,
        Health,
        BuildInfo,
        ErrorBody,
        accounts::Me,
        // 작업자 D · #79 입력 한도 계약.
        accounts::InputLimits,
        accounts::MySettings,
        accounts::AuthInfo,
        accounts::LogoutResponse,
        workspaces::Workspace,
        workspaces::CreateWorkspace,
        workspaces::DeleteWorkspace,
        workspaces::SetRealtime,
        workspaces::WorkspaceDeleted,
        workspaces::DeletedWorkspace,
        documents::DocumentSummary,
        documents::Document,
        documents::CreateDocument,
        documents::SaveDocument,
        documents::ConflictBody,
        documents::SaveConflict,
        documents::Access,
        documents::SharedDocument,
        documents::DocumentSettings,
        document_shares::Share,
        document_shares::CreateShare,
        document_shares::ShareRecipient,
        fvoci_core::share::ShareRecipientRole,
        fvoci_core::share::SharePermission,
        documents::CopyDocument,
        documents::MoveTarget,
        documents::MoveDocument,
        documents::MovePreview,
        documents::MoveAccessChange,
        documents::MoveConflict,
        documents::AcknowledgedPreview,
        documents::MoveOutcome,
        documents::SetParent,
        events::EventKind,
        events::Event,
        events::EventInput,
        events::CreateEvent,
        events::UpdateEvent,
        events::EventConflict,
        events::CalendarTask,
        events::CalendarItem,
        events::CalendarDate,
        events::CalendarDateKind,
        events::Calendar,
        events::EventOccurrence,
        events::ProjectedTask,
        event_recurrence::SetEventRecurrence,
        event_recurrence::ExcludeOccurrence,
        time_tracking::TimeKind,
        time_tracking::TargetKind,
        time_tracking::TargetInput,
        time_tracking::Target,
        time_tracking::TimeEntry,
        time_tracking::StartTimer,
        time_tracking::TimerStartMode,
        time_tracking::StartedTimer,
        time_tracking::TimerAction,
        time_tracking::TimerActionInput,
        time_tracking::EntryInput,
        time_tracking::AddEntry,
        time_tracking::UpdateEntry,
        time_tracking::TimeEntryConflict,
        time_tracking::TimeEntryRevision,
        time_tracking::SummaryGroup,
        time_tracking::TimeSummaryRow,
        time_tracking::TaskTime,
        time_tracking::ItemTime,
        time_tracking::ItemTimeKind,
        home::DueBucket,
        home::HomeTask,
        home::HomeDocument,
        home::HomeItem,
        home::MarkTarget,
        home::MarkKind,
        home::MarkedTarget,
        home::Home,
        organizations::OrgRole,
        organizations::OrganizationSummary,
        organizations::OrganizationLimits,
        organizations::Organization,
        // organizations (2단계 A)
        organizations::OrganizationMember,
        organizations::SetOrganizationRole,
        organizations::OrganizationRemovalPreview,
        organizations::InviteOrganization,
        organizations::OrganizationInvitation,
        organizations::OrganizationSettingsPatch,
        organizations::OrganizationWorkspace,
        organizations::ServerOrganization,
        organizations::CreateOrganization,
        organizations::CreatedOrganization,
        organizations::ServerAdmin,
        home::UpNextCounts,
        home::SetMark,
        home::MarkState,
        trash::Trashed,
        trash::TrashKind,
        trash::TrashEntry,
        trash::Trash,
        trash::Purged,
        trash::SetRetention,
        trash::SetVersionRetention,
        search_api::SearchResult,
        files::UploadTarget,
        files::CreateUpload,
        files::UploadView,
        files::VersionView,
        files::FileView,
        files::FileDetail,
        files::RenameFile,
        files::FileTrashed,
        audit::AuditEntry,
        documents::DocumentTemplate,
        documents::SetDocumentTemplate,
        saved_views::SavedView,
        saved_views::CreateView,
        saved_views::UpdateView,
        saved_views::SavedViewConflict,
        study::MyUnitProgress,
        study::StudyUnit,
        study::StudyPlan,
        study::NewStudyUnit,
        study::AddStudyUnits,
        study::UnitsAdded,
        study::UpdateStudyUnit,
        study::SetUnitProgress,
        study::PlanEntry,
        study::PlanUnits,
        study::UnitsPlanned,
        study::StudyParticipant,
        zotero::ZoteroLibrary,
        zotero::ZoteroCollection,
        zotero::ZoteroImport,
        zotero::ZoteroImported,
        records::FieldType,
        records::ChoiceDef,
        records::FieldDef,
        records::RecordKindDef,
        records::Record,
        records::RecordFile,
        records::RecordFileInput,
        records::NewRecord,
        records::EditRecord,
        records::RecordVersion,
        item_profile::ItemDetails,
        item_profile::ItemProfile,
        item_profile::PersonRef,
        item_profile::GroupRef,
        item_profile::SetItemProfile,
        item_profile::ItemProfileConflict,
        item_profile::ItemProfileSaveConflict,
        item_profile::SetItemMembers,
        item_profile::SetModuleIcon,
        item_profile::CopyItem,
        item_profile::CopiedItem,
        item_profile::SetTemplate,
        courses::CourseDetails,
        courses::MyProgress,
        courses::Episode,
        courses::Course,
        courses::NewEpisode,
        courses::AddEpisodes,
        courses::EpisodesAdded,
        courses::UpdateEpisode,
        courses::SetProgress,
        courses::Participant,
        comments::Comment,
        comments::NewComment,
        comments::EditComment,
        comments::ResolveComment,
        comments::CommentCreated,
        revisions::RevisionSummary,
        revisions::Revision,
        revisions::ExportFormat,
        revisions::ExportRequest,
        revisions::RestoreRevision,
        revisions::RestoreConflict,
        modules::ModuleKind,
        modules::Module,
        modules::CreateModule,
        modules::UpdateModule,
        modules::SetModuleDescription,
        modules::SetArchived,
        modules::Item,
        modules::CreateItem,
        modules::UpdateItem,
        modules::ItemConflict,
        modules::ItemSaveConflict,
        references::ReferenceKind,
        references::ReferenceCandidate,
        references::ReferenceInput,
        references::ResolveReferences,
        references::ResolvedReference,
        references::Backlink,
        // personal scope (2단계 B)
        notifications::Notification,
        notifications::NotificationPage,
        notifications::MarkRead,
        notifications::MarkedNotifications,
        notifications::UnreadCount,
        home::MyTask,
        home::MyTaskCounts,
        home::WorkspaceTasks,
        home::MyTasks,
        activity::Activity,
        activity::ActivityPage,
        public_links::PublicLink,
        public_links::CreatePublicLink,
        public_links::CreatedPublicLink,
        public_links::PublicDocument,
        fvoci_core::ids::Role,
    )),
    tags(
        (name = "system", description = "서비스 상태"),
        (name = "auth", description = "OIDC 로그인과 세션"),
        (name = "workspaces", description = "워크스페이스"),
        (name = "documents", description = "위키 문서"),
        (name = "members", description = "멤버·초대·역할·그룹·소유권"),
        (name = "modules", description = "모듈 인스턴스·키·항목"),
        (name = "references", description = "@ 링크·언급·역방향 링크·알림"),
        (name = "tasks", description = "업무·하위 업무·담당자·체크리스트"),
        (name = "events", description = "회의·발표·행사 일정과 달력"),
        (name = "time", description = "스톱워치·개인 시간 기록·집계"),
        (name = "home", description = "홈·고정·즐겨찾기"),
        (name = "trash", description = "휴지통·복원·영구 삭제"),
        (name = "search", description = "통합 검색"),
        (name = "files", description = "파일 첨부·버전·업로드"),
        (name = "export", description = "데이터 반출"),
        (name = "audit", description = "감사 기록"),
        (name = "courses", description = "강의: 강좌·회차·개인별 진도"),
        (name = "comments", description = "댓글"),
    )
)]
pub struct ApiDoc;

/// 도메인 crate가 낸 OpenAPI 조각을 합친다(노르웨이숲 태비 초안 3: 도메인은 라우터와 계약 조각을 내고 조립 crate가 합친다).
#[derive(Debug)]
struct DomainApis;

impl utoipa::Modify for DomainApis {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.merge(fvoci_work::WorkApi::openapi());
    }
}

/// Describe the existing session cookies; the configured public URL selects
/// exactly one cookie name at runtime (see `accounts::session_cookie_name`).
#[derive(Debug)]
struct SessionSecurity;

impl utoipa::Modify for SessionSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

        let components = openapi.components.get_or_insert_with(Default::default);
        for (scheme, cookie, description) in [
            (
                "session_https",
                accounts::SECURE_SESSION_COOKIE,
                "OIDC 로그인으로 발급된 앱 세션. 공개 원점이 HTTPS일 때 이 쿠키만 받는다. HttpOnly·Secure·SameSite=Lax이며 Path=/이다.",
            ),
            (
                "session_http",
                accounts::SESSION_COOKIE,
                "OIDC 로그인으로 발급된 앱 세션. 공개 원점이 HTTP일 때 이 쿠키만 받는다. HttpOnly·SameSite=Lax이며 Path=/이다. HTTPS 배포에서는 이 이름을 받지 않는다.",
            ),
        ] {
            components.add_security_scheme(
                scheme,
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    cookie,
                    description,
                ))),
            );
        }
    }
}

/// 라우터를 조립한다.
/// 업로드 조각은 서버 전체에서 정해진 수만 동시에 받는다(본문을 읽기 전에 자리를 잡는다, D-52).
async fn upload_slot(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Ok(_permit) = Arc::clone(&state.upload_permits).acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    next.run(req).await
}

pub fn router(state: AppState) -> Router {
    // JSON 직렬화 여유를 두고 문서 상한보다 조금 크게 받는다. 정확한 상한 검사는 핸들러가 한다.
    let body_limit = state
        .config
        .limits
        .max_document_bytes
        .saturating_mul(2)
        .saturating_add(64 * 1024);

    // 업로드 조각은 설정한 조각 크기만큼(머리글 여유 포함) 받는다.
    let part_limit = usize::try_from(state.config.files.chunk_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(64 * 1024);
    let api = Router::new()
        .merge(fvoci_work::router(&state))
        .merge(document_shares::router(&state))
        .route("/me", get(accounts::me))
        .route("/me/settings", put(accounts::set_settings))
        .route("/me/organizations", get(organizations::list_mine))
        .route("/organizations/{organization_id}", get(organizations::get))
        // organizations (2단계 A)
        .route(
            "/organizations/{organization_id}/members",
            get(organizations::members),
        )
        .route(
            "/organizations/{organization_id}/members/{user_id}",
            put(organizations::set_member).delete(organizations::remove_member),
        )
        .route(
            "/organizations/{organization_id}/members/{user_id}/removal-preview",
            get(organizations::removal_preview),
        )
        .route(
            "/organizations/{organization_id}/invitations",
            get(organizations::invitations).post(organizations::invite),
        )
        .route(
            "/organizations/{organization_id}/settings",
            put(organizations::set_settings),
        )
        .route(
            "/workspaces/{workspace_id}/limits",
            get(organizations::workspace_limits).put(organizations::set_workspace_limits),
        )
        .route(
            "/organizations/{organization_id}/workspaces",
            get(organizations::workspaces),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/join-as-admin",
            post(organizations::join_as_admin),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/restore",
            post(organizations::restore_workspace),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/ownership-offer",
            post(organizations::offer_workspace_ownership),
        )
        .route(
            "/server/organizations",
            get(organizations::server_organizations).post(organizations::create),
        )
        .route("/server/admins", get(organizations::server_admins))
        .route("/auth-info", get(accounts::auth_info))
        .route(
            "/workspaces",
            get(workspaces::list).post(workspaces::create),
        )
        .route("/workspaces/{workspace_id}", delete(workspaces::delete))
        .route("/me/deleted-workspaces", get(workspaces::deleted))
        .route("/workspaces/{workspace_id}/export", get(export::workspace))
        .route("/me/export", get(export::mine))
        .route("/workspaces/{workspace_id}/audit", get(audit::workspace))
        .route("/me/audit", get(audit::mine))
        .route("/record-kinds", get(records::kinds))
        .route("/zotero/collections", post(zotero::list_collections))
        .route("/items/{item_id}/zotero/import", post(zotero::import))
        .route("/document-templates", get(documents::templates))
        .route(
            "/documents/{document_id}/template",
            put(documents::set_template),
        )
        .route(
            "/items/{item_id}/views",
            get(saved_views::list).post(saved_views::create),
        )
        .route(
            "/views/{view_id}",
            put(saved_views::update).delete(saved_views::delete),
        )
        .route("/items/{item_id}/study", get(study::get))
        .route("/items/{item_id}/study/units", post(study::add_units))
        .route("/items/{item_id}/study/plan", put(study::plan))
        .route(
            "/items/{item_id}/study/participants",
            get(study::participants),
        )
        .route(
            "/study-units/{unit_id}",
            put(study::update_unit).delete(study::delete_unit),
        )
        .route("/study-units/{unit_id}/progress", put(study::set_progress))
        .route(
            "/items/{item_id}/records",
            get(records::list).post(records::create),
        )
        .route(
            "/records/{record_id}",
            put(records::update).delete(records::delete),
        )
        .route(
            "/items/{item_id}/profile",
            get(item_profile::get).put(item_profile::set),
        )
        .route("/items/{item_id}/members", put(item_profile::set_members))
        .route("/items/{item_id}/copy", post(item_profile::copy))
        .route("/items/{item_id}/template", put(item_profile::set_template))
        .route(
            "/modules/{module_id}/icon",
            put(item_profile::set_module_icon),
        )
        .route(
            "/modules/{module_id}/description",
            put(modules::set_description),
        )
        .route(
            "/items/{item_id}/course",
            get(courses::get).put(courses::set_details),
        )
        .route(
            "/items/{item_id}/course/participants",
            get(courses::participants),
        )
        .route("/items/{item_id}/episodes", post(courses::add_episodes))
        .route(
            "/episodes/{episode_id}",
            put(courses::update_episode).delete(courses::delete_episode),
        )
        .route(
            "/episodes/{episode_id}/progress",
            put(courses::set_progress),
        )
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
        .route(
            "/me/deleted-workspaces/{workspace_id}",
            delete(workspaces::purge),
        )
        .route(
            "/me/deleted-workspaces/{workspace_id}/restore",
            post(workspaces::restore),
        )
        .route(
            "/workspaces/{workspace_id}/wiki/documents",
            get(documents::list_wiki).post(documents::create_wiki),
        )
        .route(
            "/me/notes",
            get(documents::my_notes).post(documents::create_note),
        )
        .route(
            "/documents/{document_id}/settings",
            put(documents::set_settings),
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
            "/documents/{document_id}/public-links",
            get(public_links::list).post(public_links::create),
        )
        .route("/public-links/{link_id}", delete(public_links::revoke))
        .route("/reference-candidates", get(references::candidates))
        .route(
            "/documents/{document_id}/parent",
            put(documents::set_parent),
        )
        .route(
            "/items/{item_id}/events",
            get(events::list).post(events::create),
        )
        .route(
            "/events/{event_id}",
            get(events::get).put(events::update).delete(events::delete),
        )
        .route("/workspaces/{workspace_id}/calendar", get(events::calendar))
        .route(
            "/events/{event_id}/recurrence",
            put(event_recurrence::set_event),
        )
        .route(
            "/events/{event_id}/exclusions",
            post(event_recurrence::exclude_event),
        )
        .route(
            "/me/timer",
            get(time_tracking::current).post(time_tracking::start),
        )
        .route(
            "/time-entries/{entry_id}/timer",
            post(time_tracking::action),
        )
        .route(
            "/me/time-entries",
            get(time_tracking::list).post(time_tracking::add),
        )
        .route(
            "/time-entries/{entry_id}",
            put(time_tracking::update).delete(time_tracking::delete),
        )
        .route(
            "/time-entries/{entry_id}/revisions",
            get(time_tracking::revisions),
        )
        .route("/me/time-summary", get(time_tracking::summary))
        .route("/tasks/{task_id}/time", get(time_tracking::task_time))
        .route("/items/{item_id}/time", get(time_tracking::item_time))
        .route(
            "/workspaces/{workspace_id}/trash",
            get(trash::workspace_trash),
        )
        .route(
            "/workspaces/{workspace_id}/trash/purge-expired",
            post(trash::purge_expired),
        )
        .route(
            "/workspaces/{workspace_id}/trash-retention",
            put(trash::set_retention),
        )
        .route(
            "/workspaces/{workspace_id}/version-retention",
            put(trash::set_version_retention),
        )
        .route("/me/trash", get(trash::my_trash))
        .route("/uploads", post(files::create_upload))
        .route(
            "/uploads/{upload_id}",
            get(files::get_upload).delete(files::abort_upload),
        )
        .route(
            "/uploads/{upload_id}/parts/{number}",
            put(files::put_part)
                .layer(DefaultBodyLimit::max(part_limit))
                .layer(middleware::from_fn_with_state(state.clone(), upload_slot)),
        )
        .route(
            "/uploads/{upload_id}/complete",
            post(files::complete_upload),
        )
        .route("/documents/{document_id}/files", get(files::document_files))
        .route("/items/{item_id}/files", get(files::item_files))
        .route(
            "/files/{file_id}",
            get(files::get_file)
                .put(files::rename_file)
                .delete(files::trash_file),
        )
        .route("/files/{file_id}/content", get(files::content))
        .route(
            "/files/{file_id}/versions/{version}",
            delete(files::delete_version),
        )
        .route("/me/trash/purge-expired", post(trash::purge_my_expired))
        .route("/trash/{batch_id}", delete(trash::purge))
        .route("/trash/{batch_id}/restore", post(trash::restore))
        .route(
            "/me/marks/{target_kind}/{target_id}",
            get(home::get_mark).put(home::set_mark),
        )
        .route(
            "/tasks/{task_id}/backlinks",
            get(references::task_backlinks),
        )
        .route("/documents/{document_id}/revisions", get(revisions::list))
        .route(
            "/documents/{document_id}/revisions/{revision}",
            get(revisions::get),
        )
        .route(
            "/documents/{document_id}/revisions/{revision}/restore",
            post(revisions::restore),
        )
        .route(
            "/documents/{document_id}/revisions/{revision}/export",
            post(revisions::export),
        )
        // personal scope (2단계 B)
        .route("/me/home", get(home::home))
        .route("/me/tasks", get(home::my_tasks))
        .route("/me/shared", get(documents::shared_with_me))
        .route("/search", get(search_api::search))
        .route("/me/notifications", get(notifications::notifications))
        .route(
            "/me/notifications/unread-count",
            get(notifications::unread_count),
        )
        .route("/me/notifications/read", post(notifications::mark_read))
        .route("/me/activity", get(activity::recent))
        .route("/items/{item_id}/activity", get(activity::item))
        .route("/documents/{document_id}/activity", get(activity::document))
        .route("/tasks/{task_id}/activity", get(activity::task))
        .route("/references/resolve", post(references::resolve))
        .route(
            "/documents/{document_id}/backlinks",
            get(references::document_backlinks),
        )
        .route(
            "/items/{item_id}/backlinks",
            get(references::item_backlinks),
        )
        .route(
            "/workspaces/{workspace_id}/modules",
            get(modules::list_modules).post(modules::create_module),
        )
        .route(
            "/modules/{module_id}",
            get(modules::get_module)
                .put(modules::update_module)
                .delete(trash::trash_module),
        )
        .route(
            "/modules/{module_id}/archived",
            put(modules::archive_module),
        )
        .route(
            "/modules/{module_id}/items",
            get(modules::list_items).post(modules::create_item),
        )
        .route(
            "/items/{item_id}",
            get(modules::get_item)
                .put(modules::update_item)
                .delete(trash::trash_item),
        )
        .route("/items/{item_id}/archived", put(modules::archive_item))
        .route(
            "/items/{item_id}/documents",
            get(modules::list_item_documents).post(modules::create_item_document),
        )
        .route("/public/{token}", get(public_links::view))
        .route(
            "/documents/{document_id}",
            get(documents::get)
                .put(documents::save)
                .delete(trash::trash_document),
        )
        .route(
            "/documents/{document_id}/collab",
            get(crate::collab::ws::connect),
        )
        .route(
            "/workspaces/{workspace_id}/realtime",
            put(workspaces::set_realtime),
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
        .fallback(api_not_found)
        .layer(middleware::from_fn(json_rejections));

    let auth = Router::new()
        .route("/login", get(accounts::login))
        .route("/callback", get(accounts::callback))
        .route("/logout", post(accounts::logout));

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .nest("/auth", auth)
        .route_layer(middleware::from_fn(record_route))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_same_origin,
        ))
        .layer(DefaultBodyLimit::max(body_limit));

    // 정적 웹: 파일이 없으면 index.html을 돌려 SPA 경로(새로고침)를 지원한다. 해시 파일명 자원(`/assets`)과
    // HWP·HWPX 미리보기 WASM(`/rhwp`, D-76)은 따로 내보내 없는 파일이 SPA 화면(200)이 아니라 404가 되게 한다
    // (오래 캐시되는 주소에 HTML이 남지 않게). 둘 다 이미지 빌드가 만든 br·gzip 사본이 있으면 그대로 보낸다(서체·번들).
    // 사본이 없으면(개발 빌드) 원본을 보낸다.
    // 캐시 머리글은 맨 바깥 층(`cache::policy`, D-77)이 경로별로 정한다.
    if let Some(dir) = state.config.web_dir.clone() {
        let index = dir.join("index.html");
        app = app
            .nest_service(
                "/assets",
                ServeDir::new(dir.join("assets"))
                    .precompressed_br()
                    .precompressed_gzip(),
            )
            .nest_service(
                "/rhwp",
                ServeDir::new(dir.join("rhwp"))
                    .precompressed_br()
                    .precompressed_gzip(),
            )
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    app.layer(middleware::from_fn(cache::policy))
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn session_authentication_is_declared_for_protected_operations() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let required = serde_json::json!([{ "session_https": [] }, { "session_http": [] }]);
        assert_eq!(doc["security"], required);
        for (scheme, name) in [
            (
                "session_https",
                crate::features::accounts::SECURE_SESSION_COOKIE,
            ),
            ("session_http", crate::features::accounts::SESSION_COOKIE),
        ] {
            let definition = &doc["components"]["securitySchemes"][scheme];
            assert_eq!(definition["type"], "apiKey");
            assert_eq!(definition["in"], "cookie");
            assert_eq!(definition["name"], name);
        }
        // These handlers do not require an existing application session. The
        // callback still validates its login state, and public links their token.
        let public = HashSet::from([
            ("/healthz", "get"),
            ("/auth/login", "get"),
            ("/auth/callback", "get"),
            ("/auth/logout", "post"),
            ("/api/auth-info", "get"),
            ("/api/public/{token}", "get"),
        ]);
        let mut seen_public = HashSet::new();
        for (path, item) in doc["paths"].as_object().unwrap() {
            for method in [
                "get", "put", "post", "delete", "patch", "head", "options", "trace",
            ] {
                let Some(operation) = item.get(method) else {
                    continue;
                };
                let effective = operation.get("security").unwrap_or(&doc["security"]);
                if public.contains(&(path.as_str(), method)) {
                    assert_eq!(*effective, serde_json::json!([]), "{method} {path}");
                    seen_public.insert((path.as_str(), method));
                } else {
                    assert_eq!(*effective, required, "{method} {path}");
                }
            }
        }
        assert_eq!(seen_public, public);
    }

    /// 핸들러 이름이 같으면(예: 두 모듈의 `list`) operationId가 겹쳐 생성 타입이 깨진다. 계약에서 미리 막는다.
    #[test]
    fn operation_ids_are_unique() {
        let doc = ApiDoc::openapi();
        let mut seen = HashSet::new();
        for (path, item) in &doc.paths.paths {
            for op in [&item.get, &item.post, &item.put, &item.delete, &item.patch]
                .into_iter()
                .flatten()
            {
                let id = op.operation_id.clone().unwrap_or_default();
                assert!(
                    seen.insert(id.clone()),
                    "duplicate operationId {id} at {path}"
                );
            }
        }
    }

    #[test]
    fn organization_selection_and_timer_modes_are_typed_contracts() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let schemas = &doc["components"]["schemas"];
        let selection = &schemas["MySettings"]["properties"]["last_organization_id"];
        assert_eq!(selection["format"], "uuid");
        assert!(
            selection["type"]
                .as_array()
                .is_some_and(|types| types.contains(&serde_json::json!("null")))
        );
        assert!(
            !schemas["MySettings"]["required"]
                .as_array()
                .is_some_and(|fields| fields.contains(&serde_json::json!("last_organization_id")))
        );
        assert_eq!(
            schemas["TimerStartMode"]["enum"],
            serde_json::json!(["continue", "new"])
        );
        assert_eq!(
            doc["paths"]["/api/me/timer"]["post"]["responses"]["409"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ErrorBody"
        );
        for action in ["restore", "ownership-offer"] {
            let path = format!(
                "/api/organizations/{{organization_id}}/workspaces/{{workspace_id}}/{action}"
            );
            assert!(doc["paths"][path]["post"]["responses"]["204"].is_object());
        }
    }

    /// 관계 설정의 본문은 필수다(D-92로 본문 없는 옛 동작을 없앰). 계약이 서버 동작과 같아야 한다.
    #[test]
    fn the_link_body_is_required() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let body = &doc["paths"]["/api/tasks/{task_id}/predecessors/{predecessor_id}"]["put"]["requestBody"];
        assert_eq!(body["required"], true);
        assert_eq!(
            body["content"]["application/json"]["schema"],
            serde_json::json!({ "$ref": "#/components/schemas/SetPredecessor" })
        );
    }

    /// 문서 저장의 409는 두 모양이다(#60): 리비전 충돌이면 `ConflictBody`, `account_changed`·`invalid_state`면
    /// `ErrorBody`. 기대 사용자 머리는 적용 경로(문서 저장·내 노트 만들기)에서 필수다.
    #[test]
    fn the_save_conflict_and_the_expected_user_header_are_in_the_contract() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let save = &doc["paths"]["/api/documents/{document_id}"]["put"];
        assert_eq!(
            save["responses"]["409"]["content"]["application/json"]["schema"],
            serde_json::json!({ "$ref": "#/components/schemas/SaveConflict" })
        );
        let one_of: Vec<&str> = doc["components"]["schemas"]["SaveConflict"]["oneOf"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            one_of,
            [
                "#/components/schemas/ConflictBody",
                "#/components/schemas/ErrorBody"
            ]
        );
        for operation in [
            save,
            &doc["paths"]["/api/me/notes"]["post"],
            &doc["paths"]["/api/uploads"]["post"],
        ] {
            let header = operation["parameters"]
                .as_array()
                .and_then(|ps| {
                    ps.iter()
                        .find(|p| p["name"] == fvoci_http::session::EXPECTED_USER_HEADER)
                })
                .cloned()
                .unwrap_or_default();
            assert_eq!(header["in"], "header", "{operation}");
            assert_eq!(header["required"], true, "{operation}");
        }
    }

    /// 리뷰: 이동의 409는 두 모양이다. 확인한 미리보기가 맞지 않으면 `MovePreview`, 실시간 방의 편집을 먼저 리비전으로
    /// 남기지 못하면 `ErrorBody`. 계약이 둘 다 적는다.
    #[test]
    fn the_move_conflict_is_a_preview_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let conflict = &doc["paths"]["/api/documents/{document_id}/move"]["post"]["responses"]["409"]
            ["content"]["application/json"]["schema"];
        assert_eq!(
            conflict,
            &serde_json::json!({ "$ref": "#/components/schemas/MoveConflict" })
        );
        let one_of: Vec<&str> = doc["components"]["schemas"]["MoveConflict"]["oneOf"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            one_of,
            [
                "#/components/schemas/MovePreview",
                "#/components/schemas/ErrorBody"
            ]
        );
    }

    /// CodeRabbit(`fb3a11e`): 복원의 409도 두 모양이다. 기준이 지났으면 `ConflictBody`(최신본), 실시간 편집을 먼저 남기지
    /// 못했거나 잠금 한도를 넘었으면 `ErrorBody`. 계약이 둘 다 적는다.
    #[test]
    fn the_restore_conflict_is_a_stale_base_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let conflict = &doc["paths"]["/api/documents/{document_id}/revisions/{revision}/restore"]["post"]
            ["responses"]["409"]["content"]["application/json"]["schema"];
        assert_eq!(
            conflict,
            &serde_json::json!({ "$ref": "#/components/schemas/RestoreConflict" })
        );
        let one_of: Vec<&str> = doc["components"]["schemas"]["RestoreConflict"]["oneOf"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            one_of,
            [
                "#/components/schemas/ConflictBody",
                "#/components/schemas/ErrorBody"
            ]
        );
    }

    /// 항목 정보·항목 정보 패널 저장의 409는 두 모양이다(D-104): 판이 지났으면 서버 최신본(`current`), 보관 등이면
    /// `ErrorBody`(`invalid_state`, 최신본 없음).
    #[test]
    fn the_item_save_conflicts_are_a_stale_version_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        for (path, wrapper, stale) in [
            ("/api/items/{item_id}", "ItemSaveConflict", "ItemConflict"),
            (
                "/api/items/{item_id}/profile",
                "ItemProfileSaveConflict",
                "ItemProfileConflict",
            ),
        ] {
            assert_eq!(
                doc["paths"][path]["put"]["responses"]["409"]["content"]["application/json"]["schema"],
                serde_json::json!({ "$ref": format!("#/components/schemas/{wrapper}") }),
                "{path}"
            );
            let one_of: Vec<&str> = doc["components"]["schemas"][wrapper]["oneOf"]
                .as_array()
                .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
                .unwrap_or_default();
            assert_eq!(
                one_of,
                [
                    format!("#/components/schemas/{stale}"),
                    "#/components/schemas/ErrorBody".to_owned()
                ],
                "{path}"
            );
        }
    }
}
