use crate::targets::{Target, TargetKind};
use fvoci_core::error::AppError;
use fvoci_core::ids::WorkspaceId;
use fvoci_db::UserTx;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

/// 시간 기록 종류. `study`는 순공부(강의 시청 제외), `lecture`는 강의 시청, `hobby`는 취미 활동, `rest`는 휴식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeKind {
    Work,
    Study,
    Lecture,
    Hobby,
    Rest,
    Other,
}

impl TimeKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Study => "study",
            Self::Lecture => "lecture",
            Self::Hobby => "hobby",
            Self::Rest => "rest",
            Self::Other => "other",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "work" => Some(Self::Work),
            "study" => Some(Self::Study),
            "lecture" => Some(Self::Lecture),
            "hobby" => Some(Self::Hobby),
            "rest" => Some(Self::Rest),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// 시간 기록.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeEntry {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub started_at: OffsetDateTime,
    /// 진행 중이면 없다.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ended_at: Option<OffsetDateTime>,
    /// 일시정지 중이면 그 시각.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub paused_at: Option<OffsetDateTime>,
    pub paused_seconds: i32,
    /// 지금까지의 길이(초). 진행 중이면 응답 시각 기준.
    pub seconds: i64,
    pub kind: TimeKind,
    pub memo: String,
    pub target: Option<Target>,
    pub version: i32,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct EntryRow {
    id: Uuid,
    started_at: OffsetDateTime,
    ended_at: Option<OffsetDateTime>,
    paused_at: Option<OffsetDateTime>,
    paused_seconds: i32,
    seconds: i64,
    kind: String,
    memo: String,
    target_kind: Option<String>,
    target_id: Option<Uuid>,
    target_label: Option<String>,
    workspace_id: Option<WorkspaceId>,
    version: i32,
}

pub(crate) const ENTRY_SELECT: &str =
    "SELECT e.id, e.started_at, e.ended_at, e.paused_at, e.paused_seconds,
        app_entry_seconds(e) AS seconds, e.kind, e.memo, e.target_kind, e.target_id,
        app_time_target_label(e.target_kind, e.target_id) AS target_label, e.workspace_id, e.version
 FROM time_entries e";

pub(crate) fn to_entry(row: EntryRow) -> Result<TimeEntry, AppError> {
    let target = match (row.target_kind, row.target_id) {
        (Some(kind), Some(id)) => Some(Target {
            kind: TargetKind::parse(&kind)
                .ok_or_else(|| AppError::Config(format!("unknown target kind {kind}")))?,
            id,
            label: row.target_label,
            workspace_id: row.workspace_id,
        }),
        _ => None,
    };
    Ok(TimeEntry {
        id: row.id,
        started_at: row.started_at,
        ended_at: row.ended_at,
        paused_at: row.paused_at,
        paused_seconds: row.paused_seconds,
        seconds: row.seconds,
        kind: TimeKind::parse(&row.kind)
            .ok_or_else(|| AppError::Config(format!("unknown time kind {}", row.kind)))?,
        memo: row.memo,
        target,
        version: row.version,
    })
}

pub(crate) async fn load(tx: &mut UserTx, id: Uuid) -> Result<TimeEntry, AppError> {
    let row: Option<EntryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE e.id = $1 AND e.deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    to_entry(row.ok_or(AppError::NotFound)?)
}
