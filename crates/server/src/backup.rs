//! 백업·복구 (D-39 "데이터 반출·백업·복구, DB와 첨부 파일의 일관된 복구").
//!
//! `fvoci-server backup create <dir>`는 한 번의 일관된 읽기 스냅숏으로 모든 표를 PostgreSQL `COPY` 글 형식으로
//! 쓰고, 그 스냅숏에 있는 파일 버전의 내용을 저장소에서 복사한 뒤 목록(`manifest.json`)에 마이그레이션 버전·행
//! 수·파일별 크기와 SHA-256을 남긴다. 백업하는 동안 백그라운드 작업은 저장소 정리를 쉰다(지운 버전의 내용이
//! 복사 도중 사라지지 않게). 세션·로그인 상태·대기열·진행 중 업로드는 넣지 않는다(복구 뒤 모두 다시 로그인).
//!
//! `fvoci-server backup restore <dir>`는 같은 마이그레이션 버전까지 만든 빈 DB에만 복구한다. 먼저 파일 내용을
//! 저장소에 다시 올리며 SHA-256을 확인하고, 그다음 한 트랜잭션에서 표를 채운다(관리자 연결, 외래 키·트리거를
//! 잠시 끔). 끝으로 검색 색인을 다시 만들도록 대기열에 올린다. 자체 형식이 아니라 PostgreSQL `COPY` 형식과
//! 일반 파일·JSON만 쓴다.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::config::Config;
use crate::storage::{Part, Storage};
use fvoci_core::error::AppError;
use fvoci_core::secret::Secret;
use fvoci_db::config::DbCommandConfig;

/// 백업 형식 판.
const FORMAT: u32 = 1;
/// 백업·복구 중 저장소 정리를 멈추는 잠금(작업 프로세스가 공유 잠금으로 확인).
pub const STORAGE_LOCK: i64 = 0x6676_6f63_6962_6b70; // "fvocibkp"
/// 백업에 넣지 않는 표(비밀·일시 상태).
const SKIP: &[&str] = &[
    "_sqlx_migrations",
    "sessions",
    "oidc_login_states",
    "search_queue",
    "storage_deletions",
    "uploads",
    "upload_parts",
    "scheduled_runs",
];
/// 복구할 때 파일을 다시 올리는 조각 크기(S3 최소 조각보다 크게).
const RESTORE_CHUNK: usize = 8 * 1024 * 1024;

/// 이 파일을 올릴 조각 크기. 큰 파일은 조각이 10000개(S3 한도)를 넘지 않게 키운다(설정은 업로드 조각 크기 ×
/// 10000까지 허락하므로 8 MiB 조각으로는 다시 올리지 못하는 파일이 있을 수 있다).
fn restore_chunk(size: i64) -> usize {
    let size = usize::try_from(size).unwrap_or(0);
    RESTORE_CHUNK.max(size.div_ceil(10_000))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub migration_version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    pub tables: Vec<TableEntry>,
    pub objects: Vec<ObjectEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TableEntry {
    pub name: String,
    pub rows: i64,
    /// `db/<name>.copy` 파일의 SHA-256(복구 전에 확인).
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObjectEntry {
    pub key: String,
    pub size: i64,
    pub sha256: String,
}

fn io(err: std::io::Error) -> AppError {
    AppError::Io(err)
}

fn need<'a>(value: Option<&'a Secret>, key: &str) -> Result<&'a Secret, AppError> {
    value.ok_or_else(|| AppError::Config(format!("{key} is required")))
}

/// 표 이름은 카탈로그에서 읽은 소문자·숫자·밑줄만 받는다(SQL에 넣기 전에 확인).
fn check_table(name: &str) -> Result<(), AppError> {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        Ok(())
    } else {
        Err(AppError::Config(format!("unexpected table name {name}")))
    }
}

async fn migration_version(conn: &mut PgConnection) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) FROM fvoci._sqlx_migrations WHERE success",
    )
    .fetch_one(&mut *conn)
    .await?)
}

async fn tables(conn: &mut PgConnection) -> Result<Vec<String>, AppError> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'fvoci' AND NOT (tablename = ANY ($1))
         ORDER BY tablename",
    )
    .bind(SKIP)
    .fetch_all(&mut *conn)
    .await?;
    for name in &names {
        check_table(name)?;
    }
    Ok(names)
}

/// 백업을 만든다. `out`은 없거나 빈 폴더여야 한다.
pub async fn create(
    config: &Config,
    db: &DbCommandConfig,
    out: &Path,
) -> Result<Manifest, AppError> {
    if tokio::fs::try_exists(out).await.map_err(io)? {
        let mut entries = tokio::fs::read_dir(out).await.map_err(io)?;
        if entries.next_entry().await.map_err(io)?.is_some() {
            return Err(AppError::Config(format!("{} is not empty", out.display())));
        }
    }
    tokio::fs::create_dir_all(out.join("db"))
        .await
        .map_err(io)?;
    private_dir(out).await?;
    private_dir(&out.join("db")).await?;
    let storage = Storage::new(&config.storage);
    let url = need(db.migration_url.as_ref(), "DATABASE_MIGRATION_URL")?;
    let mut conn = PgConnection::connect_with(&fvoci_db::connect_options(url)?).await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(STORAGE_LOCK)
        .execute(&mut conn)
        .await?;
    let result = create_locked(&mut conn, &storage, out).await;
    if result.is_err() {
        // 실패한 읽기 트랜잭션을 닫아야 잠금 해제가 원래 오류를 덮지 않는다.
        sqlx::query("ROLLBACK").execute(&mut conn).await.ok();
    }
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(STORAGE_LOCK)
        .execute(&mut conn)
        .await?;
    conn.close().await?;
    result
}

async fn create_locked(
    conn: &mut PgConnection,
    storage: &Storage,
    out: &Path,
) -> Result<Manifest, AppError> {
    sqlx::query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *conn)
        .await?;
    let migration_version = migration_version(conn).await?;
    let mut entries = Vec::new();
    for name in tables(conn).await? {
        let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM fvoci.{name}"))
            .fetch_one(&mut *conn)
            .await?;
        let mut file = private_file(&out.join("db").join(format!("{name}.copy"))).await?;
        let mut hasher = Sha256::new();
        let mut stream = conn
            .copy_out_raw(&format!("COPY fvoci.{name} TO STDOUT"))
            .await?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(io)?;
        }
        drop(stream);
        file.sync_all().await.map_err(io)?;
        entries.push(TableEntry {
            name,
            rows,
            sha256: hex::encode(hasher.finalize()),
        });
    }
    let objects: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT storage_key, size, sha256 FROM fvoci.file_versions ORDER BY storage_key",
    )
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query("COMMIT").execute(&mut *conn).await?;

    let mut listed = Vec::new();
    for (key, size, sha256) in objects {
        let path = object_path(out, &key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io)?;
            private_dir(parent).await?;
        }
        let mut file = private_file(&path).await?;
        let mut stream = storage.get(&key).await?;
        let mut hasher = Sha256::new();
        let mut written = 0i64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(io)?;
            hasher.update(&chunk);
            written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
            file.write_all(&chunk).await.map_err(io)?;
        }
        file.sync_all().await.map_err(io)?;
        let actual = hex::encode(hasher.finalize());
        if actual != sha256 || written != size {
            return Err(AppError::Storage(format!(
                "stored content of {key} does not match its record"
            )));
        }
        listed.push(ObjectEntry { key, size, sha256 });
    }
    let manifest = Manifest {
        format: FORMAT,
        migration_version,
        created_at: time::OffsetDateTime::now_utc(),
        tables: entries,
        objects: listed,
    };
    let json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| AppError::Config(format!("manifest: {e}")))?;
    let mut file = private_file(&out.join("manifest.json")).await?;
    file.write_all(&json).await.map_err(io)?;
    file.sync_all().await.map_err(io)?;
    Ok(manifest)
}

fn object_path(root: &Path, key: &str) -> Result<PathBuf, AppError> {
    crate::storage::check_key(key)?;
    Ok(root.join("objects").join(key))
}

/// 빈 DB(같은 마이그레이션 버전)와 저장소에 복구한다.
pub async fn restore(
    config: &Config,
    db: &DbCommandConfig,
    from: &Path,
) -> Result<Manifest, AppError> {
    let raw = tokio::fs::read(from.join("manifest.json"))
        .await
        .map_err(io)?;
    let manifest: Manifest =
        serde_json::from_slice(&raw).map_err(|e| AppError::Config(format!("manifest: {e}")))?;
    if manifest.format != FORMAT {
        return Err(AppError::Config(format!(
            "unsupported backup format {}",
            manifest.format
        )));
    }
    let url = need(db.admin_url.as_ref(), "DATABASE_ADMIN_URL")?;
    let mut conn = PgConnection::connect_with(&fvoci_db::connect_options(url)?).await?;
    // 무엇이든 올리기 전에, 복구 끝에 필요한 관리자 권한(트리거를 잠시 끄는 설정)이 있는지 본다.
    check_replication_role(&mut conn).await?;
    // 표는 마이그레이션 역할(표 소유자)로 읽고 채운다. superuser가 아닌 관리자(셀프호스트 Supabase의 `postgres`)는
    // fvoci 표에 권한이 없다. 관리자 권한은 트리거를 잠시 끄는 설정에만 쓴다(`load_tables`, D-88).
    let borrow = fvoci_db::admin::borrow_migrator(&mut conn).await?;
    let result = restore_as_owner(config, &mut conn, from, &manifest).await;
    let released = fvoci_db::admin::release_migrator_or_reconnect(&mut conn, url, borrow).await;
    let closed = conn.close().await.map_err(AppError::from);
    // 원래 실패(복구)를 먼저 돌려준다. 뒤따른 정리 실패(멤버십 회수·연결 닫기)도 버리지 않고 로그에 남긴다:
    // 회수 실패(새 관리자 연결로 한 번 더 해 본 뒤)는 관리자에게 임시 SET 멤버십이 남았다는 뜻이다.
    for (step, outcome) in [("release_migrator", &released), ("close", &closed)] {
        if let (Err(_), Err(err)) = (&result, outcome) {
            tracing::error!(
                event = "backup.restore_cleanup_failed",
                step,
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(err),
                "cleanup after a failed restore also failed"
            );
        }
    }
    result?;
    released?;
    closed?;
    Ok(manifest)
}

/// 복구는 표를 채우는 동안 `session_replication_role = replica`로 트리거(외래 키 포함)를 끈다. 그 설정은 superuser,
/// 그 매개변수의 SET 권한이 있는 역할, 셀프호스트 Supabase의 `postgres`(supautils가 허락)만 바꿀 수 있다. 실제로
/// 바꿔 보고 되돌려, 권한이 없으면 파일을 올리기 전에 멈춘다(권한을 스스로 얻지 않는다).
async fn check_replication_role(conn: &mut PgConnection) -> Result<(), AppError> {
    let mut tx = conn.begin().await?;
    let allowed = sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await;
    tx.rollback().await?;
    match allowed {
        Ok(_) => Ok(()),
        Err(err) if err.as_database_error().and_then(|e| e.code()).as_deref() == Some("42501") => {
            Err(AppError::Config(
                "the admin role cannot set session_replication_role, which restore needs; use a superuser, \
                 or have a superuser run GRANT SET ON PARAMETER session_replication_role TO <admin>"
                    .into(),
            ))
        }
        Err(err) => Err(err.into()),
    }
}

async fn restore_as_owner(
    config: &Config,
    conn: &mut PgConnection,
    from: &Path,
    manifest: &Manifest,
) -> Result<(), AppError> {
    sqlx::query("SET ROLE fvoci_migrator")
        .execute(&mut *conn)
        .await?;
    let version = migration_version(conn).await?;
    if version != manifest.migration_version {
        return Err(AppError::Config(format!(
            "the database is at migration {version} but the backup is at {}; run `db migrate` with the same release",
            manifest.migration_version
        )));
    }
    let present = tables(conn).await?;
    // 같은 마이그레이션이면 백업할 표도 같다. 목록에서 빠진 표가 있으면 그 내용 없이 끝나므로 거부한다.
    if let Some(name) = present
        .iter()
        .find(|name| !manifest.tables.iter().any(|t| &t.name == *name))
    {
        return Err(AppError::Config(format!(
            "the backup has no dump of table {name}"
        )));
    }
    for table in &manifest.tables {
        check_table(&table.name)?;
        if !present.contains(&table.name) {
            return Err(AppError::Config(format!("table {} is missing", table.name)));
        }
        let used: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS (SELECT 1 FROM fvoci.{})",
            table.name
        ))
        .fetch_one(&mut *conn)
        .await?;
        if used {
            return Err(AppError::Config(format!(
                "restore needs an empty database, but {} has rows",
                table.name
            )));
        }
    }
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(STORAGE_LOCK)
        .execute(&mut *conn)
        .await?;
    let storage = Storage::new(&config.storage);
    let result = restore_locked(conn, &storage, from, manifest).await;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(STORAGE_LOCK)
        .execute(&mut *conn)
        .await?;
    result
}

async fn restore_locked(
    conn: &mut PgConnection,
    storage: &Storage,
    from: &Path,
    manifest: &Manifest,
) -> Result<(), AppError> {
    // 0. 표 덤프가 백업 때 그대로인지(저장소를 건드리기 전에).
    for table in &manifest.tables {
        let mut file = tokio::fs::File::open(from.join("db").join(format!("{}.copy", table.name)))
            .await
            .map_err(io)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = file.read(&mut buf).await.map_err(io)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        if hex::encode(hasher.finalize()) != table.sha256 {
            return Err(AppError::Config(format!(
                "backup dump of {} is damaged (SHA-256 differs)",
                table.name
            )));
        }
    }
    // 1. 파일 내용(백업의 SHA-256과 맞는지 확인하며 올리고, 올린 뒤 저장소에서 다시 확인).
    for object in &manifest.objects {
        restore_object(storage, from, object).await?;
    }
    // 2. 표(한 트랜잭션, 외래 키·트리거를 잠시 끔: 순서와 자기 참조에 상관없이 그대로 넣는다). 검색 색인 다시
    //    만들기도 같은 트랜잭션에서 올린다(실패하면 모두 되돌려 다시 할 수 있게).
    sqlx::query("BEGIN").execute(&mut *conn).await?;
    let loaded = load_tables(conn, from, manifest).await;
    if let Err(err) = loaded {
        sqlx::query("ROLLBACK").execute(&mut *conn).await?;
        return Err(err);
    }
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    Ok(())
}

/// 파일 내용 하나를 올린다. 도중에 실패하면 저장소의 여러 조각 업로드를 그만둔다.
async fn restore_object(
    storage: &Storage,
    from: &Path,
    object: &ObjectEntry,
) -> Result<(), AppError> {
    let path = object_path(from, &object.key)?;
    let mut file = tokio::fs::File::open(&path).await.map_err(io)?;
    // 목록의 크기로 조각 버퍼를 잡으므로 실제 파일 크기와 먼저 맞춘다(목록이 손상돼 크기가 터무니없으면 버퍼를 잡다가
    // 메모리가 모자라 멈췄다).
    let actual = file.metadata().await.map_err(io)?.len();
    if u64::try_from(object.size).ok() != Some(actual) {
        return Err(AppError::Config(format!(
            "backup copy of {} is damaged (size differs)",
            object.key
        )));
    }
    let upload = storage.begin(&object.key).await?;
    let chunk = restore_chunk(object.size);
    let sent = async {
        let mut parts = Vec::new();
        let mut hasher = Sha256::new();
        let mut number = 1u32;
        loop {
            let mut buf = vec![0u8; chunk];
            let mut filled = 0;
            while filled < buf.len() {
                let n = file.read(&mut buf[filled..]).await.map_err(io)?;
                if n == 0 {
                    break;
                }
                filled += n;
            }
            if filled == 0 && number > 1 {
                break;
            }
            buf.truncate(filled);
            hasher.update(&buf);
            let size = buf.len() as u64;
            let etag = storage
                .put_part(&object.key, &upload, number, bytes::Bytes::from(buf))
                .await?;
            parts.push(Part { number, etag, size });
            number += 1;
            if filled < chunk {
                break;
            }
        }
        if hex::encode(hasher.finalize()) != object.sha256 {
            return Err(AppError::Config(format!(
                "backup copy of {} is damaged (SHA-256 differs)",
                object.key
            )));
        }
        storage.complete(&object.key, &upload, &parts).await
    }
    .await;
    if let Err(err) = sent {
        storage.abort(&object.key, &upload).await.ok();
        return Err(err);
    }
    let (stored, size) = storage.digest(&object.key).await?;
    if stored != object.sha256 || i64::try_from(size).ok() != Some(object.size) {
        return Err(AppError::Storage(format!(
            "restored content of {} does not match",
            object.key
        )));
    }
    Ok(())
}

async fn load_tables(
    conn: &mut PgConnection,
    from: &Path,
    manifest: &Manifest,
) -> Result<(), AppError> {
    // 트리거(외래 키 포함)를 끄는 설정은 관리자 권한이 필요하다. 이 트랜잭션 안에서만 관리자로 돌아가 켜고 다시
    // 표 소유자로 채운다(커밋·롤백하면 둘 다 원래대로).
    sqlx::raw_sql(
        "SET LOCAL ROLE NONE;
         SET LOCAL session_replication_role = replica;
         SET LOCAL ROLE fvoci_migrator;",
    )
    .execute(&mut *conn)
    .await?;
    // 복구하는 동안 다른 쓰기가 끼어들지 않게 표를 잠그고 비어 있는지 다시 본다(서버는 멈춰 두는 것을 권한다).
    for table in &manifest.tables {
        sqlx::query(&format!(
            "LOCK TABLE fvoci.{} IN EXCLUSIVE MODE",
            table.name
        ))
        .execute(&mut *conn)
        .await?;
        let used: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS (SELECT 1 FROM fvoci.{})",
            table.name
        ))
        .fetch_one(&mut *conn)
        .await?;
        if used {
            return Err(AppError::Config(format!(
                "restore needs an empty database, but {} has rows",
                table.name
            )));
        }
    }
    for table in &manifest.tables {
        let mut file = tokio::fs::File::open(from.join("db").join(format!("{}.copy", table.name)))
            .await
            .map_err(io)?;
        let mut copy = conn
            .copy_in_raw(&format!("COPY fvoci.{} FROM STDIN", table.name))
            .await?;
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = file.read(&mut buf).await.map_err(io)?;
            if n == 0 {
                break;
            }
            copy.send(&buf[..n]).await?;
        }
        let rows = copy.finish().await?;
        if i64::try_from(rows).ok() != Some(table.rows) {
            return Err(AppError::Config(format!(
                "table {} restored {rows} rows, the backup lists {}",
                table.name, table.rows
            )));
        }
    }
    // 기록된 모든 파일 버전의 내용이 백업에 있어야 한다.
    let keys: Vec<String> = manifest.objects.iter().map(|o| o.key.clone()).collect();
    let missing: Option<String> = sqlx::query_scalar(
        "SELECT storage_key FROM fvoci.file_versions WHERE NOT (storage_key = ANY ($1)) LIMIT 1",
    )
    .bind(&keys)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(key) = missing {
        return Err(AppError::Config(format!(
            "the backup has no content for file version {key}"
        )));
    }
    sqlx::query("SELECT fvoci.app_system_search_enqueue_all()")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// 백업에는 모든 개인 노트와 첨부가 들어 있으므로 폴더는 소유자만(0700) 열 수 있게 한다.
async fn private_dir(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(io)?;
    }
    #[cfg(not(unix))]
    let _unused = path;
    Ok(())
}

/// 소유자만 읽고 쓰는(0600) 새 파일.
async fn private_file(path: &Path) -> Result<tokio::fs::File, AppError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await.map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 감사 11번: 설정이 허락하는 가장 큰 파일(조각 10000개)도 복구할 때 조각 10000개를 넘지 않는다.
    #[test]
    fn restore_parts_stay_within_the_storage_limit() {
        for size in [0u64, 1, 8 << 20, 80 << 30, 100 << 30, 640 << 30] {
            let chunk = restore_chunk(i64::try_from(size).unwrap()) as u64;
            assert!(chunk >= RESTORE_CHUNK as u64);
            assert!(
                size.div_ceil(chunk) <= 10_000,
                "{size} bytes in {chunk}-byte parts"
            );
        }
    }
}
