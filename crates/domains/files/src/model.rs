//! 파일·업로드 자원과 명령 본문.
use fvoci_core::ids::{DocumentId, FileId, ItemId, UploadId, UserId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
/// 조각 본문의 SHA-256(16진수) 헤더.
pub(crate) const EXPECTED_USER_DOC: &str = "이 편집·초안을 시작한 사용자 id(필수). 서버는 세션의 사용자와 비교만 한다: 다르면 \
    아무것도 바꾸지 않고 409 `account_changed`, 없거나 형식이 틀리면 400. 신원·권한은 세션 쿠키로만 정하며 이 값으로 \
    아무것도 허용하지 않는다. 문서 저장(`PUT /api/documents/{document_id}`), 노트 만들기(`POST /api/me/notes`), \
    업로드 시작(`POST /api/uploads`, 만료된 업로드를 스스로 다시 시작함), 업무 쓰기, 조직 설정·워크스페이스 한도 변경에서 검사한다. \
    실시간 편집 연결은 같은 값을 질의 `expected_user`로 받고, 다르면 닫기 코드 4412로 닫는다";

pub const PART_SHA256_HEADER: &str = "x-content-sha256";

/// 새 파일을 붙일 곳.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadTarget {
    /// `document` 또는 `item`.
    pub kind: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 업로드 시작. `file_id`가 있으면 그 파일의 새 버전, 없으면 `target`에 새 파일.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUpload {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub file_id: Option<FileId>,
    pub target: Option<UploadTarget>,
    /// 파일 이름(새 버전이면 비워도 된다).
    #[serde(default)]
    pub name: String,
    /// 버전 설명.
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub content_type: String,
    /// 전체 크기(바이트).
    pub size: i64,
}

/// 업로드 상태.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadView {
    pub id: UploadId,
    /// `uploading`·`completing`·`done`·`aborted`·`expired`·`failed`.
    pub state: String,
    pub name: String,
    pub size: i64,
    /// 조각 크기. 마지막 조각만 작다.
    pub chunk_bytes: i64,
    pub parts_total: i64,
    /// 이미 받은 조각 번호.
    pub received: Vec<i32>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// 끝났으면 만든 파일과 버전.
    pub file_id: Option<FileId>,
    pub version: Option<i32>,
    pub failure: Option<String>,
}

/// 버전 하나.
#[derive(Debug, Serialize, ToSchema)]
pub struct VersionView {
    pub version: i32,
    pub size: i64,
    pub sha256: String,
    pub content_type: String,
    pub note: String,
    pub uploaded_by: UserId,
    pub uploaded_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// 첨부 파일.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileView {
    pub id: FileId,
    pub name: String,
    pub document_id: Option<DocumentId>,
    pub item_id: Option<ItemId>,
    pub current_version: i32,
    pub size: i64,
    pub content_type: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 새 버전·이름 바꾸기·지우기를 할 수 있는지.
    pub can_write: bool,
    /// 내려받을 수 있는지(D-115). 소유 권한이면 항상 참이다. 그 밖에 문서 첨부는 볼 수 있으면서 문서의
    /// `allow_copy`가 켜졌을 때만 참이고(수정 권한만으로는 참이 아니다), 항목 첨부는 볼 수 있으면 참이다.
    /// 아니어도 안전한 형식은 브라우저 안에서 볼 수 있다.
    pub can_download: bool,
}

/// 파일과 모든 버전(최신 먼저).
#[derive(Debug, Serialize, ToSchema)]
pub struct FileDetail {
    pub file: FileView,
    pub versions: Vec<VersionView>,
}

/// 이름 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameFile {
    pub name: String,
}

/// 지운 파일의 휴지통 묶음.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileTrashed {
    #[schema(value_type = String, format = Uuid)]
    pub batch_id: Uuid,
}

/// 내려받기 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ContentQuery {
    /// 버전(없으면 최신).
    pub version: Option<i32>,
    /// 브라우저 안에서 열기(이미지·PDF·텍스트만, 나머지는 무시).
    #[serde(default)]
    pub inline: bool,
}
