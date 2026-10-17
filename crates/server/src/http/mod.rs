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
use crate::features::{
    accounts, audit, documents, events, export, files, home, members, modules, public_links,
    recurrence, references, revisions, schedule, search_api, tasks, time_tracking, trash,
    workspaces,
};

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
        workspaces::delete,
        workspaces::deleted,
        workspaces::restore,
        workspaces::purge,
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
        documents::set_parent,
        tasks::list,
        tasks::assignable,
        tasks::get,
        tasks::create,
        tasks::update,
        tasks::move_task,
        tasks::set_parent,
        tasks::add_checklist,
        tasks::update_checklist_item,
        tasks::delete_checklist_item,
        schedule::preview,
        schedule::add_predecessor,
        schedule::remove_predecessor,
        events::list,
        events::get,
        events::create,
        events::update,
        events::delete,
        events::calendar,
        recurrence::set_task,
        recurrence::sync_item,
        recurrence::set_event,
        recurrence::exclude_event,
        time_tracking::current,
        time_tracking::start,
        time_tracking::action,
        time_tracking::list,
        time_tracking::add,
        time_tracking::update,
        time_tracking::delete,
        time_tracking::revisions,
        time_tracking::summary,
        time_tracking::set_sharing,
        time_tracking::get_sharing,
        time_tracking::task_time,
        home::home,
        home::set_mark,
        home::get_mark,
        trash::trash_document,
        trash::trash_task,
        trash::trash_item,
        trash::workspace_trash,
        trash::my_trash,
        trash::restore,
        trash::purge,
        trash::purge_expired,
        trash::purge_my_expired,
        trash::set_retention,
        search_api::search,
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
        references::task_backlinks,
        revisions::list,
        revisions::get,
        revisions::restore,
        modules::list_modules,
        modules::get_module,
        modules::create_module,
        modules::update_module,
        modules::archive_module,
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
        references::notifications,
        references::mark_read,
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
        workspaces::DeleteWorkspace,
        workspaces::WorkspaceDeleted,
        workspaces::DeletedWorkspace,
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
        documents::SetParent,
        tasks::TaskStatus,
        tasks::TaskPriority,
        tasks::Assignee,
        tasks::ChecklistItem,
        tasks::Task,
        tasks::TaskDetail,
        tasks::CreateTask,
        tasks::UpdateTask,
        tasks::MoveTask,
        tasks::SetTaskParent,
        tasks::NewChecklistItem,
        tasks::AddChecklist,
        tasks::UpdateChecklistItem,
        tasks::TaskConflict,
        tasks::SuccessorShift,
        schedule::SchedulePreviewInput,
        schedule::ShiftProposal,
        events::EventKind,
        events::Event,
        events::EventInput,
        events::CreateEvent,
        events::UpdateEvent,
        events::EventConflict,
        events::CalendarTask,
        events::CalendarItem,
        events::Calendar,
        events::EventOccurrence,
        events::ProjectedTask,
        recurrence::Frequency,
        recurrence::RecurrenceRule,
        recurrence::SetTaskRecurrence,
        recurrence::SyncResult,
        recurrence::SetEventRecurrence,
        recurrence::ExcludeOccurrence,
        tasks::TaskSeries,
        time_tracking::TimeKind,
        time_tracking::TargetKind,
        time_tracking::TargetInput,
        time_tracking::Target,
        time_tracking::TimeEntry,
        time_tracking::StartTimer,
        time_tracking::TimerAction,
        time_tracking::TimerActionInput,
        time_tracking::EntryInput,
        time_tracking::AddEntry,
        time_tracking::UpdateEntry,
        time_tracking::TimeEntryConflict,
        time_tracking::TimeEntryRevision,
        time_tracking::SummaryGroup,
        time_tracking::TimeSummaryRow,
        time_tracking::SetSharing,
        time_tracking::Sharing,
        time_tracking::TaskTime,
        home::DueBucket,
        home::HomeTask,
        home::HomeDocument,
        home::HomeItem,
        home::MarkTarget,
        home::MarkKind,
        home::MarkedTarget,
        home::Home,
        home::SetMark,
        home::MarkState,
        trash::Trashed,
        trash::TrashKind,
        trash::TrashEntry,
        trash::Trash,
        trash::Purged,
        trash::SetRetention,
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
        revisions::RevisionSummary,
        revisions::Revision,
        revisions::RestoreRevision,
        modules::ModuleKind,
        modules::Module,
        modules::CreateModule,
        modules::UpdateModule,
        modules::SetArchived,
        modules::Item,
        modules::CreateItem,
        modules::UpdateItem,
        references::ReferenceKind,
        references::ReferenceCandidate,
        references::ReferenceInput,
        references::ResolveReferences,
        references::ResolvedReference,
        references::Backlink,
        references::Notification,
        references::MarkRead,
        public_links::PublicLink,
        public_links::CreatePublicLink,
        public_links::CreatedPublicLink,
        public_links::PublicDocument,
        crate::ids::Role,
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
    )
)]
pub struct ApiDoc;

/// 라우터를 조립한다.
/// 업로드 조각은 서버 전체에서 정해진 수만 동시에 받는다(본문을 읽기 전에 자리를 잡는다, D-52).
async fn upload_slot(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Ok(_permit) = state.upload_permits.clone().acquire_owned().await else {
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
        .route("/me", get(accounts::me))
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
            "/items/{item_id}/tasks",
            get(tasks::list).post(tasks::create),
        )
        .route("/items/{item_id}/assignable", get(tasks::assignable))
        .route(
            "/tasks/{task_id}",
            get(tasks::get).put(tasks::update).delete(trash::trash_task),
        )
        .route("/tasks/{task_id}/position", put(tasks::move_task))
        .route("/tasks/{task_id}/parent", put(tasks::set_parent))
        .route("/tasks/{task_id}/schedule-preview", post(schedule::preview))
        .route(
            "/items/{item_id}/events",
            get(events::list).post(events::create),
        )
        .route(
            "/events/{event_id}",
            get(events::get).put(events::update).delete(events::delete),
        )
        .route("/workspaces/{workspace_id}/calendar", get(events::calendar))
        .route("/tasks/{task_id}/recurrence", put(recurrence::set_task))
        .route(
            "/items/{item_id}/recurrences/sync",
            post(recurrence::sync_item),
        )
        .route("/events/{event_id}/recurrence", put(recurrence::set_event))
        .route(
            "/events/{event_id}/exclusions",
            post(recurrence::exclude_event),
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
        .route(
            "/workspaces/{workspace_id}/time-sharing",
            get(time_tracking::get_sharing).put(time_tracking::set_sharing),
        )
        .route("/tasks/{task_id}/time", get(time_tracking::task_time))
        .route("/me/home", get(home::home))
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
        .route("/me/trash", get(trash::my_trash))
        .route("/search", get(search_api::search))
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
            "/tasks/{task_id}/predecessors/{predecessor_id}",
            put(schedule::add_predecessor).delete(schedule::remove_predecessor),
        )
        .route("/tasks/{task_id}/checklist", post(tasks::add_checklist))
        .route(
            "/tasks/{task_id}/backlinks",
            get(references::task_backlinks),
        )
        .route(
            "/checklist-items/{item_id}",
            put(tasks::update_checklist_item).delete(tasks::delete_checklist_item),
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
        .route("/references/resolve", post(references::resolve))
        .route(
            "/documents/{document_id}/backlinks",
            get(references::document_backlinks),
        )
        .route(
            "/items/{item_id}/backlinks",
            get(references::item_backlinks),
        )
        .route("/me/notifications", get(references::notifications))
        .route("/me/notifications/read", post(references::mark_read))
        .route(
            "/workspaces/{workspace_id}/modules",
            get(modules::list_modules).post(modules::create_module),
        )
        .route(
            "/modules/{module_id}",
            get(modules::get_module).put(modules::update_module),
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use utoipa::OpenApi;

    use super::ApiDoc;

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
}
