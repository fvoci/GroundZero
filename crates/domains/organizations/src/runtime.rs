//! 조직 명령 전 실시간 편집 보존. 구현과 방 수명은 서버 조립부가 주입한다.

use fvoci_core::{error::AppError, ids::UserId};
use fvoci_db::Db;
use std::{fmt::Debug, future::Future, pin::Pin, sync::Arc};
use uuid::Uuid;

/// SQL이 명령 권한을 확인할 범위. 내용 열람·대리 저장 권한을 부여하지 않는다.
#[derive(Debug, Clone, Copy)]
pub struct RoomChange {
    pub action: &'static str,
    pub scope: Uuid,
    pub member: Option<UserId>,
}

/// 커밋·롤백·요청 취소까지 새 방 진입을 막고, 버리면 차단을 해제한다.
pub trait PreparedRooms: Debug + Send {}

pub type PrepareReply<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PreparedRooms>, AppError>> + Send + 'a>>;

pub trait OrganizationRuntime: Debug + Send + Sync {
    fn prepare<'a>(&'a self, db: &'a Db, user: UserId, change: RoomChange) -> PrepareReply<'a>;
}

/// 조직 라우터에만 넣는 실행 접점. 도메인은 서버 타입에 의존하지 않는다.
#[derive(Debug, Clone)]
pub struct OrganizationRooms(pub Arc<dyn OrganizationRuntime>);

impl OrganizationRooms {
    pub async fn prepare(
        &self,
        db: &Db,
        user: UserId,
        action: &'static str,
        scope: Uuid,
        member: Option<UserId>,
    ) -> Result<Box<dyn PreparedRooms>, AppError> {
        self.0
            .prepare(
                db,
                user,
                RoomChange {
                    action,
                    scope,
                    member,
                },
            )
            .await
    }
}
