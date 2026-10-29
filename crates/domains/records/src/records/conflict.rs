//! 기록 판 충돌의 현재 상태와 바뀐 칸.

use super::{RecordFile, RecordFileInput, record_kinds};
use crate::RecordsState;
use axum::response::{IntoResponse, Response};
use fvoci_core::{error::AppError, ids::UserId};
use fvoci_db::UserTx;
use fvoci_http::conflict::StaleWrite;
use serde_json::Value;
use uuid::Uuid;

pub(super) async fn stale_record(
    state: &RecordsState,
    user: UserId,
    id: Uuid,
    sent: &Value,
    files: &[RecordFileInput],
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let (current, module) = super::read::load_record(&mut tx, id).await?;
    let mut sent = sent.clone();
    if let Some(def) = record_kinds()
        .iter()
        .find(|def| def.kind == current.kind && def.module_kind == module)
        && let Some(fields) = sent.get("fields").and_then(Value::as_object)
    {
        let (mut people, mut courses, mut episodes) = (Vec::new(), Vec::new(), Vec::new());
        if let Ok(fields) = super::validation::normalize_fields(
            def,
            fields.clone(),
            &mut people,
            &mut courses,
            &mut episodes,
        ) {
            sent["fields"] = fields;
        }
    }
    if same_files(&mut tx, files, &current.files).await? {
        sent["files"] = serde_json::to_value(&current.files)
            .map_err(|error| AppError::Config(format!("record files: {error}")))?;
    }
    tx.commit().await?;
    Ok(
        StaleWrite::new("conflict", "the record changed; reload it", current, &sent)
            .into_response(),
    )
}

async fn same_files(
    tx: &mut UserTx,
    input: &[RecordFileInput],
    current: &[RecordFile],
) -> Result<bool, AppError> {
    // SQL은 영구 삭제된 파일의 스냅샷을 자동으로 남긴다. 그 연결은 화면이 다시 보내지 않는다.
    let current: Vec<_> = current
        .iter()
        .filter(|file| file.file_id.is_some())
        .collect();
    if input.len() != current.len() {
        return Ok(false);
    }
    let ids: Vec<_> = input
        .iter()
        .filter(|file| file.version.is_none())
        .map(|file| file.file_id)
        .collect();
    let versions: Vec<(Uuid, i32)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT id, current_version FROM files WHERE id = ANY($1) AND deleted_at IS NULL",
        )
        .bind(ids)
        .fetch_all(tx.conn())
        .await?
    };
    Ok(input.iter().zip(current).all(|(input, current)| {
        let version = input.version.or_else(|| {
            versions
                .iter()
                .find(|(id, _)| *id == input.file_id)
                .map(|(_, version)| *version)
        });
        current.file_id == Some(input.file_id)
            && version == Some(current.version)
            && input.role == current.role
    }))
}
