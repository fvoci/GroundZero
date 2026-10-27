//! 세션으로 요청한 사용자를 정하는 추출기와 기대 사용자 검사(D-96). 로그인·로그아웃은 서버의 `accounts`가 맡는다.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use fvoci_core::error::AppError;
use fvoci_core::ids::UserId;
use fvoci_db::Db;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// 같은 이름의 쿠키가 둘 이상 왔다(쿠키 주입 방지로 거부한다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateCookie;

/// 이름이 정확히 한 번 나온 쿠키 값. 없으면 `Ok(None)`, 둘 이상이면 오류.
pub fn single_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, DuplicateCookie> {
    let mut found: Option<String> = None;
    for header in headers.get_all(COOKIE) {
        let Ok(raw) = header.to_str() else { continue };
        for part in raw.split(';') {
            let Some((k, v)) = part.trim().split_once('=') else {
                continue;
            };
            if k == name {
                if found.is_some() {
                    return Err(DuplicateCookie);
                }
                found = Some(v.to_owned());
            }
        }
    }
    Ok(found)
}

/// 세션으로 요청한 사용자를 정하는 데 필요한 상태. 추출기는 처리기의 상태에서 `FromRef`로 꺼낸다.
#[derive(Debug, Clone)]
pub struct SessionState {
    pub db: Db,
    /// 세션 쿠키 이름(https 공개 원점이면 `__Host-` 이름).
    pub cookie_name: &'static str,
}

/// 요청한 사용자. 세션 쿠키가 없거나, 중복이거나, 만료되면 401.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser(pub UserId);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    SessionState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let state = SessionState::from_ref(state);
        let token = single_cookie(&parts.headers, state.cookie_name)
            .map_err(|DuplicateCookie| AppError::Unauthorized)?
            .ok_or(AppError::Unauthorized)?;
        let mut tx = state.db.begin_anonymous().await?;
        let user: Option<UserId> = sqlx::query_scalar("SELECT app_session_user($1)")
            .bind(token_hash(&token))
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        user.map(CurrentUser).ok_or(AppError::Unauthorized)
    }
}

/// 요청의 기대 사용자 머리. 편집·초안을 시작한 화면의 사용자 id(UUID)를 담는다.
pub const EXPECTED_USER_HEADER: &str = "x-fvoci-expected-user";

/// 기대 사용자를 확인한 요청한 사용자(#60). 사용자 초안을 들고 자동 저장·재시도하는 쓰기(문서 저장, 내 노트 만들기)에
/// 쓴다. 순서: 세션으로 사용자를 정하고(없으면 401) → 머리가 없거나 둘 이상이거나 UUID가 아니면 400 → 세션의 사용자와
/// 다르면 409 `account_changed`. 처리기 본문(트랜잭션·쓰기)보다 먼저 끝난다.
///
/// 머리는 비교에만 쓴다. 돌려주는 사용자는 언제나 세션의 사용자이고 권한도 그 사용자로 판단한다(머리로 아무것도
/// 허용하지 않는다). 다른 탭에서 계정을 바꾼 뒤 앞 계정의 화면이 보낸 요청이 새 계정의 이름으로 남지 않게 한다.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedUser(pub UserId);

impl<S> FromRequestParts<S> for ExpectedUser
where
    S: Send + Sync,
    SessionState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        let mut values = parts.headers.get_all(EXPECTED_USER_HEADER).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(AppError::BadRequest(format!(
                "send exactly one {EXPECTED_USER_HEADER} header"
            )));
        };
        check_expected_user(user, value.to_str().ok(), EXPECTED_USER_HEADER)?;
        Ok(Self(user))
    }
}

/// 기대 사용자 값(머리, 실시간 연결의 `expected_user` 질의)을 세션의 사용자와 비교한다. 값이 없거나 UUID가 아니면
/// 400, 다르면 `AccountChanged`. 비교만 하고 아무것도 허용하지 않는다. `name`은 오류 문구에 쓰는 값의 이름.
pub fn check_expected_user(user: UserId, raw: Option<&str>, name: &str) -> Result<(), AppError> {
    let expected = raw
        .and_then(|v| Uuid::parse_str(v).ok())
        .map(UserId)
        .ok_or_else(|| AppError::BadRequest(format!("{name} must be a user id")))?;
    if expected == user {
        Ok(())
    } else {
        Err(AppError::AccountChanged)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn duplicate_cookies_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=1; fvoci_session=x"));
        assert_eq!(
            single_cookie(&headers, "fvoci_session"),
            Ok(Some("x".to_owned()))
        );
        headers.append(COOKIE, HeaderValue::from_static("fvoci_session=y"));
        assert_eq!(
            single_cookie(&headers, "fvoci_session"),
            Err(DuplicateCookie)
        );
        assert_eq!(single_cookie(&HeaderMap::new(), "fvoci_session"), Ok(None));
    }
}
