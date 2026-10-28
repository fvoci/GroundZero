//! 기록 목록 자원 읽기.

use super::{Record, RecordFile};
use crate::RecordsState;
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::{ItemId, UserId},
};
use fvoci_http::session::CurrentUser;
use serde_json::Value;
use std::collections::BTreeMap;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

/// 한 SQL 스냅샷에서 기록 판과 고정 파일 연결을 함께 읽는다.
pub(super) async fn load_record(
    tx: &mut fvoci_db::UserTx,
    id: Uuid,
) -> Result<(Record, fvoci_core::module::ModuleKind), AppError> {
    let row: Option<(Value, String, String)> = sqlx::query_as(
        "SELECT to_jsonb(r) || jsonb_build_object(
            'created_by_name', (SELECT u.display_name FROM users u WHERE u.id = r.created_by),
            'can_edit', false, 'can_delete', false,
            'files', (SELECT coalesce(jsonb_agg(to_jsonb(f) || jsonb_build_object(
                'available', f.version_id IS NOT NULL AND EXISTS (
                    SELECT 1 FROM files x WHERE x.id = f.file_id AND x.deleted_at IS NULL))
                ORDER BY f.position), '[]'::jsonb) FROM record_files f WHERE f.record_id = r.id)),
            m.kind, app_access('record', r.id)
         FROM item_records r JOIN items i ON i.id = r.item_id JOIN modules m ON m.id = i.module_id
         WHERE r.id = $1",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    let (json, module, access) = row.ok_or(AppError::NotFound)?;
    let mut record: Record = serde_json::from_value(json)
        .map_err(|error| AppError::Config(format!("record resource: {error}")))?;
    let grade = fvoci_core::access::Grade::parse_present(&access)?;
    record.can_edit = grade.can_write();
    record.can_delete = grade.can_write() && (record.created_by == tx.user() || grade.is_owner());
    let module = fvoci_core::module::ModuleKind::parse(&module).ok_or(AppError::NotFound)?;
    Ok((record, module))
}

type RecordRow = (
    Uuid,
    String,
    String,
    Option<Date>,
    String,
    Value,
    UserId,
    Option<String>,
    OffsetDateTime,
    i32,
    String,
    bool,
);

type FileRow = (Uuid, String, Option<Uuid>, String, i32, String, i64, bool);

/// 항목의 기록(볼 수 있는 사람). 종류·날짜 순.
#[utoipa::path(get, path = "/api/items/{item_id}/records", operation_id = "item_records",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Record]), (status = 404, body = ErrorBody)), tag = "records")]
pub async fn list(
    State(state): State<RecordsState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Record>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Item, item).await?;
    let rows: Vec<RecordRow> = sqlx::query_as(
        "SELECT r.id, r.kind, r.title, r.occurred_on, r.body, r.fields, r.created_by,
                (SELECT u.display_name FROM users u WHERE u.id = r.created_by), r.updated_at, r.version,
                app_access('record', r.id), r.created_by = app_user_id()
         FROM item_records r WHERE r.item_id = $1
         ORDER BY r.kind, r.occurred_on DESC NULLS LAST, r.created_at DESC",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let files: Vec<FileRow> = sqlx::query_as(
        "SELECT f.record_id, f.role, f.file_id, f.file_name, f.version, f.sha256, f.size,
                f.version_id IS NOT NULL AND EXISTS (SELECT 1 FROM files x WHERE x.id = f.file_id AND x.deleted_at IS NULL)
         FROM record_files f WHERE f.record_id = ANY($1) ORDER BY f.record_id, f.position",
    )
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let mut by_record: BTreeMap<Uuid, Vec<RecordFile>> = BTreeMap::new();
    for (record, role, file_id, file_name, version, sha256, size, available) in files {
        by_record.entry(record).or_default().push(RecordFile {
            role,
            file_id,
            file_name,
            version,
            sha256,
            size,
            available,
        });
    }
    rows.into_iter()
        .map(
            |(
                id,
                kind,
                title,
                occurred_on,
                body,
                fields,
                created_by,
                name,
                updated_at,
                version,
                access,
                mine,
            )| {
                let grade = fvoci_db::access::Grade::parse_present(&access)?;
                Ok(Record {
                    id,
                    kind,
                    title,
                    occurred_on,
                    body,
                    fields,
                    files: by_record.remove(&id).unwrap_or_default(),
                    created_by,
                    created_by_name: name,
                    updated_at,
                    version,
                    can_edit: grade.can_write(),
                    can_delete: grade.can_write() && (mine || grade.is_owner()),
                })
            },
        )
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}
