//! 데이터 반출 (D-39). 워크스페이스(관리자)와 내 개인 자료를 ZIP 하나로 내려받는다.
//!
//! ZIP 안: `export.json`(구조 데이터: 모듈·항목·문서 본문(Tiptap/ProseMirror JSON)·업무·체크리스트·담당·일정·
//! 강좌·파일 버전 목록·감사 기록 등), `documents/`(문서마다 일반 글자), `files/`(모든 파일 버전의 내용). 읽기는 요청한
//! 사람의 권한(RLS)으로 하므로 볼 수 없는 것은 들어가지 않는다. 구조 데이터와 파일 목록은 한 문장(한 스냅숏)으로
//! 읽는다. ZIP은 만들면서 바로 응답으로 흘려보내(임시 파일 없음, 첫 바이트가 곧 나감) 압축하지 않고 담는다. 그
//! 사이 영구 삭제되어 저장소에 없는 내용은 건너뛰고 `missing.json`에 적는다. 동시에 만드는 반출 수는
//! `FVOCI_EXPORT_CONCURRENCY`(기본 2)로 제한한다. Markdown 변환은 편집기의 변환기를 쓰고 서버에 두 번째
//! 변환기를 두지 않는다.

use std::fmt::Write as _;
use std::io::Write;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::features::search_text::plain_text;
use crate::http::AppState;
use crate::storage::Storage;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, WorkspaceId};
use fvoci_http::session::CurrentUser;

/// 응답으로 보내는 조각 크기.
const CHUNK: usize = 64 * 1024;

/// ZIP 쓰기 스레드가 받는 것.
enum Msg {
    Start(String),
    Data(Bytes),
    /// 만들다 실패: 잘린 ZIP이 멀쩡해 보이지 않게 응답을 끊는다.
    Abort(String),
}

/// 담을 것.
enum Entry {
    Bytes(String, Vec<u8>),
    Object(String, String),
}

/// ZIP 출력을 응답 본문 채널로 보내는 쓰기 대상(블로킹 스레드에서 쓴다).
struct BodyWriter {
    tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    buf: Vec<u8>,
}

impl BodyWriter {
    fn send(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::take(&mut self.buf));
        self.tx
            .blocking_send(Ok(chunk))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "client went away"))
    }
}

impl Write for BodyWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= CHUNK {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
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

/// 내려받을 ZIP 이름 머리글(ASCII 대체 이름 + RFC 5987 UTF-8 이름).
fn disposition(name: &str) -> String {
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
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            let _ignored = write!(encoded, "%{b:02X}");
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// 담을 것을 ZIP으로 만들며 바로 응답으로 흘려보낸다. 쓰기는 블로킹 스레드, 저장소 읽기는 비동기 작업.
fn stream_zip(
    storage: Storage,
    entries: Vec<Entry>,
    name: &str,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Response {
    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    let (msg_tx, mut msg_rx) = mpsc::channel::<Msg>(8);
    let writer_tx = body_tx.clone();
    // 블로킹 스레드에는 요청 id가 따라가지 않으므로 직접 넘긴다(D-83).
    let request_id = fvoci_core::logging::current_request_id();
    tokio::task::spawn_blocking(move || {
        // 저장소 실패로 멈춘 경우의 분류(우리가 만든 안전한 문구). 그 밖에는 io 오류 종류만 남긴다.
        let mut aborted: Option<String> = None;
        let result = (|| -> std::io::Result<()> {
            let mut zip = zip::ZipWriter::new_stream(BodyWriter {
                tx: writer_tx,
                buf: Vec::new(),
            });
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .large_file(true);
            while let Some(msg) = msg_rx.blocking_recv() {
                match msg {
                    Msg::Start(name) => zip.start_file(name, options).map_err(zip_error)?,
                    Msg::Data(bytes) => zip.write_all(&bytes)?,
                    Msg::Abort(reason) => {
                        aborted = Some(reason.clone());
                        return Err(std::io::Error::other(reason));
                    }
                }
            }
            let mut out = zip.finish().map_err(zip_error)?;
            out.flush()
        })();
        if let Err(err) = result {
            let detail = aborted.unwrap_or_else(|| format!("io {:?}", err.kind()));
            tracing::warn!(
                event = "export.stopped",
                request_id = request_id.as_deref(),
                detail,
                "export stopped"
            );
            let _sent = body_tx.blocking_send(Err(err));
        }
    });
    tokio::spawn(async move {
        let _permit = permit;
        let mut missing = Vec::new();
        for entry in entries {
            let sent = match entry {
                Entry::Bytes(name, bytes) => {
                    msg_tx.send(Msg::Start(name)).await.is_ok()
                        && msg_tx.send(Msg::Data(Bytes::from(bytes))).await.is_ok()
                }
                Entry::Object(name, key) => match storage.get(&key).await {
                    Err(AppError::NotFound) => {
                        missing.push(name);
                        true
                    }
                    Err(err) => {
                        let _sent = msg_tx
                            .send(Msg::Abort(fvoci_core::logging::safe_error(&err)))
                            .await;
                        return;
                    }
                    Ok(mut stream) => {
                        let mut ok = msg_tx.send(Msg::Start(name)).await.is_ok();
                        while ok {
                            match stream.next().await {
                                None => break,
                                Some(Ok(chunk)) => ok = msg_tx.send(Msg::Data(chunk)).await.is_ok(),
                                Some(Err(err)) => {
                                    let _sent = msg_tx
                                        .send(Msg::Abort(format!("storage read {:?}", err.kind())))
                                        .await;
                                    return;
                                }
                            }
                        }
                        ok
                    }
                },
            };
            if !sent {
                return;
            }
        }
        if !missing.is_empty() {
            let note = serde_json::json!({
                "note": "these file versions were permanently deleted while the export was running",
                "files": missing,
            });
            let _sent = msg_tx.send(Msg::Start("missing.json".into())).await.is_ok()
                && msg_tx
                    .send(Msg::Data(Bytes::from(note.to_string())))
                    .await
                    .is_ok();
        }
    });
    let mut response =
        Body::from_stream(futures_util::stream::unfold(body_rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition(name)) {
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

/// 반출 자리. 모자라면 409(잠시 뒤 다시).
fn export_permit(state: &AppState) -> Result<tokio::sync::OwnedSemaphorePermit, AppError> {
    Arc::clone(&state.export_permits)
        .try_acquire_owned()
        .map_err(|_| AppError::Conflict("other exports are running; try again shortly".into()))
}

/// 구조 데이터에서 담을 목록을 만든다: `export.json`, 문서 일반 글자, 파일 버전 내용.
fn entries_from(mut data: Value, docs_key: &str) -> Result<Vec<Entry>, AppError> {
    let objects = data
        .as_object_mut()
        .and_then(|o| o.remove("_objects"))
        .unwrap_or(Value::Null);
    let mut entries = vec![Entry::Bytes(
        "export.json".into(),
        serde_json::to_vec_pretty(&data).map_err(|e| AppError::Config(format!("export: {e}")))?,
    )];
    for doc in data[docs_key].as_array().into_iter().flatten() {
        let title = doc["title"].as_str().unwrap_or_default();
        let label = doc["reference"].as_str().map_or_else(
            || doc["id"].as_str().unwrap_or("_").replace('-', ""),
            safe_name,
        );
        let text = format!("{title}\n\n{}", plain_text(&doc["body"]));
        entries.push(Entry::Bytes(
            format!("documents/{label} {}.txt", safe_name(title)),
            text.into_bytes(),
        ));
    }
    for object in objects.as_array().into_iter().flatten() {
        let (Some(file), Some(name), Some(version), Some(key)) = (
            object["file"].as_str(),
            object["name"].as_str(),
            object["version"].as_i64(),
            object["key"].as_str(),
        ) else {
            continue;
        };
        entries.push(Entry::Object(
            format!(
                "files/{}/v{version} {}",
                file.replace('-', ""),
                safe_name(name)
            ),
            key.to_owned(),
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
  'item_members', (SELECT coalesce(json_agg(im ORDER BY im.item_id, im.added_at), '[]') FROM (
      SELECT x.item_id, x.user_id, x.group_id, x.added_at FROM item_members x JOIN items it ON it.id = x.item_id
      WHERE it.workspace_id = $1 AND it.deleted_at IS NULL) im),
  'item_records', (SELECT coalesce(json_agg(r ORDER BY r.item_id, r.created_at), '[]') FROM (
      SELECT rec.id, rec.item_id, rec.kind, rec.title, rec.occurred_on, rec.body, rec.fields, rec.created_by,
             rec.created_at, rec.updated_at,
             (SELECT coalesce(json_agg(json_build_object('role', rf.role, 'file_id', rf.file_id, 'file_name', rf.file_name,
                                                        'version', rf.version, 'sha256', rf.sha256)
                                       ORDER BY rf.position), '[]')
              FROM record_files rf WHERE rf.record_id = rec.id) AS files
      FROM item_records rec JOIN items it ON it.id = rec.item_id
      WHERE rec.workspace_id = $1 AND it.deleted_at IS NULL) r),
  'documents', (SELECT coalesce(json_agg(d ORDER BY d.created_at), '[]') FROM (
      SELECT doc.id, doc.title, app_document_reference(doc.id) AS reference, doc.item_id, doc.parent_id,
             doc.visibility, doc.schema_version, doc.revision, doc.body, doc.created_at, doc.updated_at
      FROM documents doc WHERE doc.workspace_id = $1 AND doc.deleted_at IS NULL) d),
  'tasks', (SELECT coalesce(json_agg(t ORDER BY t.created_at), '[]') FROM (
      SELECT tk.*, s.name AS status, s.category AS status_category, app_task_reference(tk.id) AS reference,
             (SELECT coalesce(json_agg(a.user_id), '[]') FROM task_assignees a WHERE a.task_id = tk.id) AS assignees,
             (SELECT coalesce(json_agg(c ORDER BY c.position), '[]') FROM checklist_items c WHERE c.task_id = tk.id)
               AS checklist
      FROM tasks tk JOIN task_statuses s ON s.id = tk.status_id WHERE tk.workspace_id = $1 AND tk.deleted_at IS NULL) t),
  'task_statuses', (SELECT coalesce(json_agg(s ORDER BY s.item_id, s.position), '[]')
      FROM task_statuses s JOIN items i ON i.id = s.item_id WHERE i.workspace_id = $1),
  'task_dependencies', (SELECT coalesce(json_agg(dep), '[]') FROM (
      SELECT dp.* FROM task_dependencies dp JOIN items it ON it.id = dp.item_id WHERE it.workspace_id = $1) dep),
  'task_series', (SELECT coalesce(json_agg(s), '[]') FROM (SELECT * FROM task_series WHERE workspace_id = $1) s),
  'events', (SELECT coalesce(json_agg(e ORDER BY e.created_at), '[]') FROM (
      SELECT * FROM events WHERE workspace_id = $1 AND deleted_at IS NULL) e),
  'comments', (SELECT coalesce(json_agg(c ORDER BY c.created_at), '[]') FROM (
      SELECT cm.id, cm.document_id, cm.task_id, cm.parent_id, cm.body, cm.created_by, cm.created_at, cm.edited,
             cm.deleted_at, cm.resolved_at
      FROM comments cm
      LEFT JOIN documents cd ON cd.id = cm.document_id LEFT JOIN tasks ct ON ct.id = cm.task_id
      WHERE coalesce(cd.workspace_id, ct.workspace_id) = $1) c),
  'course_details', (SELECT coalesce(json_agg(c), '[]') FROM (
      SELECT cd.* FROM course_details cd JOIN items it ON it.id = cd.item_id
      WHERE cd.workspace_id = $1 AND it.deleted_at IS NULL) c),
  'study_units', (SELECT coalesce(json_agg(s ORDER BY s.item_id, s.number), '[]') FROM (
      SELECT su.* FROM study_units su JOIN items it ON it.id = su.item_id
      WHERE su.workspace_id = $1 AND it.deleted_at IS NULL) s),
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
,
  '_objects', (SELECT coalesce(json_agg(json_build_object('file', fl.id, 'name', fl.name, 'version', v.version,
                                                          'key', v.storage_key) ORDER BY fl.id, v.version), '[]')
      FROM files fl JOIN file_versions v ON v.file_id = fl.id
      WHERE fl.workspace_id = $1 AND fl.deleted_at IS NULL)
)";

/// 워크스페이스 반출(관리자). 지금 볼 수 있는 모든 내용과 파일 버전을 ZIP으로.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/export", operation_id = "export_workspace",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "export")]
pub async fn workspace(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_owner(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let permit = export_permit(&state)?;
    // 실시간으로 편집 중인 문서는 본문(마지막 리비전)이 뒤처져 있을 수 있다. 읽기 전에 열린 방을 리비전으로 남긴다.
    let docs: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    // 방을 기다리는 동안 DB 연결을 쥐지 않는다(방이 리비전을 남길 때 연결이 필요하다). 다시 열고 권한도 다시 본다.
    tx.commit().await?;
    state
        .collab
        .snapshot_documents(&docs.into_iter().map(DocumentId).collect())
        .await;
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_owner(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let data: Value = sqlx::query_scalar(WORKSPACE_JSON)
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    let name = data["workspace"]["name"]
        .as_str()
        .unwrap_or("workspace")
        .to_owned();
    sqlx::query("SELECT app_audit_workspace_export($1, $2)")
        .bind(ws)
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    let entries = entries_from(data, "documents")?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(stream_zip(
        state.storage.clone(),
        entries,
        &format!("fvoci-{}-{date}.zip", safe_name(&name)),
        permit,
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
  'unit_progress', (SELECT coalesce(json_agg(p), '[]') FROM (
      SELECT * FROM unit_progress WHERE user_id = $1) p),
  'saved_views', (SELECT coalesce(json_agg(v), '[]') FROM (
      SELECT id, item_id, name, config, created_at FROM saved_views WHERE user_id = $1) v),
  'audit_log', (SELECT coalesce(json_agg(a ORDER BY a.created_at), '[]') FROM (
      SELECT * FROM audit_log WHERE owner_user_id = $1 AND workspace_id IS NULL) a)
,
  '_objects', (SELECT coalesce(json_agg(json_build_object('file', fl.id, 'name', fl.name, 'version', v.version,
                                                          'key', v.storage_key) ORDER BY fl.id, v.version), '[]')
      FROM files fl JOIN file_versions v ON v.file_id = fl.id
      WHERE fl.owner_user_id = $1 AND fl.deleted_at IS NULL)
)";

/// 내 개인 자료 반출: 개인 노트·첨부·내 시간 기록·강의 진도·고정·즐겨찾기.
#[utoipa::path(get, path = "/api/me/export", operation_id = "export_mine",
    responses((status = 200, content_type = "application/zip", body = Vec<u8>), (status = 409, body = ErrorBody)),
    tag = "export")]
pub async fn mine(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, AppError> {
    let permit = export_permit(&state)?;
    let mut tx = state.db.begin_as(user).await?;
    let data: Value = sqlx::query_scalar(PERSONAL_JSON)
        .bind(user)
        .fetch_one(tx.conn())
        .await?;
    sqlx::query("SELECT app_audit_personal_export($1)")
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    let entries = entries_from(data, "notes")?;
    let date = time::OffsetDateTime::now_utc().date();
    Ok(stream_zip(
        state.storage.clone(),
        entries,
        &format!("fvoci-mine-{date}.zip"),
        permit,
    ))
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
