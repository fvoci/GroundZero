//! 서버 로컬 디렉터리 저장소. `objects/<키>`에 내용을, `uploads/<업로드 id>/`에 받은 조각을 둔다.
//! 조각과 완성본은 임시 파일에 쓴 뒤 이름을 바꿔 넣으므로, 중간에 멈춰도 반쯤 쓴 파일이 보이지 않는다.

use std::path::PathBuf;

use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use super::{ByteStream, Part, sha256_hex};
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct LocalStore {
    dir: PathBuf,
}

fn io(err: std::io::Error) -> AppError {
    AppError::Storage(format!("local: {err}"))
}

/// 업로드 id는 이 저장소가 만든 UUID(하이픈 없는 32자)만 받는다.
fn check_upload(upload_id: &str) -> Result<(), AppError> {
    if upload_id.len() == 32 && upload_id.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::Storage(format!("invalid upload id {upload_id}")))
    }
}

impl LocalStore {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn object(&self, key: &str) -> PathBuf {
        self.dir.join("objects").join(key)
    }

    fn upload(&self, upload_id: &str) -> PathBuf {
        self.dir.join("uploads").join(upload_id)
    }

    fn temp(&self) -> PathBuf {
        self.dir
            .join("tmp")
            .join(Uuid::now_v7().simple().to_string())
    }

    async fn write_atomically(&self, path: &PathBuf, body: &[u8]) -> Result<(), AppError> {
        let temp = self.temp();
        tokio::fs::create_dir_all(self.dir.join("tmp"))
            .await
            .map_err(io)?;
        let mut file = tokio::fs::File::create(&temp).await.map_err(io)?;
        file.write_all(body).await.map_err(io)?;
        file.sync_all().await.map_err(io)?;
        tokio::fs::rename(&temp, path).await.map_err(io)
    }

    pub(super) async fn begin(&self, _key: &str) -> Result<String, AppError> {
        let id = Uuid::now_v7().simple().to_string();
        tokio::fs::create_dir_all(self.upload(&id))
            .await
            .map_err(io)?;
        Ok(id)
    }

    pub(super) async fn put_part(
        &self,
        upload_id: &str,
        number: u32,
        body: Bytes,
    ) -> Result<String, AppError> {
        check_upload(upload_id)?;
        let dir = self.upload(upload_id);
        if !tokio::fs::try_exists(&dir).await.map_err(io)? {
            return Err(AppError::Storage(format!("upload {upload_id} is gone")));
        }
        let etag = sha256_hex(&body);
        self.write_atomically(&dir.join(format!("{number:05}")), &body)
            .await?;
        self.write_atomically(&dir.join(format!("{number:05}.sha256")), etag.as_bytes())
            .await?;
        Ok(etag)
    }

    pub(super) async fn list_parts(&self, upload_id: &str) -> Result<Vec<Part>, AppError> {
        check_upload(upload_id)?;
        let dir = self.upload(upload_id);
        let mut entries = tokio::fs::read_dir(&dir).await.map_err(io)?;
        let mut parts = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(io)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(number) = name.parse::<u32>() else {
                continue;
            };
            let Ok(etag) = tokio::fs::read_to_string(dir.join(format!("{name}.sha256"))).await
            else {
                continue;
            };
            let size = entry.metadata().await.map_err(io)?.len();
            parts.push(Part { number, etag, size });
        }
        parts.sort_by_key(|p| p.number);
        Ok(parts)
    }

    pub(super) async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[Part],
    ) -> Result<(), AppError> {
        check_upload(upload_id)?;
        let dir = self.upload(upload_id);
        let temp = self.temp();
        tokio::fs::create_dir_all(self.dir.join("tmp"))
            .await
            .map_err(io)?;
        let mut out = tokio::fs::File::create(&temp).await.map_err(io)?;
        for part in parts {
            let path = dir.join(format!("{:05}", part.number));
            let mut input = tokio::fs::File::open(&path).await.map_err(io)?;
            tokio::io::copy(&mut input, &mut out).await.map_err(io)?;
        }
        out.sync_all().await.map_err(io)?;
        let target = self.object(key);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io)?;
        }
        tokio::fs::rename(&temp, &target).await.map_err(io)?;
        tokio::fs::remove_dir_all(&dir).await.map_err(io)
    }

    pub(super) async fn abort(&self, upload_id: &str) -> Result<(), AppError> {
        check_upload(upload_id)?;
        match tokio::fs::remove_dir_all(self.upload(upload_id)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }

    pub(super) async fn get(&self, key: &str) -> Result<ByteStream, AppError> {
        match tokio::fs::File::open(self.object(key)).await {
            Ok(file) => Ok(Box::pin(ReaderStream::new(file))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound),
            Err(e) => Err(io(e)),
        }
    }

    pub(super) async fn delete(&self, key: &str) -> Result<(), AppError> {
        match tokio::fs::remove_file(self.object(key)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
}
