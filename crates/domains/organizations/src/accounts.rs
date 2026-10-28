//! 현재 계정과 표시 설정(D-50·D-81·D-109·D-121).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AccountState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{OrganizationId, UserId};
use fvoci_http::session::CurrentUser;

/// 웹 입력 미리 검사에 쓰는 서버 설정. 글자 수는 Unicode 코드 포인트 기준이며 서버가 최종 검증한다.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct InputLimits {
    /// 제목·이름 상한(`FVOCI_MAX_TITLE_CHARS`). 업무 CSV의 각 행에도 적용된다.
    pub max_title_chars: usize,
    /// 문서 본문 JSON 직렬화 크기 상한(바이트).
    pub max_document_bytes: usize,
    /// 문서 하나의 참조 노드 수 상한.
    pub max_references_per_document: usize,
    /// 문서 트리 최대 깊이(최상위 = 1).
    pub max_document_depth: usize,
}

/// 현재 사용자 정보.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    pub id: UserId,
    pub display_name: String,
    pub email: Option<String>,
    /// 내 시간대(IANA 이름, D-50). 비었으면 아직 고르지 않음(화면이 브라우저 시간대를 쓴다). 본인에게만 보인다.
    pub time_zone: String,
    /// 표시 언어(D-81): `ko`·`en`, 비었으면 아직 고르지 않음.
    pub locale: String,
    /// 한 주 시작(D-81): `mon`·`sun`, 비었으면 아직 고르지 않음.
    pub week_start: String,
    /// 시각 표시(D-81): `h12`·`h23`, 비었으면 아직 고르지 않음.
    pub hour_cycle: String,
    /// 화면 밝기(D-109): `system`(기본, 운영체제 설정을 따름)·`light`·`dark`.
    pub theme: String,
    /// 마지막으로 고른 조직. 미설정이면 null이며 현재 소속 여부는 조직 목록에서 확인한다.
    pub last_organization_id: Option<OrganizationId>,
    /// 서버 입력 상한. 강의 회차 제목의 고정 한도(300자)는 이 설정과 별개다.
    pub limits: InputLimits,
}

/// 현재 로그인한 사용자.
#[utoipa::path(get, path = "/api/me",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody)), tag = "auth")]
pub async fn me(
    State(state): State<AccountState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Me>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    type MeRow = (
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        Option<OrganizationId>,
    );
    let (display_name, email, time_zone, locale, week_start, hour_cycle, theme, last_organization_id): MeRow =
        sqlx::query_as(
            "SELECT u.display_name, u.email, coalesce(s.time_zone, ''), coalesce(s.locale, ''),
                coalesce(s.week_start, ''), coalesce(s.hour_cycle, ''), coalesce(s.theme, 'system'), s.last_organization_id
         FROM users u LEFT JOIN user_settings s ON s.user_id = u.id WHERE u.id = $1",
        )
        .bind(user)
        .fetch_optional(tx.conn())
        .await?
        .ok_or(AppError::Unauthorized)?;
    tx.commit().await?;
    Ok(Json(Me {
        id: user,
        display_name,
        email,
        time_zone,
        locale,
        week_start,
        hour_cycle,
        theme,
        last_organization_id,
        limits: state.limits,
    }))
}

/// 내 표시 설정과 마지막 조직 바꾸기. 보내지 않은 칸은 그대로 둔다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MySettings {
    /// IANA 시간대 이름(예: `Asia/Seoul`). 빈 문자열이면 지운다.
    #[serde(default)]
    pub time_zone: Option<String>,
    /// `ko`·`en`, 빈 문자열이면 지운다.
    #[serde(default)]
    pub locale: Option<String>,
    /// `mon`·`sun`, 빈 문자열이면 지운다.
    #[serde(default)]
    pub week_start: Option<String>,
    /// `h12`·`h23`, 빈 문자열이면 지운다.
    #[serde(default)]
    pub hour_cycle: Option<String>,
    /// `system`·`light`·`dark`. 빈 문자열은 받지 않는다(기본이 `system`).
    #[serde(default)]
    pub theme: Option<String>,
    /// 마지막 조직. 생략하면 유지하고 null이면 지운다. UUID는 소속 조직만 허용한다.
    #[serde(default, deserialize_with = "deserialize_last_organization")]
    #[schema(value_type = Option<String>, format = Uuid, nullable = true)]
    pub last_organization_id: Option<Option<OrganizationId>>,
}

fn deserialize_last_organization<'de, D>(
    deserializer: D,
) -> Result<Option<Option<OrganizationId>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<OrganizationId>::deserialize(deserializer).map(Some)
}

/// 내 표시 설정과 마지막 조직 바꾸기(D-50·D-81·D-109·D-121). 저장된 일정 시각·날짜는 그대로다.
/// 모르는 설정 값이나 소속하지 않은 조직이면 400.
#[utoipa::path(put, path = "/api/me/settings", operation_id = "set_my_settings", request_body = MySettings,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)), tag = "auth")]
pub async fn set_settings(
    State(state): State<AccountState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<MySettings>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_display_settings($1, $2, $3, $4, $5)")
        .bind(input.time_zone.as_deref())
        .bind(input.locale.as_deref())
        .bind(input.week_start.as_deref())
        .bind(input.hour_cycle.as_deref())
        .bind(input.theme.as_deref())
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    if let Some(organization) = input.last_organization_id {
        sqlx::query("SELECT app_set_last_organization($1)")
            .bind(organization)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
