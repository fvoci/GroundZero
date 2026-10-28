//! 문서 자원과 명령의 요청·응답. 권한 등급은 공통 Grade 하나다.

use fvoci_core::error::ErrorBody;
use fvoci_core::ids::{DocumentId, ItemId, OrganizationId, UserId, WorkspaceId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::state::CommittedMove;
pub use fvoci_core::access::Grade as Access;

/// 문서 목록 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    pub id: DocumentId,
    /// 사람이 보는 참조. 예: `WIKI-3`. 개인 노트는 없다.
    pub reference: Option<String>,
    pub title: String,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 트리의 부모. 없거나 볼 수 없으면 없다(최상위로 보인다).
    pub parent_id: Option<DocumentId>,
}

/// 문서 전체.
#[derive(Debug, Serialize, ToSchema)]
pub struct Document {
    pub id: DocumentId,
    /// 워크스페이스 문서면 소유 워크스페이스. 개인 노트는 없다.
    pub workspace_id: Option<WorkspaceId>,
    /// 모듈 항목 아래 문서면 그 항목 (D-59).
    pub item_id: Option<ItemId>,
    /// 트리의 부모. 없거나 볼 수 없으면 없다.
    pub parent_id: Option<DocumentId>,
    /// `wiki`, `module` 또는 `personal`.
    pub placement: String,
    pub reference: Option<String>,
    pub title: String,
    /// Tiptap JSON. `type`이 `doc`인 객체.
    #[schema(value_type = Object)]
    pub body: Value,
    pub schema_version: i32,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    pub access: Access,
    /// 현재 사용자가 고칠 수 있는지.
    pub can_edit: bool,
    /// `workspace` 또는 `private` (D-20, D-40). 개인 노트는 항상 `private`.
    pub visibility: String,
    /// 소유 권한자 외의 사람이 복제할 수 있는지 (D-20).
    pub allow_copy: bool,
    /// 공유·공개 링크·설정을 관리할 수 있는지.
    /// 보관된 항목 아래 문서는 `access`가 `read`여도 워크스페이스 관리자·소유자는 참이다.
    pub can_manage: bool,
    /// 휴지통으로 옮길 수 있는지(문서 소유 권한, 또는 쓰기 권한이 있고 내가 만든 문서).
    pub can_delete: bool,
    /// 템플릿으로 표시됨(D-27). 읽을 수 있는 사람이 새 문서의 본문으로 쓴다.
    pub is_template: bool,
    /// 실시간 공동 편집 중인 문서(워크스페이스가 켬, D-70). 본문은 실시간 연결로 고치고 저장 API는 제목만 바꾼다.
    pub realtime: bool,
    /// 이 사람이 내보낼 수 있는지(D-75·D-115). 소유 권한이면 항상 참이고, 그 밖에는 볼 수 있으면서
    /// 문서의 `allow_copy`가 켜졌을 때만 참이다. 수정 권한만으로는 참이 되지 않는다.
    pub can_export: bool,
}

/// 문서 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub title: String,
    /// 이 문서 아래에 만든다(같은 자리의 문서). 없으면 최상위 맨 끝.
    #[serde(default)]
    pub parent_id: Option<DocumentId>,
    /// 이 템플릿의 본문으로 시작한다(D-27). 읽을 수 있는 템플릿이어야 한다.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = Uuid)]
    pub template_id: Option<DocumentId>,
}

/// 문서 저장 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 편집을 시작한 리비전.
    pub base_revision: i32,
    pub title: String,
    #[schema(value_type = Object)]
    pub body: Value,
}

/// 저장 충돌 응답. `current`가 서버 최신본이다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConflictBody {
    /// `conflict`(409, `base_revision`이 지남) 또는 `precondition_failed`(412, `If-Match`가 맞지 않음).
    pub error: &'static str,
    pub current: Document,
    /// `current`를 이 사람이 단건 조회했을 때의 ETag(D-77). 서버 최신본을 받아들인 뒤 `If-Match`로 쓴다.
    pub etag: String,
}

/// 문서 저장의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `ConflictBody`(`error: "conflict"`, `current`): `base_revision`이 지남.
/// - `ErrorBody`(`current` 없음): `account_changed`(기대 사용자가 세션의 사용자와 다름), `invalid_state`(실시간 편집
///   중 본문 변경, 동시 변경 재시도 등).
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SaveConflict {
    Stale(Box<ConflictBody>),
    State(ErrorBody),
}

/// 공유받은 문서.
#[derive(Debug, Serialize, ToSchema)]
pub struct SharedDocument {
    pub id: DocumentId,
    pub title: String,
    /// `read` 또는 `write`.
    pub permission: String,
    /// 공유한 사람의 이름. 워크스페이스 이름은 멤버가 아닌 사람에게 알리지 않는다.
    pub shared_by: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 부모·순서 바꾸기 요청. `parent_id`가 없으면 최상위, `before_id`가 있으면 그 형제 바로 앞, 없으면 맨 끝.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetParent {
    pub parent_id: Option<DocumentId>,
    pub before_id: Option<DocumentId>,
}

/// 문서 설정 변경 요청. 바꾸는 칸만 보낸다. 없거나 null인 칸은 지금 값 그대로다(연 뒤 다른 소유 권한자가 바꾼
/// 비공개·복제 금지를 다른 칸만 바꾼 요청이 되돌리지 않게, 감사 W6).
#[derive(Debug, Deserialize, ToSchema)]
pub struct DocumentSettings {
    /// `workspace` 또는 `private`.
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub allow_copy: Option<bool>,
}

/// 공유 항목.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Share {
    pub organization_id: OrganizationId,
    pub grantee_user_id: UserId,
    pub display_name: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub email: Option<String>,
    /// `read` 또는 `write`.
    pub permission: String,
}

/// 공유 추가 요청. 같은 이메일이면 권한을 바꾼다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShare {
    pub organization_id: Option<OrganizationId>,
    pub email: String,
    /// `read` 또는 `write`.
    pub permission: String,
}

/// 복제 요청. `target_workspace_id`가 없으면 내 개인 노트로 만든다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CopyDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub target_workspace_id: Option<WorkspaceId>,
    pub title: String,
}

/// 이동 대상.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveTarget {
    pub target_workspace_id: Option<WorkspaceId>,
    /// 대상 워크스페이스의 모듈 항목. 없으면 그 워크스페이스의 위키 (D-59).
    pub target_item_id: Option<ItemId>,
}

/// 이동 요청. 미리보기에서 접근 변화를 확인했다는 표시(`acknowledge`)가 있어야 이동한다.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MoveDocument {
    pub target_workspace_id: Option<WorkspaceId>,
    pub target_item_id: Option<ItemId>,
    /// 사용자가 확인한 미리보기. 없거나 지금 계산한 것과 다르면 이동하지 않고 409와 새 미리보기를 돌려준다.
    pub acknowledged: Option<AcknowledgedPreview>,
}

/// 사용자별 접근 등급 변화. 새 접근이나 회수에서는 한쪽 등급이 없다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct MoveAccessChange {
    pub change: String,
    pub user_id: UserId,
    pub display_name: String,
    pub from_grade: Option<String>,
    pub to_grade: Option<String>,
}

/// 사용자가 확인한 접근 변화.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AcknowledgedPreview {
    pub changes: Vec<MoveAccessChange>,
}

/// 이동 미리보기: 접근이 생기는 사람과 사라지는 사람.
#[derive(Debug, Serialize, ToSchema)]
pub struct MovePreview {
    pub changes: Vec<MoveAccessChange>,
    /// 참조 번호가 새로 매겨지는지. 같은 모듈의 다른 항목으로 옮기면 번호를 유지한다. 링크는 id 기준이라 유지된다 (D-18).
    pub reference_changes: bool,
    /// 이전 리비전 기록과 댓글, 첨부 파일(이전 버전 포함)도 함께 옮겨져 새로 볼 수 있게 되는 사람에게 보인다.
    pub history_moves_with_document: bool,
    /// 이동해도 유지되는 활성 공개 링크 수. 로그인 없이 계속 볼 수 있다.
    pub active_public_links: i64,
    /// 원래 자리에 남는 하위 문서 수(내가 볼 수 있는 것). 이동하는 문서의 원래 부모에 붙는다.
    pub children_left_behind: i64,
}

/// 이동의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `MovePreview`: 확인한 미리보기가 없거나 지금 계산한 것과 다르다(새 미리보기를 보고 다시 확인한다).
/// - `ErrorBody`(`error: "invalid_state"`): 옮기지 않았다. 실시간 방의 편집이나 방 없이 남은 저장 대기 상태를 먼저
///   리비전으로 남기지 못했거나(편집은 남는다), 다른 변경이 문서나 그 자리를 잠금 한도보다 오래 쥐었거나, 방이 바쁘거나
///   닫혔다. `message`가 "try again"으로 끝나면 다시 하면 된다. 그 밖(본문 검사를 통과하지 못하는 편집, 그 사이 다른 길로
///   바뀐 본문)은 문서를 다시 열어 보고 판단한다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum MoveConflict {
    Preview(Box<MovePreview>),
    State(ErrorBody),
}

/// 이동 결과. 이동한 사람이 새 위치에서 문서를 볼 수 없으면 `document`가 없다
/// (예: 비공개 문서를 작성 권한만 있는 워크스페이스로 옮긴 경우).
#[derive(Debug, Serialize, ToSchema)]
pub struct MoveOutcome {
    pub workspace_id: WorkspaceId,
    pub reference: String,
    pub document: Option<Document>,
}

/// 이동 트랜잭션의 결과.
#[derive(Debug)]
pub enum MoveReply {
    /// 옮겼다. `committed_at`은 서버가 커밋 응답을 받은 때이며, 그 뒤 받은 편집은 넣지 않는다.
    Moved {
        outcome: Box<MoveOutcome>,
        committed_at: std::time::Instant,
        /// 정확히 이 이동이 권한 확인 snapshot에 보이는지 구분하는 커밋 증거.
        commit: CommittedMove,
    },
    /// 확인한 미리보기가 없거나 잠근 뒤 다시 계산한 것과 달라 옮기지 않았다(새 미리보기).
    Changed(MovePreview),
}

/// 템플릿 목록 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct TemplateQuery {
    /// 이 워크스페이스의 템플릿과 내 개인 노트 템플릿. 없으면 내 개인 노트 템플릿만.
    #[param(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<WorkspaceId>,
}

/// 템플릿 하나.
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentTemplate {
    pub id: DocumentId,
    pub title: String,
    /// 예: `WIKI-3`. 개인 노트면 없다.
    pub reference: Option<String>,
    /// 개인 노트 템플릿인지.
    pub personal: bool,
}

/// 템플릿 표시.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetDocumentTemplate {
    pub template: bool,
}
