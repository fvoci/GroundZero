//! 공유 수신자의 계산 역할과 공유 상한(D-20). 저장하는 공간 역할 `ids::Role`과 분리한다.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 공간 문서 공유 수신자의 계산 역할. 개인 노트의 수신자에는 공간 역할을 적용하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ShareRecipientRole {
    Owner,
    Admin,
    Member,
    Readonly,
    OutsideMember,
}

/// 개별 공유에 저장하거나 수신자 상한으로 반환하는 권한. owner 등급은 공유로 부여하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum SharePermission {
    Read,
    Write,
}
