//! 저장소 연결 구성. 환경 변수 해석은 조립 crate가 맡는다.

use fvoci_core::secret::Secret;
use std::path::PathBuf;
use url::Url;

/// S3 호환 저장소 연결 (D-61). 경로 방식 주소를 쓴다. R2의 지역은 `auto`.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: Url,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: Secret,
}

/// 파일 저장소 (D-06). `FVOCI_STORAGE=local|s3`, 기본 local(`FVOCI_STORAGE_DIR`, 기본 `data/files`).
#[derive(Debug, Clone)]
pub enum StorageConfig {
    Local { dir: PathBuf },
    S3(S3Config),
}
