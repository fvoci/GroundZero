//! 파일 처리기의 부분 상태와 조립 crate가 주입하는 공동 편집 저장 동작.

use fvoci_db::Db;
use fvoci_storage::Storage;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// 파일 한도 (D-52). 기본값은 O-26 제안값이며 운영 설정으로 바꾼다.
#[derive(Debug, Clone, Copy)]
pub struct FileLimits {
    /// 파일 하나의 최대 크기(바이트). `FVOCI_MAX_FILE_BYTES`, 기본 1 GiB.
    pub max_file_bytes: u64,
    /// 업로드 조각 크기(바이트). `FVOCI_UPLOAD_CHUNK_BYTES`, 기본 8 MiB, 5 MiB~64 MiB(S3 최소 조각 5 MiB,
    /// Cloudflare 요청 한도 100 MB보다 충분히 작게).
    pub chunk_bytes: u64,
    /// 워크스페이스(개인 영역은 사람마다) 총 저장량(바이트). `FVOCI_MAX_WORKSPACE_STORAGE_BYTES`, 기본 50 GiB.
    pub max_storage_bytes: u64,
    /// 마치지 않은 업로드를 버리기까지의 시간. `FVOCI_UPLOAD_TTL_SECS`, 기본 24시간.
    pub upload_ttl: Duration,
    /// 서버 전체에서 동시에 받는 업로드 조각 수(조각마다 메모리에 담는다). `FVOCI_UPLOAD_CONCURRENCY`, 기본 8.
    pub upload_concurrency: usize,
    /// 동시에 만드는 반출 ZIP 수. `FVOCI_EXPORT_CONCURRENCY`, 기본 2.
    pub export_concurrency: usize,
}

/// 버전 참조를 확인하기 전에 열린 공동 편집 상태를 저장한다.
pub trait PersistWorkspace: std::fmt::Debug + Send + Sync {
    fn persist_workspace(&self, workspace: Uuid) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// 파일 처리기에 필요한 연결·저장소·운영 한도.
#[derive(Debug, Clone)]
pub struct FilesState {
    pub db: Db,
    pub storage: Storage,
    pub limits: FileLimits,
    pub upload_permits: Arc<tokio::sync::Semaphore>,
    pub collab: Arc<dyn PersistWorkspace>,
}
