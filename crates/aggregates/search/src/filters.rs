//! 검색 입력과 공통 날짜·색인 필터.

use crate::indexing::day_number;
use fvoci_core::error::{AppError, InvalidReason};
use fvoci_core::ids::{OrganizationId, UserId, WorkspaceId};
use fvoci_db::UserTx;
use serde::Deserialize;
use std::fmt::Write as _;
use time::OffsetDateTime;
use utoipa::IntoParams;
use uuid::Uuid;

const KINDS: &[&str] = &["document", "task", "item", "event", "file", "record"];
/// 업무 상태 범주(D-116). 검색 필터는 상태 이름이 아니라 범주만 본다.
const STATUS_CATEGORIES: &[&str] = &["todo", "doing", "done"];

/// 검색 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    #[param(max_length = 200)]
    pub q: String,
    /// 이 워크스페이스 안에서만(멤버).
    #[param(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<WorkspaceId>,
    /// 쉼표로 구분한 종류(`document,task,item,event,file,record`). 없으면 전부.
    pub kinds: Option<String>,
    /// 이 모듈 안의 것만.
    #[param(value_type = Option<String>, format = Uuid)]
    pub module_id: Option<Uuid>,
    /// 업무 상태 범주(`todo`·`doing`·`done`, D-116). 주면 업무만 나온다. 세 낱말 밖은 400.
    pub status_category: Option<String>,
    /// 담당자(`me` 또는 사용자 id). 주면 업무만 나온다.
    pub assignee: Option<String>,
    /// 날짜 범위 시작(YYYY-MM-DD). 날짜만 있는 값(업무 기한(없으면 시작일), 종일 일정의 시작일, 기록의 발생일)은 그 날짜로,
    /// 실제 시각(문서·항목·파일의 고친 때, 발생일 없는 기록의 고친 때, 시각 일정의 시작)은 `tz`의 그날 0시부터 비교한다.
    pub from: Option<String>,
    /// 날짜 범위 끝(YYYY-MM-DD, 포함). 실제 시각은 `tz`의 다음 날 0시 전까지.
    pub to: Option<String>,
    /// 날짜 범위의 하루를 정하는 IANA 시간대(예: `Asia/Seoul`). 없으면 저장한 시간대, 그것도 없으면 UTC. 모르는 이름은 400.
    pub tz: Option<String>,
    /// 작성자(`me` 또는 사용자 id): 그 사람이 만든 것만(각 대상의 `created_by`). 볼 수 있는 범위는 넓히지 않는다.
    pub author: Option<String>,
    /// `true`면 제목에서만 찾는다(본문·첨부 글자·번호는 보지 않는다).
    pub title_only: Option<bool>,
}

/// 문자열부터 받아 각 칸의 형식 오류를 공통 400 본문으로 바꾼다. 조직 질의는 OrgScope가 별도로 검사한다.
#[derive(Debug, Deserialize)]
pub struct RawSearchQuery {
    q: Option<String>,
    workspace_id: Option<String>,
    kinds: Option<String>,
    module_id: Option<String>,
    status_category: Option<String>,
    assignee: Option<String>,
    from: Option<String>,
    to: Option<String>,
    tz: Option<String>,
    author: Option<String>,
    title_only: Option<String>,
}

fn optional_uuid(raw: Option<String>, field: &str) -> Result<Option<Uuid>, AppError> {
    raw.map(|raw| {
        Uuid::parse_str(&raw).map_err(|_| {
            AppError::invalid(
                field,
                InvalidReason::InvalidFormat,
                None,
                format!("{field} must be a uuid"),
            )
        })
    })
    .transpose()
}

impl TryFrom<RawSearchQuery> for SearchQuery {
    type Error = AppError;

    fn try_from(raw: RawSearchQuery) -> Result<Self, AppError> {
        let q = raw.q.ok_or_else(|| {
            AppError::invalid("q", InvalidReason::Required, None, "q is required")
        })?;
        let title_only = raw
            .title_only
            .map(|value| {
                value.parse::<bool>().map_err(|_| {
                    AppError::invalid(
                        "title_only",
                        InvalidReason::InvalidFormat,
                        None,
                        "title_only must be true or false",
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            q,
            workspace_id: optional_uuid(raw.workspace_id, "workspace_id")?.map(WorkspaceId),
            kinds: raw.kinds,
            module_id: optional_uuid(raw.module_id, "module_id")?,
            status_category: raw.status_category,
            assignee: raw.assignee,
            from: raw.from,
            to: raw.to,
            tz: raw.tz,
            author: raw.author,
            title_only,
        })
    }
}

/// 검색 조건 중 필터.
#[derive(Debug, Default)]
pub(super) struct Filters {
    pub organization: Option<OrganizationId>,
    pub module: Option<Uuid>,
    pub status_category: Option<&'static str>,
    pub assignee: Option<Uuid>,
    /// 날짜만 있는 값과 비교하는 범위(끝 포함).
    pub from: Option<time::Date>,
    pub to: Option<time::Date>,
    /// 실제 시각과 비교하는 범위: `from`의 그날 시작부터 `to` 다음 날 시작 전까지(보는 사람 시간대, `day_bounds`).
    pub start: Option<OffsetDateTime>,
    pub end: Option<OffsetDateTime>,
    pub author: Option<Uuid>,
}

fn parse_day(raw: Option<&str>, name: &str) -> Result<Option<time::Date>, AppError> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Ok(None);
    };
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let day = time::Date::parse(raw.trim(), &format).map_err(|_| {
        AppError::invalid(
            name,
            InvalidReason::InvalidFormat,
            None,
            format!("{name} must be YYYY-MM-DD"),
        )
    })?;
    // 그날 시작과 다음 날 시작을 시각으로 만들 수 있는 날만(`day_bounds`): 9999-12-31은 다음 날이 `time` 범위 밖이고,
    // 1년 전은 시간대에 따라 PostgreSQL 시각 범위를 벗어날 수 있다.
    if day.year() < 1 || day.next_day().is_none() {
        return Err(AppError::invalid(
            name,
            InvalidReason::OutOfRange,
            None,
            format!("{name} is out of range"),
        ));
    }
    Ok(Some(day))
}

pub(super) fn filters(query: &SearchQuery, me: UserId) -> Result<Filters, AppError> {
    let status_category = match query.status_category.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(
            STATUS_CATEGORIES
                .iter()
                .copied()
                .find(|c| *c == raw)
                .ok_or_else(|| {
                    AppError::invalid(
                        "status_category",
                        InvalidReason::InvalidFormat,
                        None,
                        "status_category must be todo, doing or done",
                    )
                })?,
        ),
    };
    let person = |raw: Option<&str>, name: &str| -> Result<Option<Uuid>, AppError> {
        match raw.map(str::trim) {
            None | Some("") => Ok(None),
            Some("me") => Ok(Some(me.0)),
            Some(raw) => Uuid::parse_str(raw).map(Some).map_err(|_| {
                AppError::invalid(
                    name,
                    InvalidReason::InvalidFormat,
                    None,
                    format!("{name} must be me or a user id"),
                )
            }),
        }
    };
    let assignee = person(query.assignee.as_deref(), "assignee")?;
    let author = person(query.author.as_deref(), "author")?;
    Ok(Filters {
        organization: None,
        module: query.module_id,
        status_category,
        assignee,
        from: parse_day(query.from.as_deref(), "from")?,
        to: parse_day(query.to.as_deref(), "to")?,
        start: None,
        end: None,
        author,
    })
}

/// 날짜 범위를 보는 사람 시간대의 시각 범위로 바꾼다(D-101). 시작은 `from`의 그날 시작, 끝은 `to` 다음 날의 시작(제외)이라
/// 여름시간으로 23·25시간인 날도 그대로 맞다. 색인 경로와 대체 경로가 이 값을 함께 쓴다. 시간대를 보내지 않으면 저장한
/// 시간대(D-81), 그것도 없으면 UTC. 모르는 시간대는 400.
pub(super) async fn day_bounds(
    tx: &mut UserTx,
    f: &mut Filters,
    tz: Option<&str>,
) -> Result<(), AppError> {
    if f.from.is_none() && f.to.is_none() {
        return Ok(());
    }
    let tz: String = match tz.map(str::trim).filter(|t| !t.is_empty()) {
        Some(tz) => tz.to_owned(),
        None => sqlx::query_scalar(
            "SELECT coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC')",
        )
        .fetch_one(tx.conn())
        .await?,
    };
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(&tz)
        .execute(tx.conn())
        .await
        .map_err(|err| match AppError::from_db_function(err) {
            AppError::BadRequest(message) => {
                AppError::invalid("tz", InvalidReason::InvalidFormat, None, message)
            }
            other => other,
        })?;
    // 하루의 시작은 그날 0시가 처음 오는 시점이다(0시가 두 번 오는 날은 앞의 0시, 없는 날은 건너뛴 직후). 시간 기록의 날
    // 경계(`time_tracking.rs`, 감사 S21)와 같은 식.
    let (start, end): (Option<OffsetDateTime>, Option<OffsetDateTime>) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(f.from)
    .bind(f.to)
    .bind(&tz)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    f.start = start;
    f.end = end;
    Ok(())
}

/// Meilisearch 필터에 더할 조건(값은 UUID·고정 낱말·숫자뿐이라 따옴표 밖으로 나갈 수 없다).
pub(super) fn meili_filters(f: &Filters) -> String {
    let mut out = String::new();
    if let Some(m) = f.module {
        let _ignored = write!(out, " AND module_id = \"{m}\"");
    }
    if let Some(c) = f.status_category {
        // 범주는 고정 낱말 셋 가운데 하나다(D-116). 상태 이름으로는 거르지 않는다.
        let _ignored = write!(out, " AND status_category = \"{c}\"");
    }
    if let Some(a) = f.assignee {
        let _ignored = write!(out, " AND assignee_ids = \"{a}\"");
    }
    // 한 문서에는 `at`(실제 시각)·`date`(날짜만) 가운데 하나만 있다. 대체 경로와 같은 조건.
    let mut at = Vec::new();
    let mut date = Vec::new();
    if let (Some(start), Some(d)) = (f.start, f.from) {
        at.push(format!("at >= {}", start.unix_timestamp()));
        date.push(format!("date >= {}", day_number(d)));
    }
    if let (Some(end), Some(d)) = (f.end, f.to) {
        at.push(format!("at < {}", end.unix_timestamp()));
        date.push(format!("date <= {}", day_number(d)));
    }
    if !at.is_empty() {
        let _ignored = write!(
            out,
            " AND (({}) OR ({}))",
            at.join(" AND "),
            date.join(" AND ")
        );
    }
    if let Some(a) = f.author {
        let _ignored = write!(out, " AND created_by = \"{a}\"");
    }
    out
}

pub(super) fn parse_kinds(raw: Option<&str>) -> Result<Vec<&'static str>, AppError> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Ok(KINDS.to_vec());
    };
    raw.split(',')
        .map(|k| {
            KINDS
                .iter()
                .copied()
                .find(|known| *known == k.trim())
                .ok_or_else(|| {
                    AppError::invalid(
                        "kinds",
                        InvalidReason::InvalidFormat,
                        None,
                        format!("unknown kind {k}"),
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 검색 필터는 상태 이름이 아니라 범주만 본다(D-116): 세 낱말 가운데 하나만 받고 그대로 필터 식에 넣는다.
    #[test]
    fn status_category_is_one_of_three_fixed_words() {
        let filter = Filters {
            status_category: Some("doing"),
            ..Filters::default()
        };
        assert_eq!(meili_filters(&filter), " AND status_category = \"doing\"");
        let me = UserId(Uuid::from_u128(1));
        let query = |raw: &str| -> SearchQuery {
            serde_json::from_value(serde_json::json!({ "q": "x", "status_category": raw })).unwrap()
        };
        assert_eq!(
            filters(&query(" done "), me).unwrap().status_category,
            Some("done")
        );
        assert_eq!(filters(&query(""), me).unwrap().status_category, None);
        assert!(matches!(
            filters(&query("late"), me),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            filters(&query("검토"), me),
            Err(AppError::Invalid(_))
        ));
    }

    /// 날짜 범위는 그날 시작과 다음 날 시작을 시각으로 만든다. 다음 날이 없는 날(9999-12-31)이나 1년 전은 그 시각을
    /// 나타낼 수 없어(`time`·PostgreSQL 범위 밖) 500이 되던 것을 400으로 거절한다(CodeRabbit #99).
    #[test]
    fn days_without_a_representable_boundary_are_rejected() {
        for raw in ["9999-12-31", "0000-12-31", "-0001-01-01"] {
            assert!(
                matches!(parse_day(Some(raw), "to"), Err(AppError::Invalid(_))),
                "{raw}"
            );
        }
        for raw in ["9999-12-30", "0001-01-01", "2026-03-08"] {
            assert!(matches!(parse_day(Some(raw), "to"), Ok(Some(_))), "{raw}");
        }
    }

    #[test]
    fn date_ranges_compare_instants_and_dates_separately() {
        let day = |s: &str| {
            time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).ok()
        };
        let at = |s: i64| OffsetDateTime::from_unix_timestamp(s).ok();
        let author = Uuid::from_u128(7);
        let both = Filters {
            from: day("2026-03-08"),
            to: day("2026-03-08"),
            start: at(1_772_946_000),
            end: at(1_773_028_800),
            author: Some(author),
            ..Filters::default()
        };
        assert_eq!(
            meili_filters(&both),
            format!(
                " AND ((at >= 1772946000 AND at < 1773028800) OR (date >= 20260308 AND date <= 20260308)) \
                 AND created_by = \"{author}\""
            )
        );
        let from_only = Filters {
            from: day("2026-03-09"),
            start: at(1_773_028_800),
            ..Filters::default()
        };
        assert_eq!(
            meili_filters(&from_only),
            " AND ((at >= 1773028800) OR (date >= 20260309))"
        );
        assert_eq!(meili_filters(&Filters::default()), "");
    }
}
