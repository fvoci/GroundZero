//! 조립 crate가 저장소·공동 편집·본문 변환을 주입한다. 도메인은 구현 crate에 기대지 않는다.

use bytes::Bytes;
use futures_util::Stream;
use fvoci_core::{error::AppError, ids::DocumentId};
use fvoci_db::Db;
use serde_json::Value;
use std::{collections::HashSet, fmt::Debug, future::Future, pin::Pin, sync::Arc};

/// 조립 경계를 넘는 비동기 작업.
pub type IoFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
/// 저장된 파일 내용.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// 반출과 자동 정리에 필요한 저장소 연산.
pub trait ObjectStore: Debug + Send + Sync {
    fn get<'a>(&'a self, key: &'a str) -> IoFuture<'a, Result<ByteStream, AppError>>;
    fn delete<'a>(&'a self, key: &'a str) -> IoFuture<'a, Result<(), AppError>>;
    fn abort<'a>(&'a self, key: &'a str, upload: &'a str) -> IoFuture<'a, Result<(), AppError>>;
}

/// 현재 구현의 스냅샷 호출 계약을 그대로 주입한다. 실패·시간 초과 보장은 문서·실시간 조각 소유다.
pub trait SnapshotDocuments: Debug + Send + Sync {
    fn snapshot<'a>(&'a self, docs: &'a HashSet<DocumentId>) -> IoFuture<'a, ()>;
}

#[derive(Debug, Clone)]
pub struct SystemState {
    pub db: Db,
    pub storage: Arc<dyn ObjectStore>,
    pub snapshots: Arc<dyn SnapshotDocuments>,
    pub plain_text: fn(&Value) -> String,
    pub export_permits: Arc<tokio::sync::Semaphore>,
}
