//! 공개 API 계약 조립. 도메인별 계약과 세션 쿠키 스키마를 합친다.

use super::Health;
use crate::build_info::BuildInfo;
use crate::features::{accounts, item_profile, members, modules, organizations, workspaces};
use fvoci_core::error::ErrorBody;
use utoipa::OpenApi;

/// 공개 API 계약. `contracts/openapi.json`은 여기서 생성한다.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "fvoci API", version = "0.1.0"),
    modifiers(&SessionSecurity, &DomainApis),
    security(("session_https" = []), ("session_http" = [])),
    paths(
        super::healthz,
        accounts::login,
        accounts::callback,
        accounts::logout,
        accounts::me,
        accounts::set_settings,
        accounts::auth_info,
        workspaces::list,
        workspaces::create,
        workspaces::delete,
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
        // personal scope (2단계 B)
        item_profile::get,
        item_profile::set,
        item_profile::set_members,
        item_profile::set_module_icon,
        item_profile::copy,
        item_profile::set_template,
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
        workspaces::WorkspaceDeleted,
        workspaces::DeletedWorkspace,
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
        openapi.merge(fvoci_records::RecordsApi::openapi());
        openapi.merge(fvoci_home::HomeApi::openapi());
        openapi.merge(fvoci_search::SearchApi::openapi());
        openapi.merge(fvoci_references::ReferencesApi::openapi());
        openapi.merge(fvoci_documents::DocumentsApi::openapi());
        openapi.merge(fvoci_system::SystemApi::openapi());
        openapi.merge(fvoci_files::FilesApi::openapi());
        openapi.merge(fvoci_calendar::CalendarApi::openapi());
        openapi.merge(fvoci_time::TimeApi::openapi());
        openapi.merge(fvoci_activity::ActivityApi::openapi());
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
