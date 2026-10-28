//! 파일 저장소 (D-06, D-61): 서버 로컬 디렉터리 또는 S3 호환 저장소(Cloudflare R2, 테스트는 SeaweedFS).
//!
//! 두 구현은 같은 여러 조각(multipart) 업로드 모양을 따른다: 시작 → 조각 넣기 → 받은 조각 보기(이어 올리기) →
//! 마치기 또는 그만두기, 그리고 읽기·지우기. 업로드 마치기는 잇기(`assemble`)와 최종 키로 게시하기(`publish`)를
//! 나눠, 마치기를 맡은 시도만 게시한다(로컬. S3는 잇기가 곧 게시다). 큰 파일도 조각마다 따로 보내므로
//! 프록시(예: Cloudflare)의 요청 크기 제한 안에서 전송하고, 마칠 때 파일 전체를 다시 보내지 않는다. 자체 S3
//! 서버는 만들지 않는다(S3 쪽은 서명만 직접 한다).

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod config;
mod local;
mod s3;

use std::pin::Pin;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use sha2::{Digest, Sha256};

use crate::config::StorageConfig;
use fvoci_core::error::AppError;

pub use local::LocalStore;
pub use s3::S3Store;

/// 저장된 내용 읽기 흐름.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// 받은 조각.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub number: u32,
    pub etag: String,
    pub size: u64,
}

/// 저장소. 키는 서버가 만든 값(`[a-z0-9/-]`)만 쓴다.
#[derive(Debug, Clone)]
pub enum Storage {
    Local(LocalStore),
    S3(S3Store),
}

/// 서버가 만든 키인지 확인한다(경로 탈출·이상한 문자 방지).
pub fn check_key(key: &str) -> Result<(), AppError> {
    let ok = !key.is_empty()
        && key.len() <= 512
        && !key.starts_with('/')
        && !key.ends_with('/')
        && !key.contains("//")
        && key
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/');
    if ok {
        Ok(())
    } else {
        Err(AppError::Storage(format!("invalid key {key}")))
    }
}

impl Storage {
    #[must_use]
    pub fn new(config: &StorageConfig) -> Self {
        match config {
            StorageConfig::Local { dir } => Self::Local(LocalStore::new(dir.clone())),
            StorageConfig::S3(s3) => Self::S3(S3Store::new(s3)),
        }
    }

    /// 여러 조각 업로드를 시작한다. 업로드 id를 돌려준다.
    pub async fn begin(&self, key: &str) -> Result<String, AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.begin(key).await,
            Self::S3(s) => s.begin(key).await,
        }
    }

    /// 조각 하나를 넣는다(1부터). 같은 번호를 다시 넣으면 바꾼다. 조각 표시(etag)를 돌려준다.
    pub async fn put_part(
        &self,
        key: &str,
        upload_id: &str,
        number: u32,
        body: Bytes,
    ) -> Result<String, AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.put_part(upload_id, number, body).await,
            Self::S3(s) => s.put_part(key, upload_id, number, body).await,
        }
    }

    /// 받은 조각 목록(번호 순). 이어 올리기에서 무엇을 받았는지 보는 데 쓴다. 마치기에는 조각을 넣을 때 받은
    /// 표시(etag)를 쓴다(저장소마다 다시 올린 조각의 목록 표시가 다르다).
    pub async fn list_parts(&self, key: &str, upload_id: &str) -> Result<Vec<Part>, AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.list_parts(upload_id).await,
            Self::S3(s) => s.list_parts(key, upload_id).await,
        }
    }

    /// 조각을 순서대로 이어 하나의 내용으로 마친다(잇고 바로 게시한다). 마치기를 다투는 시도가 없을 때(백업 복구) 쓴다.
    pub async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[Part],
    ) -> Result<(), AppError> {
        let assembled = self.assemble(key, upload_id, parts).await?;
        let published = self.publish(key, upload_id, &assembled).await;
        assembled.discard().await;
        published
    }

    /// 조각을 순서대로 잇는다. 로컬은 이 시도만의 준비본에 잇고 최종 키는 `publish`가 바꾼다. S3는 여기서 마친다
    /// (마친 여러 조각 업로드는 다시 마칠 수 없어, 늦게 돌아온 시도가 최종 키를 덮지 못한다). 앞선 시도가 이미
    /// 게시해 업로드가 없으면 `NotFound`.
    pub async fn assemble(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[Part],
    ) -> Result<Assembled, AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.stage(upload_id, parts).await.map(Assembled::Staged),
            Self::S3(s) => s
                .complete(key, upload_id, parts)
                .await
                .map(|()| Assembled::Published),
        }
    }

    /// 이은 내용의 SHA-256과 크기. 게시 전 준비본이면 그것을, 아니면 최종 키를 읽는다.
    pub async fn digest_assembled(
        &self,
        key: &str,
        assembled: &Assembled,
    ) -> Result<(String, u64), AppError> {
        match assembled {
            Assembled::Staged(staged) => {
                digest_stream(LocalStore::read_staged(staged).await?).await
            }
            Assembled::Published => self.digest(key).await,
        }
    }

    /// 이은 내용을 최종 키로 게시한다(이미 게시됐으면 할 일이 없다). 업로드 마치기는 그 시도가 점유를 쥔 동안에만
    /// 부른다.
    pub async fn publish(
        &self,
        key: &str,
        upload_id: &str,
        assembled: &Assembled,
    ) -> Result<(), AppError> {
        check_key(key)?;
        match (self, assembled) {
            (Self::Local(s), Assembled::Staged(staged)) => s.publish(key, upload_id, staged).await,
            _ => Ok(()),
        }
    }

    /// 업로드를 그만두고 받은 조각을 버린다(없어도 성공).
    pub async fn abort(&self, key: &str, upload_id: &str) -> Result<(), AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.abort(upload_id).await,
            Self::S3(s) => s.abort(key, upload_id).await,
        }
    }

    /// 내용을 읽는다.
    pub async fn get(&self, key: &str) -> Result<ByteStream, AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.get(key).await,
            Self::S3(s) => s.get(key).await,
        }
    }

    /// 내용을 지운다(없어도 성공).
    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        check_key(key)?;
        match self {
            Self::Local(s) => s.delete(key).await,
            Self::S3(s) => s.delete(key).await,
        }
    }

    /// 저장된 내용 전체의 SHA-256(소문자 16진수)과 크기. 마친 뒤 무결성 확인에 쓴다.
    pub async fn digest(&self, key: &str) -> Result<(String, u64), AppError> {
        digest_stream(self.get(key).await?).await
    }
}

/// 조각을 이은 결과(`Storage::assemble`).
#[derive(Debug)]
pub enum Assembled {
    /// 이 시도만의 준비본(로컬). 게시하기 전에는 최종 키에 보이지 않는다.
    Staged(local::Staged),
    /// 이미 최종 키에 있다(S3는 마치기가 곧 게시이고, 앞선 시도가 이미 게시했을 때도).
    Published,
}

impl Assembled {
    /// 게시하지 않은 준비본을 버린다(게시했으면 할 일이 없다).
    pub async fn discard(self) {
        if let Self::Staged(staged) = self {
            staged.discard().await;
        }
    }
}

async fn digest_stream(mut stream: ByteStream) -> Result<(String, u64), AppError> {
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Storage(format!("read: {e}")))?;
        size += chunk.len() as u64;
        hasher.update(&chunk);
    }
    Ok((hex::encode(hasher.finalize()), size))
}

/// 조각 표시용 SHA-256(16진수).
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_must_be_server_made() {
        assert!(check_key("v1/0123/abc-def").is_ok());
        for bad in [
            "", "/a", "a/", "a//b", "a/../b", "A", "a b", "a/./b", "a\\b", "a?b",
        ] {
            assert!(check_key(bad).is_err(), "{bad}");
        }
    }
}
