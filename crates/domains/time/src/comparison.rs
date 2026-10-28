use crate::{TimeState, model::TimeKind};
use axum::{
    Json,
    extract::{Path, State},
};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_http::session::CurrentUser;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// 업무의 예상 시간과 실제 시간.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskTime {
    pub estimate_minutes: Option<i32>,
    /// 내 기록의 합계(초).
    pub mine_seconds: i64,
}

/// 업무의 예상·실제 시간(업무를 볼 수 있는 사람). 기록 하나하나는 돌려주지 않는다.
#[utoipa::path(get, path = "/api/tasks/{task_id}/time", operation_id = "task_time",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = TaskTime), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn task_time(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskTime>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 한 SQL 문장·스냅샷에서 시간 함수의 가시성 검사와 업무 예상 시간을 함께 읽는다.
    let (mine_seconds, estimate_minutes): (i64, Option<i32>) = sqlx::query_as(
        "SELECT app_task_time($1), (SELECT estimate_minutes FROM tasks WHERE id = $1)",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(TaskTime {
        estimate_minutes,
        mine_seconds,
    }))
}

/// 항목의 시간 합계 한 종류.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemTimeKind {
    pub kind: TimeKind,
    /// 내 끝난 기록의 합계(초).
    pub mine_seconds: i64,
}

/// 항목의 시간 합계(D-78): 항목 자체와 그 아래 업무·문서·강의 회차·스터디 단위의 기록을 종류별로 모은다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemTime {
    /// 항목의 휴지통에 없는 업무 예상 시간 합(분).
    pub estimate_minutes: i64,
    /// 기록이 있는 종류만, 종류 이름 순.
    pub kinds: Vec<ItemTimeKind>,
}

/// 항목의 예상·실제 시간(항목을 볼 수 있는 사람). 기록 하나하나는 돌려주지 않는다.
#[utoipa::path(get, path = "/api/items/{item_id}/time", operation_id = "item_time",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = ItemTime), (status = 404, body = ErrorBody)), tag = "time")]
pub async fn item_time(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ItemTime>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 빈 실제 기록 모음에도 예상 시간을 돌려주며, 시간 함수는 본인 기록만 집계한다.
    let rows: Vec<(i64, Option<String>, Option<i64>)> = sqlx::query_as(
        "WITH actual AS MATERIALIZED (SELECT * FROM app_item_time($1))
         SELECT estimate.minutes, actual.entry_kind, actual.mine_seconds
         FROM (SELECT coalesce(sum(estimate_minutes), 0)::bigint AS minutes
               FROM tasks WHERE item_id = $1 AND deleted_at IS NULL) estimate
         LEFT JOIN actual ON true",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    let estimate_minutes = rows.first().map_or(0, |row| row.0);
    tx.commit().await?;
    let kinds = rows
        .into_iter()
        .filter_map(|(_, kind, mine_seconds)| {
            Some(ItemTimeKind {
                kind: TimeKind::parse(&kind?)?,
                mine_seconds: mine_seconds?,
            })
        })
        .collect();
    Ok(Json(ItemTime {
        estimate_minutes,
        kinds,
    }))
}
