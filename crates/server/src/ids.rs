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

/// 워크스페이스 역할 (D-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Writer,
    Reader,
}

impl Role {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "admin" => Some(Self::Admin),
            "writer" => Some(Self::Writer),
            "reader" => Some(Self::Reader),
            _ => None,
        }
    }

    /// 문서를 만들고 고칠 수 있는지.
    pub fn can_write(self) -> bool {
        matches!(self, Self::Admin | Self::Writer)
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
