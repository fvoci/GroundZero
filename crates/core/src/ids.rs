//! 의미 있는 ID·역할 타입 (D-09). 모든 값을 `Uuid`나 문자열로 넘기지 않는다.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, ToSchema)]
        #[serde(transparent)]
        #[sqlx(transparent)]
        #[schema(value_type = String, format = Uuid)]
        pub struct $name(pub Uuid);
    };
}

id_type!(
    /// 사용자 내부 ID.
    UserId
);
id_type!(
    /// 워크스페이스 내부 ID.
    WorkspaceId
);
id_type!(
    /// 문서 내부 ID. CRDT 도입 후에도 유지된다 (D-62).
    DocumentId
);

id_type!(
    /// 조직 내부 ID.
    OrganizationId
);

/// 워크스페이스 역할 (D-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Member,
    Readonly,
}

impl Role {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            "readonly" => Some(Self::Readonly),
            _ => None,
        }
    }
}
id_type!(
    /// 모듈 인스턴스 내부 ID (D-15). 키가 바뀌어도 유지된다.
    ModuleId
);
id_type!(
    /// 모듈 항목(개별 프로젝트·연구·스터디·강좌·취미 활동) 내부 ID (D-14).
    ItemId
);
id_type!(
    /// 첨부 파일 내부 ID (D-06). 새 버전을 올려도 유지된다(D-35).
    FileId
);
id_type!(
    /// 업로드 세션 ID. 끊긴 업로드를 이어 올릴 때 쓴다.
    UploadId
);
