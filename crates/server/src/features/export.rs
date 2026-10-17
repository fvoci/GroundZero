//! 데이터 반출 (D-39). 워크스페이스(관리자)와 내 개인 자료를 ZIP 하나로 내려받는다.
//!
//! ZIP 안: `export.json`(구조 데이터: 모듈·항목·문서 본문(Tiptap/ProseMirror JSON)·업무·체크리스트·담당·일정·
//! 파일 버전 목록·감사 기록 등), `documents/`(문서마다 일반 글자), `files/`(모든 파일 버전의 내용). 읽기는 요청한
//! 사람의 권한(RLS)으로 하므로 볼 수 없는 것은 들어가지 않는다. 압축하지 않고 담는다(첨부는 대개 이미 압축됨).
//! Markdown 변환은 편집기의 변환기(문서 화면의 Markdown 보기)를 쓰고 서버에 두 번째 변환기를 두지 않는다.

use std::io::{Seek, SeekFrom, Write};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::search_text::plain_text;
use crate::features::workspaces::member_role;
use crate::http::AppState;
use crate::ids::{Role, WorkspaceId};
use crate::storage::Storage;

enum Msg {
    Start(String),
    Data(Bytes),
}

/// ZIP에 담을 것.
enum Entry {
    Bytes(String, Vec<u8>),
    Object(String, String),
}

fn zip_error(err: zip::result::ZipError) -> std::io::Error {
    std::io::Error::other(err)
}

/// ZIP 안 이름 조각(경로 구분자·제어 문자 제거, 길이 제한).
fn safe_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    let short: String = trimmed.chars().take(80).collect();
    if short.is_empty() { "_".into() } else { short }
}

/// 담을 것을 임시 파일의 ZIP으로 쓴다. 쓰기는 블로킹 스레드에서, 저장소 읽기는 비동기로 한다. 돌려받은 파일은
/// 이름이 이미 지워져 닫으면 사라진다.
async fn build_zip(storage: &Storage, entries: Vec<Entry>) -> Result<std::fs::File, AppError> {
    let path = std::env::temp_dir().join(format!("fvoci-export-{}.zip", Uuid::now_v7().simple()));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Msg>(8);
    let target = path.clone();
    let writer = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
        // 다 쓴 뒤 같은 핸들로 다시 읽어 보내므로 읽기·쓰기로 연다.
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        while let Some(msg) = rx.blocking_recv() {
            match msg {
                Msg::Start(name) => zip.start_file(name, options).map_err(zip_error)?,
                Msg::Data(bytes) => zip.write_all(&bytes)?,
            }
        }
        let mut file = zip.finish().map_err(zip_error)?;
        file.seek(SeekFrom::Start(0))?;
        std::fs::remove_file(&target)?;
        Ok(file)
    });
    let sent = async {
        for entry in entries {
            let closed = || AppError::Config("export writer stopped".into());
            match entry {
                Entry::Bytes(name, bytes) => {
                    tx.send(Msg::Start(name)).await.map_err(|_| closed())?;
                    tx.send(Msg::Data(Bytes::from(bytes)))
                        .await
                        .map_err(|_| closed())?;
                }
                Entry::Object(name, key) => {
                    let mut stream = storage.get(&key).await?;
                    tx.send(Msg::Start(name)).await.map_err(|_| closed())?;
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|e| AppError::Storage(format!("read: {e}")))?;
                        tx.send(Msg::Data(chunk)).await.map_err(|_| closed())?;
                    }
                }
            }
        }
        Ok::<(), AppError>(())
    }
    .await;
    drop(tx);
    let written = writer
        .await
        .map_err(|e| AppError::Config(format!("export task: {e}")))?;
    if let Err(err) = sent {
        // 쓰다 만 파일은 버린다.
        drop(written);
        let _ignored = std::fs::remove_file(&path);
        return Err(err);
    }
    written.map_err(AppError::Io)
}

fn zip_response(file: std::fs::File, name: &str) -> Response {
    let stream = ReaderStream::new(tokio::fs::File::from_std(file));
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{ascii}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// 문서 일반 글자와 파일 버전 내용을 담을 목록을 만든다.
async fn documents_and_files(
    tx: &mut UserTx,
    docs_where: &str,
    files_where: &str,
    bind: Uuid,
) -> Result<Vec<Entry>, AppError> {
    let mut entries = Vec::new();
    let docs: Vec<(Uuid, String, Option<String>, Value)> = sqlx::query_as(&format!(
        "SELECT id, title, app_document_reference(id), body FROM documents WHERE {docs_where} ORDER BY id"
    ))
    .bind(bind)
    .fetch_all(tx.conn())
    .await?;
    for (id, title, reference, body) in docs {
        let label = reference.map_or_else(|| id.simple().to_string(), |r| safe_name(&r));
        let mut text = format!("{title}\n\n");
        text.push_str(&plain_text(&body));
        entries.push(Entry::Bytes(
            format!("documents/{label} {}.txt", safe_name(&title)),
            text.into_bytes(),
        ));
    }
    let versions: Vec<(Uuid, String, i32, String)> = sqlx::query_as(&format!(
        "SELECT f.id, f.name, v.version, v.storage_key FROM files f JOIN file_versions v ON v.file_id = f.id
         WHERE {files_where} ORDER BY f.id, v.version"
    ))
    .bind(bind)
    .fetch_all(tx.conn())
    .await?;
    for (id, name, version, key) in versions {
        entries.push(Entry::Object(
            format!("files/{}/v{version} {}", id.simple(), safe_name(&name)),
            key,
        ));
    }
    Ok(entries)
}

const WORKSPACE_JSON: &str = "
SELECT json_build_object(
  'format', 1,
  'exported_at', now(),
  'workspace', (SELECT row_to_json(w) FROM (SELECT id, name, created_at, trash_retention_days FROM workspaces WHERE id = $1) w),
  'members', (SELECT coalesce(json_agg(m ORDER BY m.display_name), '[]') FROM (
      SELECT wm.user_id, u.display_name, wm.role FROM workspace_members wm JOIN users u ON u.id = wm.user_id
      WHERE wm.workspace_id = $1) m),
  'modules', (SELECT coalesce(json_agg(m ORDER BY m.created_at), '[]') FROM (
      SELECT md.*, (SELECT key FROM module_keys k WHERE k.module_id = md.id AND k.retired_at IS NULL) AS key
      FROM modules md WHERE md.workspace_id = $1) m),
  'items', (SELECT coalesce(json_agg(i ORDER BY i.created_at), '[]') FROM (
      SELECT it.*, app_item_reference(it.id) AS reference FROM items it
      WHERE it.workspace_id = $1 AND it.deleted_at IS NULL) i),
  'documents', (SELECT coalesce(json_agg(d ORDER BY d.created_at), '[]') FROM (
      SELECT doc.id, doc.title, app_document_reference(doc.id) AS reference, doc.item_id, doc.parent_id,
             doc.visibility, doc.schema_version, doc.revision, doc.body, doc.created_at, doc.updated_at
      FROM documents doc WHERE doc.workspace_id = $1 AND doc.deleted_at IS NULL) d),
  'tasks', (SELECT coalesce(json_agg(t ORDER BY t.created_at), '[]') FROM (
      SELECT tk.*, app_task_reference(tk.id) AS reference,
             (SELECT coalesce(json_agg(a.user_id), '[]') FROM task_assignees a WHERE a.task_id = tk.id) AS assignees,
             (SELECT coalesce(json_agg(c ORDER BY c.position), '[]') FROM checklist_items c WHERE c.task_id = tk.id)
               AS checklist
      FROM tasks tk WHERE tk.workspace_id = $1 AND tk.deleted_at IS NULL) t),
  'task_dependencies', (SELECT coalesce(json_agg(dep), '[]') FROM (
      SELECT dp.* FROM task_dependencies dp JOIN items it ON it.id = dp.item_id WHERE it.workspace_id = $1) dep),
  'task_series', (SELECT coalesce(json_agg(s), '[]') FROM (SELECT * FROM task_series WHERE workspace_id = $1) s),
  'events', (SELECT coalesce(json_agg(e ORDER BY e.created_at), '[]') FROM (
      SELECT * FROM events WHERE workspace_id = $1 AND deleted_at IS NULL) e),
  'course_details', (SELECT coalesce(json_agg(c), '[]') FROM (
      SELECT cd.* FROM course_details cd JOIN items it ON it.id = cd.item_id
      WHERE cd.workspace_id = $1 AND it.deleted_at IS NULL) c),
  'course_episodes', (SELECT coalesce(json_agg(c ORDER BY c.item_id, c.number), '[]') FROM (
      SELECT ce.* FROM course_episodes ce JOIN items it ON it.id = ce.item_id
      WHERE ce.workspace_id = $1 AND it.deleted_at IS NULL) c),
  'files', (SELECT coalesce(json_agg(f ORDER BY f.created_at), '[]') FROM (
      SELECT fl.id, fl.name, fl.document_id, fl.item_id, fl.current_version, fl.created_at,
             (SELECT json_agg(json_build_object('version', v.version, 'size', v.size, 'sha256', v.sha256,
                       'content_type', v.content_type, 'note', v.note, 'uploaded_by', v.uploaded_by,
                       'created_at', v.created_at) ORDER BY v.version)
              FROM file_versions v WHERE v.file_id = fl.id) AS versions
      FROM files fl WHERE fl.workspace_id = $1 AND fl.deleted_at IS NULL) f),
  'audit_log', (SELECT coalesce(json_agg(a ORDER BY a.created_at), '[]') FROM (
      SELECT * FROM audit_log WHERE workspace_id = $1) a)
)";

/// 워크스페이스 반출(관리자). 지금 볼 수 있는 모든 내용과 파일 버전을 ZIP으로.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/export", operation_id = "export_workspace",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "export")]
pub async fn workspace(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    if member_role(&mut tx, ws).await? != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let data: Value = sqlx::query_scalar(WORKSPACE_JSON)
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    let name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    let mut entries = vec![Entry::Bytes(
        "export.json".into(),
        serde_json::to_vec_pretty(&data).map_err(|e| AppError::Config(format!("export: {e}")))?,
    )];
    entries.extend(
        documents_and_files(
            &mut tx,
            "workspace_id = $1 AND deleted_at IS NULL",
            "f.workspace_id = $1 AND f.deleted_at IS NULL",
            ws.0,
        )
        .await?,
    );
    sqlx::query("SELECT app_audit_workspace_export($1)")
        .bind(ws)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    let file = build_zip(&state.storage, entries).await?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(zip_response(
        file,
        &format!("fvoci-{}-{date}.zip", safe_name(&name)),
    ))
}

const PERSONAL_JSON: &str = "
SELECT json_build_object(
  'format', 1,
  'exported_at', now(),
  'user', (SELECT row_to_json(u) FROM (SELECT id, display_name, email, created_at FROM users WHERE id = $1) u),
  'notes', (SELECT coalesce(json_agg(d ORDER BY d.created_at), '[]') FROM (
      SELECT id, title, parent_id, schema_version, revision, body, created_at, updated_at
      FROM documents WHERE owner_user_id = $1 AND deleted_at IS NULL) d),
  'files', (SELECT coalesce(json_agg(f ORDER BY f.created_at), '[]') FROM (
      SELECT fl.id, fl.name, fl.document_id, fl.current_version, fl.created_at,
             (SELECT json_agg(json_build_object('version', v.version, 'size', v.size, 'sha256', v.sha256,
                       'content_type', v.content_type, 'note', v.note, 'created_at', v.created_at)
                       ORDER BY v.version)
              FROM file_versions v WHERE v.file_id = fl.id) AS versions
      FROM files fl WHERE fl.owner_user_id = $1 AND fl.deleted_at IS NULL) f),
  'time_entries', (SELECT coalesce(json_agg(t ORDER BY t.started_at), '[]') FROM (
      SELECT id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id, workspace_id,
             created_at, updated_at
      FROM time_entries WHERE user_id = $1 AND deleted_at IS NULL) t),
  'marks', (SELECT coalesce(json_agg(m), '[]') FROM (SELECT * FROM user_marks WHERE user_id = $1) m),
  'episode_progress', (SELECT coalesce(json_agg(p), '[]') FROM (
      SELECT * FROM episode_progress WHERE user_id = $1) p),
  'audit_log', (SELECT coalesce(json_agg(a ORDER BY a.created_at), '[]') FROM (
      SELECT * FROM audit_log WHERE owner_user_id = $1 AND workspace_id IS NULL) a)
)";

/// 내 개인 자료 반출: 개인 노트·첨부·내 시간 기록·고정·즐겨찾기.
#[utoipa::path(get, path = "/api/me/export", operation_id = "export_mine",
    responses((status = 200, content_type = "application/zip", body = Vec<u8>)), tag = "export")]
pub async fn mine(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let data: Value = sqlx::query_scalar(PERSONAL_JSON)
        .bind(user)
        .fetch_one(tx.conn())
        .await?;
    let mut entries = vec![Entry::Bytes(
        "export.json".into(),
        serde_json::to_vec_pretty(&data).map_err(|e| AppError::Config(format!("export: {e}")))?,
    )];
    entries.extend(
        documents_and_files(
            &mut tx,
            "owner_user_id = $1 AND deleted_at IS NULL",
            "f.owner_user_id = $1 AND f.deleted_at IS NULL",
            user.0,
        )
        .await?,
    );
    sqlx::query("SELECT app_audit_personal_export()")
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    let file = build_zip(&state.storage, entries).await?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(zip_response(file, &format!("fvoci-mine-{date}.zip")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_inside_the_archive_stay_inside() {
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name("회의록: 1/2"), "회의록_ 1_2");
        assert_eq!(safe_name("  "), "_");
        assert_eq!(safe_name(&"가".repeat(200)).chars().count(), 80);
    }
}
