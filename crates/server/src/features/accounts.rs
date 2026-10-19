//! OIDC 로그인·세션·로그아웃 (D-45).
//!
//! - Authorization Code + PKCE. 로그인 진행 상태(OIDC state, PKCE verifier, nonce, 돌아갈 경로)는
//!   `oidc_login_states`에 두고, 브라우저 쿠키에는 별도의 무작위 비밀값만 담는다. 행은 그 비밀값의
//!   해시로 찾고, 돌아온 `state`는 행에 저장한 값과 비교한다. 콜백 URL만 가진 사람은 로그인을 끝낼 수 없다.
//! - 사용자는 ID 토큰의 (issuer, subject)로 식별한다. 이메일로 병합하지 않는다.
//! - 세션 토큰 원문은 쿠키에만 있고 DB에는 SHA-256 해시만 있다. 앱 역할은 세션·로그인 상태 테이블을
//!   직접 읽지 못하고 해시를 받는 정의자 함수만 부른다. 로그인 때 기존 세션은 회전(삭제)한다.
//! - https 공개 원점에서는 `__Host-` 쿠키 이름을 쓴다. 같은 이름의 쿠키가 둘 이상 오면 거부한다(쿠키 주입 방지).
//! - 로그아웃은 서버 세션을 지우고 제공자의 end_session_endpoint로 RP-initiated logout URL을 돌려준다.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, ProviderMetadataWithLogout, RedirectUrl, Scope, TokenResponse as _,
};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::sync::RwLock;
use utoipa::{IntoParams, ToSchema};

use crate::config::{Config, OidcConfig};
use crate::db::Db;
use crate::error::{AppError, ErrorBody};
use crate::http::AppState;
use crate::ids::UserId;
use uuid::Uuid;

/// http 공개 원점(개발·테스트)의 세션 쿠키 이름.
pub const SESSION_COOKIE: &str = "fvoci_session";
/// https 공개 원점의 세션 쿠키 이름. `__Host-`는 Secure·Path=/·Domain 없음을 브라우저가 강제한다.
pub const SECURE_SESSION_COOKIE: &str = "__Host-fvoci_session";
const LOGIN_COOKIE: &str = "fvoci_login";
const SECURE_LOGIN_COOKIE: &str = "__Host-fvoci_login";
/// 로그인 진행 상태 유효 시간(초). 제공자 로그인 화면을 거치는 동안만 필요하다.
const LOGIN_STATE_TTL_SECS: i32 = 600;
/// `return_to` 최대 길이.
const MAX_RETURN_TO: usize = 512;

/// 공개 원점에 맞는 세션 쿠키 이름.
pub fn session_cookie_name(config: &Config) -> &'static str {
    if config.secure_cookies() {
        SECURE_SESSION_COOKIE
    } else {
        SESSION_COOKIE
    }
}

fn login_cookie_name(config: &Config) -> &'static str {
    if config.secure_cookies() {
        SECURE_LOGIN_COOKIE
    } else {
        LOGIN_COOKIE
    }
}

/// OIDC 제공자 연결. 디스커버리는 첫 사용 시 가져와 캐시한다(서버 시작을 제공자 기동에 묶지 않는다).
#[derive(Debug)]
pub struct OidcProvider {
    config: OidcConfig,
    redirect_url: RedirectUrl,
    http: reqwest::Client,
    metadata: RwLock<Option<ProviderMetadataWithLogout>>,
}

impl OidcProvider {
    pub fn new(config: &OidcConfig, public_url: &url::Url) -> Result<Self, AppError> {
        let redirect = public_url
            .join("/auth/callback")
            .map_err(|e| AppError::Config(format!("redirect url: {e}")))?;
        let http = reqwest::Client::builder()
            // 토큰 요청에서 리다이렉트를 따라가지 않는다 (openidconnect 권장, SSRF 방지).
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AppError::Config(format!("http client: {e}")))?;
        Ok(Self {
            config: config.clone(),
            redirect_url: RedirectUrl::from_url(redirect),
            http,
            metadata: RwLock::new(None),
        })
    }

    async fn metadata(&self) -> Result<ProviderMetadataWithLogout, AppError> {
        if let Some(meta) = self.metadata.read().await.as_ref() {
            return Ok(meta.clone());
        }
        self.refresh_metadata().await
    }

    /// 디스커버리와 서명 키 목록을 다시 받아 캐시를 바꾼다. 제공자가 서명 키를 바꾸면(키 회전) 캐시한 목록으로는
    /// 새 id_token을 검증하지 못한다.
    async fn refresh_metadata(&self) -> Result<ProviderMetadataWithLogout, AppError> {
        let issuer = IssuerUrl::from_url(self.config.issuer.clone());
        let meta = ProviderMetadataWithLogout::discover_async(issuer, &self.http)
            .await
            .map_err(|e| AppError::Oidc(format!("discovery failed: {e}")))?;
        *self.metadata.write().await = Some(meta.clone());
        Ok(meta)
    }
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// 요청의 세션 쿠키 토큰 해시(실시간 연결이 세션이 살아 있는지 다시 볼 때 쓴다).
pub(crate) fn session_token_hash(headers: &HeaderMap, config: &Config) -> Option<Vec<u8>> {
    single_cookie(headers, session_cookie_name(config))
        .ok()
        .flatten()
        .map(|token| token_hash(&token))
}

fn cookie(config: &Config, name: &'static str, value: String) -> Cookie<'static> {
    let mut c = Cookie::new(name, value);
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_secure(config.secure_cookies());
    c.set_path("/");
    c
}

fn removal(config: &Config, name: &'static str) -> Cookie<'static> {
    let mut c = cookie(config, name, String::new());
    c.make_removal();
    c
}

/// 이름이 정확히 한 번 나온 쿠키 값. 없으면 `Ok(None)`, 둘 이상이면 오류.
fn single_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, ()> {
    let mut found: Option<String> = None;
    for header in headers.get_all(COOKIE) {
        let Ok(raw) = header.to_str() else { continue };
        for part in raw.split(';') {
            let Some((k, v)) = part.trim().split_once('=') else {
                continue;
            };
            if k == name {
                if found.is_some() {
                    return Err(());
                }
                found = Some(v.to_owned());
            }
        }
    }
    Ok(found)
}

/// `return_to`는 같은 원점의 상대 경로만 허용한다(오픈 리다이렉트 방지).
fn safe_return_to(raw: Option<&str>) -> String {
    match raw {
        Some(p)
            if p.starts_with('/')
                && !p.starts_with("//")
                && !p.contains('\\')
                && !p.chars().any(char::is_control)
                && p.len() <= MAX_RETURN_TO =>
        {
            p.to_owned()
        }
        _ => "/".to_owned(),
    }
}

/// 로그인 시작 매개변수.
#[derive(Debug, Deserialize, IntoParams)]
pub struct LoginQuery {
    /// 로그인 후 돌아갈 같은 원점 경로. 기본 `/`.
    pub return_to: Option<String>,
}

fn client_for(
    provider: &OidcProvider,
    meta: ProviderMetadataWithLogout,
) -> impl FnOnce() -> openidconnect::core::CoreClient<
    openidconnect::EndpointSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointMaybeSet,
    openidconnect::EndpointMaybeSet,
> {
    let id = ClientId::new(provider.config.client_id.clone());
    let secret = ClientSecret::new(provider.config.client_secret.expose().to_owned());
    let redirect = provider.redirect_url.clone();
    move || CoreClient::from_provider_metadata(meta, id, Some(secret)).set_redirect_uri(redirect)
}

/// 로그인 시작: 제공자 인증 화면으로 보낸다.
#[utoipa::path(get, path = "/auth/login", params(LoginQuery),
    responses((status = 303, description = "제공자 로그인 화면으로 이동")), tag = "auth")]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<LoginQuery>,
) -> Result<Response, AppError> {
    let provider = state
        .oidc
        .as_ref()
        .ok_or(AppError::Config("OIDC is not configured".into()))?;
    let client = client_for(provider, provider.metadata().await?)();

    let secret = random_token();
    let oidc_state = random_token();
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let state_for_url = oidc_state.clone();
    // openid 범위는 라이브러리가 붙인다.
    let (auth_url, _csrf, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            move || CsrfToken::new(state_for_url),
            Nonce::new_random,
        )
        .add_scope(Scope::new("profile".into()))
        .add_scope(Scope::new("email".into()))
        .set_pkce_challenge(challenge)
        .url();

    let mut tx = state.db.begin_anonymous().await?;
    sqlx::query("SELECT app_create_login_state($1, $2, $3, $4, $5, $6)")
        .bind(token_hash(&secret))
        .bind(&oidc_state)
        .bind(verifier.secret())
        .bind(nonce.secret())
        .bind(safe_return_to(query.return_to.as_deref()))
        .bind(LOGIN_STATE_TTL_SECS)
        .execute(tx.conn())
        .await?;
    tx.commit().await?;

    let jar = jar.add(cookie(
        &state.config,
        login_cookie_name(&state.config),
        secret,
    ));
    Ok((jar, Redirect::to(auth_url.as_str())).into_response())
}

/// 제공자 콜백 매개변수.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// 인증된 신원. 콜백과 테스트가 같은 경로로 세션을 만든다.
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    /// 제공자가 이메일을 검증했는지(`email_verified`). 초대 대상 확인에만 쓰고 계정 병합에는 쓰지 않는다.
    pub email_verified: bool,
    pub display_name: String,
    pub id_token: String,
}

/// 사용자를 만들거나 갱신하고 세션을 발급한다. `replaces`가 있으면 그 세션을 끝낸다(회전).
/// 반환값은 쿠키에 넣을 토큰 원문.
pub async fn start_session(
    db: &Db,
    ttl: std::time::Duration,
    identity: &VerifiedIdentity,
    replaces: Option<&str>,
) -> Result<(UserId, String), AppError> {
    let ttl_secs =
        i64::try_from(ttl.as_secs()).map_err(|_| AppError::Config("session ttl".into()))?;
    let mut tx = db.begin_anonymous().await?;
    let user: UserId = sqlx::query_scalar("SELECT app_upsert_user($1, $2, $3, $4, $5)")
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(&identity.email)
        .bind(identity.email_verified)
        .bind(&identity.display_name)
        .fetch_one(tx.conn())
        .await?;
    let token = random_token();
    sqlx::query("SELECT app_create_session($1, $2, $3, $4, $5)")
        .bind(token_hash(&token))
        .bind(user)
        .bind(&identity.id_token)
        .bind(ttl_secs)
        .bind(replaces.map(token_hash))
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    Ok((user, token))
}

/// 운영자가 지정한 초기 관리자(D-51)가 로그인하면 첫 워크스페이스를 만들어 소유자로 둔다. 인스턴스에서 한 번만.
/// 지정이 없거나 다른 사람이면 아무것도 하지 않는다(첫 로그인 사용자를 자동 관리자로 만들지 않음, X-10).
pub async fn claim_initial_admin(
    db: &Db,
    config: &Config,
    identity: &VerifiedIdentity,
    user: UserId,
) -> Result<Option<Uuid>, AppError> {
    let (Some(subject), Some(oidc)) = (
        config.initial_admin_subject.as_deref(),
        config.oidc.as_ref(),
    ) else {
        return Ok(None);
    };
    let same_issuer =
        identity.issuer.trim_end_matches('/') == oidc.issuer.as_str().trim_end_matches('/');
    if !same_issuer || identity.subject != subject {
        return Ok(None);
    }
    let mut tx = db.begin_anonymous().await?;
    let workspace: Option<Uuid> =
        sqlx::query_scalar("SELECT app_system_claim_initial_workspace($1, $2)")
            .bind(user)
            .bind(&config.initial_workspace)
            .fetch_one(tx.conn())
            .await?;
    tx.commit().await?;
    Ok(workspace)
}

/// 제공자 콜백: 상태 확인, 코드 교환, ID 토큰 검증, 세션 발급.
#[utoipa::path(get, path = "/auth/callback", params(CallbackQuery),
    responses((status = 303, description = "로그인 완료 후 원래 경로로 이동"),
              (status = 400, description = "상태 불일치·만료", body = ErrorBody)), tag = "auth")]
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    let provider = state
        .oidc
        .as_ref()
        .ok_or(AppError::Config("OIDC is not configured".into()))?;
    if let Some(err) = query.error {
        return Err(AppError::BadRequest(format!(
            "login was not completed: {err}"
        )));
    }
    let secret = single_cookie(&headers, login_cookie_name(&state.config))
        .map_err(|()| AppError::BadRequest("duplicate login cookie".into()))?;
    let (Some(code), Some(returned_state), Some(secret)) = (query.code, query.state, secret) else {
        return Err(AppError::BadRequest("missing login state".into()));
    };

    // 쿠키 비밀값과 돌아온 state가 모두 맞아야 상태를 꺼낸다(CSRF). 맞지 않으면 상태는 그대로 남아
    // 진짜 콜백이 이어서 끝낼 수 있다.
    let mut tx = state.db.begin_anonymous().await?;
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT pkce_verifier, nonce, return_to FROM app_take_login_state($1, $2)")
            .bind(token_hash(&secret))
            .bind(&returned_state)
            .fetch_optional(tx.conn())
            .await?;
    tx.commit().await?;
    let (pkce_verifier, nonce, return_to) =
        row.ok_or_else(|| AppError::BadRequest("login state expired or mismatched".into()))?;

    let client = client_for(provider, provider.metadata().await?)();
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| AppError::Oidc(format!("token endpoint: {e}")))?
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(&provider.http)
        .await
        .map_err(|e| AppError::Oidc(format!("code exchange failed: {e}")))?;
    let id_token = token
        .id_token()
        .ok_or_else(|| AppError::Oidc("no id_token".into()))?;
    let nonce = Nonce::new(nonce);
    let claims = match id_token.claims(&client.id_token_verifier(), &nonce) {
        Ok(claims) => claims,
        // 토큰은 제공자의 토큰 엔드포인트에서 바로 받았다. 서명 검증에 실패하면 키 회전일 수 있으니 키 목록을 한 번만
        // 다시 받아 같은 토큰을 다시 검증한다(그래도 실패하면 거부). 그 밖의 실패(nonce·기한·대상)는 바로 거부.
        Err(openidconnect::ClaimsVerificationError::SignatureVerification(_)) => {
            let fresh = client_for(provider, provider.refresh_metadata().await?)();
            id_token
                .claims(&fresh.id_token_verifier(), &nonce)
                .map_err(|e| AppError::Oidc(format!("id_token rejected: {e}")))?
        }
        Err(e) => return Err(AppError::Oidc(format!("id_token rejected: {e}"))),
    };

    let subject = claims.subject().as_str().to_owned();
    let email = claims.email().map(|e| e.as_str().to_owned());
    let display_name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.as_str().to_owned())
        .or_else(|| claims.preferred_username().map(|u| u.as_str().to_owned()))
        .or_else(|| email.clone())
        .unwrap_or_else(|| subject.clone());
    let identity = VerifiedIdentity {
        issuer: claims.issuer().as_str().to_owned(),
        subject,
        email,
        email_verified: claims.email_verified().unwrap_or(false),
        display_name,
        id_token: id_token.to_string(),
    };

    // 이미 로그인 중이었다면 그 세션은 끝낸다(회전). 중복 쿠키면 회전 대상 없이 진행한다.
    let previous = single_cookie(&headers, session_cookie_name(&state.config))
        .ok()
        .flatten();
    let (user, session_token) = start_session(
        &state.db,
        state.config.session_ttl,
        &identity,
        previous.as_deref(),
    )
    .await?;
    claim_initial_admin(&state.db, &state.config, &identity, user).await?;
    let jar = jar
        .remove(removal(&state.config, login_cookie_name(&state.config)))
        .add(cookie(
            &state.config,
            session_cookie_name(&state.config),
            session_token,
        ));
    Ok((jar, Redirect::to(&return_to)).into_response())
}

/// 로그아웃 결과. 클라이언트는 `redirect_to`로 이동해 제공자 세션도 끝낸다.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub redirect_to: String,
}

/// 로그아웃: 서버 세션 삭제 + 제공자 로그아웃 URL.
#[utoipa::path(post, path = "/auth/logout",
    responses((status = 200, description = "세션 삭제됨", body = LogoutResponse)), tag = "auth")]
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, AppError> {
    let home = state.config.public_url.as_str().to_owned();
    let mut redirect_to = home.clone();
    // 같은 이름 쿠키가 둘 이상이면 어느 세션도 끝내지 않되, 제거 쿠키는 보내 사용자가 빠져나올 수 있게 한다.
    let token = single_cookie(&headers, session_cookie_name(&state.config))
        .ok()
        .flatten();
    if let Some(token) = token {
        let mut tx = state.db.begin_anonymous().await?;
        let id_token: Option<String> = sqlx::query_scalar("SELECT app_end_session($1)")
            .bind(token_hash(&token))
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        if let (Some(id_token), Some(provider)) = (id_token, state.oidc.as_ref()) {
            let meta = provider.metadata().await?;
            if let Some(end) = meta.additional_metadata().end_session_endpoint.as_ref() {
                let mut url = end.url().clone();
                url.query_pairs_mut()
                    .append_pair("id_token_hint", &id_token)
                    .append_pair("post_logout_redirect_uri", &home);
                redirect_to = url.to_string();
            }
        }
    }
    let jar = jar.remove(removal(&state.config, session_cookie_name(&state.config)));
    Ok((jar, Json(LogoutResponse { redirect_to })).into_response())
}

/// 요청한 사용자. 세션 쿠키가 없거나, 중복이거나, 만료되면 401.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser(pub UserId);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = single_cookie(&parts.headers, session_cookie_name(&state.config))
            .map_err(|()| AppError::Unauthorized)?
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
}

/// 현재 로그인한 사용자.
#[utoipa::path(get, path = "/api/me",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody)), tag = "auth")]
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Me>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    type MeRow = (String, Option<String>, String, String, String, String);
    let (display_name, email, time_zone, locale, week_start, hour_cycle): MeRow = sqlx::query_as(
        "SELECT u.display_name, u.email, coalesce(s.time_zone, ''), coalesce(s.locale, ''),
                coalesce(s.week_start, ''), coalesce(s.hour_cycle, '')
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
    }))
}

/// 내 표시 설정 바꾸기. 보내지 않은 칸은 그대로 둔다.
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
}

/// 내 표시 설정 바꾸기(D-50 시간대, D-81 언어·주 시작·시각 표시). 표시만 바꾸고 저장된 일정 시각·날짜는 그대로다.
/// 모르는 값이면 400.
#[utoipa::path(put, path = "/api/me/settings", operation_id = "set_my_settings", request_body = MySettings,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)), tag = "auth")]
pub async fn set_settings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<MySettings>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_display_settings($1, $2, $3, $4)")
        .bind(input.time_zone.as_deref())
        .bind(input.locale.as_deref())
        .bind(input.week_start.as_deref())
        .bind(input.hour_cycle.as_deref())
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 로그인 가능 여부. 웹이 로그인 버튼 표시에 쓴다.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthInfo {
    pub oidc_enabled: bool,
}

#[utoipa::path(get, path = "/api/auth-info", responses((status = 200, body = AuthInfo)), tag = "auth")]
pub async fn auth_info(State(state): State<AppState>) -> Json<AuthInfo> {
    Json(AuthInfo {
        oidc_enabled: state.oidc.is_some(),
    })
}

/// 상태에 둘 제공자 핸들.
pub type SharedProvider = Option<Arc<OidcProvider>>;

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn return_to_accepts_only_same_origin_paths() {
        assert_eq!(safe_return_to(Some("/w/abc")), "/w/abc");
        assert_eq!(safe_return_to(Some("//evil.example")), "/");
        assert_eq!(safe_return_to(Some("https://evil.example")), "/");
        assert_eq!(safe_return_to(Some("/\\evil")), "/");
        assert_eq!(safe_return_to(Some("/a\nb")), "/");
        assert_eq!(safe_return_to(None), "/");
    }

    #[test]
    fn duplicate_cookies_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=1; fvoci_session=x"));
        assert_eq!(
            single_cookie(&headers, "fvoci_session"),
            Ok(Some("x".to_owned()))
        );
        headers.append(COOKIE, HeaderValue::from_static("fvoci_session=y"));
        assert_eq!(single_cookie(&headers, "fvoci_session"), Err(()));
        assert_eq!(single_cookie(&HeaderMap::new(), "fvoci_session"), Ok(None));
    }
}
