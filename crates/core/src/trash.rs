//! 지우기 명령이 함께 쓰는 응답. 휴지통 목록·복구·비우기는 시스템 도메인이고, 지우기 명령은 각 도메인에 있다(D-41).

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// 지운 결과(되살릴 때 쓰는 휴지통 묶음).
#[derive(Debug, Serialize, ToSchema)]
pub struct Trashed {
    #[schema(value_type = String, format = Uuid)]
    pub batch_id: Uuid,
}
