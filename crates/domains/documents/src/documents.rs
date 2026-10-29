//! 문서 자원·명령의 공개 경계와 책임별 구현 조립.

mod access;
mod create;
mod management;
mod movement;
mod read;
mod repository;
mod save;
mod shares;
mod title;
mod validation;

pub use crate::model::{
    AcknowledgedPreview, ConflictBody, CopyDocument, CreateDocument, CreateShare, Document,
    DocumentSettings, DocumentSummary, DocumentTemplate, MoveAccessChange, MoveConflict,
    MoveDocument, MoveOutcome, MovePreview, MoveReply, MoveTarget, SaveConflict, SaveDocument,
    SetDocumentTemplate, SetParent, Share, ShareRecipient, SharedDocument, TemplateQuery,
};
pub use fvoci_core::access::Grade as Access;

/// 현재 본문 스키마 버전.
pub const SCHEMA_VERSION: i32 = 1;

/// 이동·복원 트랜잭션이 잠금 하나를 기다리는 한도. 열린 방은 이동·복원을 자기 명령 안에서 하므로(#71) 그동안 편집이 멈춘다
/// (권한 재확인은 이동 중에도 이어 간다, `Room::move_rechecking`). 끝없이 기다리지 않고 409로 돌려준다(하지 않음, 편집은 방에 남는다, #64 리뷰). PostgreSQL
/// `lock_timeout`이라 잠금을 얻으려는 시도마다 따로 재며, 이동·복원 요청 전체의 상한이 아니다(잠금 대기는 최대 이 값 ×
/// 기다린 잠금 수, D-95 docs/decisions/documents.md#d-95). 다른 이동·항목 변경은 보통 이보다 훨씬 짧게 잠근다.
pub(crate) const LOCK_LIMIT: &str = "10s";

/// 기대 사용자 머리의 계약 설명(OpenAPI).
pub const EXPECTED_USER_DOC: &str = "이 편집·초안을 시작한 사용자 id(필수). 서버는 세션의 사용자와 비교만 한다: 다르면 \
    아무것도 바꾸지 않고 409 `account_changed`, 없거나 형식이 틀리면 400. 신원·권한은 세션 쿠키로만 정하며 이 값으로 \
    아무것도 허용하지 않는다. 문서 저장(`PUT /api/documents/{document_id}`), 노트 만들기(`POST /api/me/notes`), \
    업로드 시작(`POST /api/uploads`, 만료된 업로드를 스스로 다시 시작함), 업무 쓰기, 조직 설정·워크스페이스 한도 변경에서 검사한다. \
    실시간 편집 연결은 같은 값을 질의 `expected_user`로 받고, 다르면 닫기 코드 4412로 닫는다";

pub use access::{access, require_manager};
pub use create::{
    __path_create_note, __path_create_wiki, __path_list_wiki, __path_my_notes, create_note,
    create_wiki, list_wiki, my_notes,
};
pub(crate) use create::{NewDocumentPlace, initial_body};
pub(crate) use management::max_depth;
pub use management::{
    __path_set_parent, __path_set_settings, __path_set_template, __path_templates, set_parent,
    set_settings, set_template, templates,
};
pub use movement::{
    __path_copy, __path_move_document, __path_move_preview, copy, document_for, move_document,
    move_in_db, move_preview,
};
pub(crate) use movement::{limit_lock_wait, lock_limit_error};
use read::conflict_reply;
pub use read::{__path_get, __path_shared_with_me, get, shared_with_me};
pub(crate) use read::{conflict, document_reply, representation, stale_reply};
use repository::load;
pub use repository::{DocRow, load_for_update, write_revision};
pub(crate) use repository::{
    SUMMARY_COLUMNS, SummaryRow, TREE_ORDER, load_with_access, to_document, to_summaries,
};
pub use save::{__path_save, save};
pub use shares::{
    __path_create_share, __path_delete_share, __path_list_shares, __path_recipient,
    ShareRecipientQuery, create_share, delete_share, list_shares, recipient,
};
pub(crate) use validation::empty_body;
pub use validation::validate_body;

pub use title::{__path_patch_title, DocumentTitleConflict, PatchDocumentTitle, patch_title};
