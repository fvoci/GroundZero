//! 검색에 쓰는 공통 텍스트 추출기를 집계 crate에 연결한다.

use crate::features::{file_text, search_text};
use crate::storage::Storage;
use futures_util::StreamExt;
use fvoci_core::error::AppError;
use fvoci_db::Db;
use serde_json::Value;

pub use fvoci_search::indexing::{Hit, IndexDoc, SearchClient, TaskOutcome, prepare};

/// 조립 crate가 소유하는 저장소와 공통 추출기.
#[derive(Debug)]
pub struct TextExtractor<'a>(pub Option<&'a Storage>);

impl fvoci_search::TextExtractor for TextExtractor<'_> {
    fn plain_text(&self, body: &Value) -> String {
        search_text::plain_text(body)
    }

    async fn file_text(
        &self,
        key: Option<&str>,
        name: Option<&str>,
        content_type: Option<&str>,
        size: Option<i64>,
    ) -> Option<String> {
        let storage = self.0?;
        let key = key?;
        let name = name.unwrap_or_default();
        let content_type = content_type.unwrap_or_default();
        let size = u64::try_from(size?).ok()?;
        if size > file_text::MAX_EXTRACT_BYTES || !file_text::supported(content_type, name) {
            return None;
        }
        let mut stream = storage.get(key).await.ok()?;
        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.ok()?);
        }
        let content_type = content_type.to_owned();
        let name = name.to_owned();
        let job =
            tokio::task::spawn_blocking(move || file_text::extract(&bytes, &content_type, &name));
        match tokio::time::timeout(file_text::EXTRACT_TIMEOUT, job).await {
            Ok(Ok(text)) => text,
            Ok(Err(_)) | Err(_) => {
                tracing::warn!(
                    event = "search.extract_failed",
                    file = key,
                    "attachment text extraction failed or timed out"
                );
                None
            }
        }
    }
}

/// 서버 시험의 저장소 인자를 추출기 어댑터로 연결한다.
pub async fn drain(
    db: &Db,
    client: &SearchClient,
    storage: Option<&Storage>,
) -> Result<i32, AppError> {
    fvoci_search::indexing::drain(db, client, &TextExtractor(storage)).await
}
