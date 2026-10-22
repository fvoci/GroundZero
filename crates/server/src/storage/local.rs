//! 서버 로컬 디렉터리 저장소. `objects/<키>`에 내용을, `uploads/<업로드 id>/`에 받은 조각을 둔다.
//! 조각은 임시 파일에 쓴 뒤 이름을 바꿔 넣으므로, 중간에 멈춰도 반쯤 쓴 파일이 보이지 않는다. 마칠 때는 시도마다
//! 따로 준비 파일에 조각을 잇고(`stage`), 그 시도가 마치기를 맡고 있을 때만 최종 키로 이름을 바꿔 게시한다(`publish`).

use std::path::PathBuf;

use bytes::Bytes;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use super::{ByteStream, Part, sha256_hex};
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct LocalStore {
    dir: PathBuf,
}

/// 한 마치기 시도가 조각을 이은 준비 파일. 게시하기 전에는 최종 키와 다른 시도에 보이지 않는다.
#[derive(Debug)]
pub struct Staged {
    path: PathBuf,
    file: tokio::fs::File,
}

impl Staged {
    /// 게시하지 않은 준비 파일을 지운다(게시했거나 조각 폴더와 함께 지워졌으면 할 일이 없다).
    pub(super) async fn discard(self) {
        drop(self.file);
        let _ignored = tokio::fs::remove_file(&self.path).await;
    }
}

fn io(err: std::io::Error) -> AppError {
    AppError::Storage(format!("local: {err}"))
}

/// 임시·준비 파일에 쓰다 실패하면 그 파일을 지운다(반쯤 쓴 파일이 쌓이지 않게).
async fn discard_on_error(temp: &PathBuf, result: std::io::Result<()>) -> Result<(), AppError> {
    if result.is_err() {
        let _ignored = tokio::fs::remove_file(temp).await;
    }
    result.map_err(io)
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
        let written = async {
            let mut file = tokio::fs::File::create(&temp).await?;
            file.write_all(body).await?;
            file.sync_all().await?;
            tokio::fs::rename(&temp, path).await
        }
        .await;
        discard_on_error(&temp, written).await
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

    /// 조각을 순서대로 이 시도만의 준비 파일(`uploads/<업로드 id>/staged-<uuid>`)에 잇는다. 최종 키는 건드리지 않는다.
    /// 받은 조각과 같은 폴더에 두므로 업로드를 게시하거나 그만두면(폴더를 지우면) 남은 준비 파일도 함께 지워진다.
    pub(super) async fn stage(&self, upload_id: &str, parts: &[Part]) -> Result<Staged, AppError> {
        check_upload(upload_id)?;
        let dir = self.upload(upload_id);
        // 앞선 시도가 이미 게시했으면 업로드 폴더가 없다(S3의 NoSuchUpload와 같게).
        if !tokio::fs::try_exists(&dir).await.map_err(io)? {
            return Err(AppError::NotFound);
        }
        let path = dir.join(format!("staged-{}", Uuid::now_v7().simple()));
        let mut file = match tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
            Err(e) => return Err(io(e)),
        };
        let joined = async {
            for part in parts {
                let mut input =
                    tokio::fs::File::open(dir.join(format!("{:05}", part.number))).await?;
                tokio::io::copy(&mut input, &mut file).await?;
            }
            file.sync_all().await
        }
        .await;
        discard_on_error(&path, joined).await?;
        Ok(Staged { path, file })
    }

    /// 준비본을 처음부터 읽는다. 다른 시도가 조각 폴더를 지워도 열어 둔 파일로 읽는다.
    pub(super) async fn read_staged(staged: &Staged) -> Result<ByteStream, AppError> {
        let mut file = staged.file.try_clone().await.map_err(io)?;
        file.rewind().await.map_err(io)?;
        Ok(Box::pin(ReaderStream::new(file)))
    }

    /// 준비본을 최종 키로 게시하고(이름 바꾸기) 받은 조각 폴더를 지운다.
    pub(super) async fn publish(
        &self,
        key: &str,
        upload_id: &str,
        staged: &Staged,
    ) -> Result<(), AppError> {
        check_upload(upload_id)?;
        let target = self.object(key);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io)?;
        }
        tokio::fs::rename(&staged.path, &target).await.map_err(io)?;
        // 게시는 끝났다. 조각 폴더(다른 시도가 남긴 준비본 포함)를 지우지 못해도 내용은 맞으므로 기록만 남긴다.
        match tokio::fs::remove_dir_all(self.upload(upload_id)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                let err = io(e);
                tracing::warn!(
                    event = "storage.upload_dir_left",
                    upload = upload_id,
                    error_code = err.code(),
                    detail = crate::logging::safe_error(&err),
                    "could not remove the parts of a published upload"
                );
            }
            _ => {}
        }
        Ok(())
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
