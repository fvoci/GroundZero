use crate::TimeState;
use axum::{
    Json,
    extract::{Query, State},
};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::WorkspaceId;
use fvoci_http::session::CurrentUser;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 연결 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Task,
    Document,
    Item,
    /// 강의 회차(D-32).
    Episode,
    /// 스터디 단위(D-32 공부).
    StudyUnit,
}

impl TargetKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Document => "document",
            Self::Item => "item",
            Self::Episode => "episode",
            Self::StudyUnit => "study_unit",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "task" => Some(Self::Task),
            "document" => Some(Self::Document),
            "item" => Some(Self::Item),
            "episode" => Some(Self::Episode),
            "study_unit" => Some(Self::StudyUnit),
            _ => None,
        }
    }
}

/// 연결 대상(보낼 때).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct TargetInput {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 연결 대상(받을 때). 지금 볼 수 없으면 `label`이 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Target {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub label: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
}

pub(crate) fn split_target(target: Option<TargetInput>) -> (Option<&'static str>, Option<Uuid>) {
    target.map_or((None, None), |t| (Some(t.kind.as_str()), Some(t.id)))
}

/// 본인 기록의 대상 표시. 기록 자원과 따로 읽어 웹에서 조합한다.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeEntryTarget {
    #[schema(value_type = String, format = Uuid)]
    pub entry_id: Uuid,
    #[serde(flatten)]
    pub target: Target,
}

/// 하나의 기록 또는 목록과 같은 기간. 두 범위를 섞지 않는다.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct TargetQuery {
    pub entry_id: Option<Uuid>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub tz: Option<String>,
}

enum Scope {
    Entry(Uuid),
    Range(crate::range::TimeRange),
}

impl TargetQuery {
    fn scope(self) -> Result<Scope, AppError> {
        match (self.entry_id, self.from, self.to, self.tz) {
            (Some(id), None, None, None) => Ok(Scope::Entry(id)),
            (None, Some(from), Some(to), Some(tz)) => {
                Ok(Scope::Range(crate::range::TimeRange { from, to, tz }))
            }
            _ => Err(AppError::invalid(
                "entry_id",
                InvalidReason::InvalidFormat,
                None,
                "send entry_id or all of from, to and tz",
            )),
        }
    }
}

/// 본인 기록의 대상 표시 모음. 타인 기록은 서버 관리자에게도 나오지 않는다.
/// 공통 참조 해석은 강의 회차·스터디 단위를 지원하지 않아 시간의 다섯 대상 종류를 여기서 읽는다.
#[utoipa::path(get, path = "/api/time-entry-targets", operation_id = "time_entry_targets",
    params(TargetQuery),
    responses((status = 200, body = [TimeEntryTarget]), (status = 400, body = ErrorBody)), tag = "time")]
pub async fn list(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<TargetQuery>,
) -> Result<Json<Vec<TimeEntryTarget>>, AppError> {
    let scope = query.scope()?;
    let mut tx = state.db.begin_as(user).await?;
    let (id, from, to) = match scope {
        Scope::Entry(id) => (Some(id), None, None),
        Scope::Range(range) => {
            let window = crate::range::window(&mut tx, &range).await?;
            (None, Some(window.from_at), Some(window.to_at))
        }
    };
    type Row = (Uuid, String, Uuid, Option<String>, Option<WorkspaceId>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT e.id, e.target_kind, e.target_id,
                app_time_target_label(e.target_kind, e.target_id), e.workspace_id
         FROM time_entries e
         WHERE e.deleted_at IS NULL AND e.target_kind IS NOT NULL
           AND ($1::uuid IS NULL OR e.id = $1)
           AND ($2::timestamptz IS NULL OR e.started_at >= $2)
           AND ($3::timestamptz IS NULL OR e.started_at < $3)
         ORDER BY e.id",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(entry_id, kind, id, label, workspace_id)| {
            Ok(TimeEntryTarget {
                entry_id,
                target: Target {
                    kind: TargetKind::parse(&kind)
                        .ok_or_else(|| AppError::Config(format!("unknown target kind {kind}")))?,
                    id,
                    label,
                    workspace_id,
                },
            })
        })
        .collect::<Result<_, _>>()
        .map(Json)
}
