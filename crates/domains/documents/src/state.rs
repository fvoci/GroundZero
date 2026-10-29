//! 문서 처리기의 부분 상태와 이동·복원 명령에 필요한 runtime 접점.

use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fvoci_core::error::AppError;
use fvoci_core::ids::{DocumentId, UserId};
use fvoci_db::Db;
use url::Url;
use uuid::Uuid;

use crate::model::{MoveDocument, MoveReply};

/// 문서 명령과 본문 검증이 사용하는 실행 한도.
#[derive(Debug, Clone, Copy)]
pub struct DocumentLimits {
    pub max_document_bytes: usize,
    pub max_title_chars: usize,
    pub max_references_per_document: usize,
    pub max_document_depth: usize,
}

/// 복원할 판과 요청자가 확인한 현재 판.
#[derive(Debug, Clone)]
pub struct RestoreRequest {
    pub doc: DocumentId,
    pub user: UserId,
    pub revision: i32,
    pub request_id: Uuid,
    pub base_revision: i32,
}

/// 처리기가 권한과 이동 대상을 사전 확인한 문서 이동 요청.
#[derive(Debug, Clone)]
pub struct MoveRequest {
    pub doc: DocumentId,
    pub user: UserId,
    pub input: MoveDocument,
}

/// 문서 이동 트랜잭션의 커밋 증거. Snapshot 판정과 연결 관리는 server runtime이 맡는다.
#[derive(Debug, Clone, Copy)]
pub struct CommittedMove(u64);

impl CommittedMove {
    pub const fn new(xid: u64) -> Self {
        Self(xid)
    }

    pub const fn transaction_id(self) -> u64 {
        self.0
    }
}

pub type RuntimeReply<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// 열린 방의 미저장 편집을 보존하며 문서 이동·복원을 직렬화하는 접점(D-95·D-99).
/// 공간의 열린 문서 상태 저장은 파일 소유 PersistWorkspace의 책임이다.
pub trait DocumentRuntime: Debug + Send + Sync {
    fn move_document<'a>(
        &'a self,
        db: &'a Db,
        limits: &'a DocumentLimits,
        request: MoveRequest,
    ) -> RuntimeReply<'a, MoveReply>;

    fn restore<'a>(
        &'a self,
        db: &'a Db,
        limits: &'a DocumentLimits,
        request: RestoreRequest,
    ) -> RuntimeReply<'a, String>;
}

/// 문서 처리기는 전체 Config나 server의 Rooms를 받지 않는다.
#[derive(Debug, Clone)]
pub struct DocumentsState {
    pub db: Db,
    pub limits: DocumentLimits,
    pub public_url: Url,
    pub runtime: Arc<dyn DocumentRuntime>,
}
