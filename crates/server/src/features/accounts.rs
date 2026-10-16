//! OIDC 로그인·세션·로그아웃 (D-45).
//!
//! - Authorization Code + PKCE. 로그인 진행 상태(PKCE verifier, nonce, 돌아갈 경로)는
//!   `oidc_login_states`에 두고 쿠키에는 그 id만 담는다. id는 OIDC `state` 값으로도 쓰인다.
//! - 사용자는 ID 토큰의 (issuer, subject)로 식별한다. 이메일로 병합하지 않는다.
//! - 세션 토큰 원문은 쿠키에만 있고 DB에는 SHA-256 해시만 저장한다. 절대 만료는 설정값.
//! - 로그아웃은 서버 세션을 지우고, 제공자의 end_session_endpoint로 RP-initiated logout URL을 돌려준다.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
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
use crate::db::{AnonTx, Db};
use crate::error::{AppError, ErrorBody};
use crate::http::AppState;
use crate::ids::UserId;

/// 세션 쿠키 이름.
pub const SESSION_COOKIE: &str = "fvoci_session";
/// 로그인 진행 쿠키 이름.
pub const LOGIN_COOKIE: &str = "fvoci_login";
/// 로그인 진행 상태 유효 시간(초). 제공자 로그인 화면을 거치는 동안만 필요하다.
const LOGIN_STATE_TTL_SECS: i64 = 600;
/// `return_to` 최대 길이.
const MAX_RETURN_TO: usize = 512;

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
        let issuer = IssuerUrl::from_url(self.config.issuer.clone());
        let meta = ProviderMetadataWithLogout::discover_async(issuer, &self.http)
            .await
            .map_err(|e| AppError::Oidc(format!("discovery failed: {e}")))?;
        *self.metadata.write().await = Some(meta.clone());
        Ok(meta)
    }
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn cookie(
    config: &Config,
    name: &'static str,
    value: String,
    path: &'static str,
) -> Cookie<'static> {
    let mut c = Cookie::new(name, value);
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_secure(config.secure_cookies());
    c.set_path(path);
    c
}

fn removal(config: &Config, name: &'static str, path: &'static str) -> Cookie<'static> {
    let mut c = cookie(config, name, String::new(), path);
    c.make_removal();
    c
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
    let meta = provider.metadata().await?;
    let client = CoreClient::from_provider_metadata(
        meta,
        ClientId::new(provider.config.client_id.clone()),
        Some(ClientSecret::new(
            provider.config.client_secret.expose().to_owned(),
        )),
    )
    .set_redirect_uri(provider.redirect_url.clone());

    let state_id = random_token();
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let state_for_url = state_id.clone();
    let (auth_url, _csrf, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            move || CsrfToken::new(state_for_url),
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("profile".into()))
        .add_scope(Scope::new("email".into()))
        .set_pkce_challenge(challenge)
        .url();

    let mut tx = state.db.begin_anonymous().await?;
    // 만료된 진행 상태를 함께 정리한다.
    sqlx::query("DELETE FROM oidc_login_states WHERE expires_at < now()")
        .execute(tx.conn())
        .await?;
    sqlx::query(
        "INSERT INTO oidc_login_states (id, pkce_verifier, nonce, return_to, expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))",
    )
    .bind(&state_id)
    .bind(verifier.secret())
    .bind(nonce.secret())
    .bind(safe_return_to(query.return_to.as_deref()))
    .bind(LOGIN_STATE_TTL_SECS as f64)
    .execute(tx.conn())
    .await?;
    tx.commit().await?;

    let jar = jar.add(cookie(&state.config, LOGIN_COOKIE, state_id, "/auth"));
    Ok((jar, Redirect::to(auth_url.as_str())).into_response())
}

/// 제공자 콜백 매개변수.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

struct LoginState {
    pkce_verifier: String,
    nonce: String,
    return_to: String,
}

async fn take_login_state(tx: &mut AnonTx, id: &str) -> Result<Option<LoginState>, AppError> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "DELETE FROM oidc_login_states WHERE id = $1 AND expires_at > now()
         RETURNING pkce_verifier, nonce, return_to",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row.map(|(pkce_verifier, nonce, return_to)| LoginState {
        pkce_verifier,
        nonce,
        return_to,
    }))
}

/// 인증된 신원. 콜백과 테스트가 같은 경로로 세션을 만든다.
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub display_name: String,
    pub id_token: String,
}

/// 사용자를 만들거나 갱신하고 세션을 발급한다. 반환값은 쿠키에 넣을 토큰 원문.
pub async fn start_session(
    db: &Db,
    ttl: std::time::Duration,
    identity: &VerifiedIdentity,
) -> Result<(UserId, String), AppError> {
    let mut tx = db.begin_anonymous().await?;
    let user: UserId = sqlx::query_scalar("SELECT app_upsert_user($1, $2, $3, $4)")
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(&identity.email)
        .bind(&identity.display_name)
        .fetch_one(tx.conn())
        .await?;
    let token = random_token();
    let ttl_secs =
        i64::try_from(ttl.as_secs()).map_err(|_| AppError::Config("session ttl".into()))?;
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, id_token, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
    )
    .bind(token_hash(&token))
    .bind(user)
    .bind(&identity.id_token)
    .bind(ttl_secs as f64)
    .execute(tx.conn())
    .await?;
    tx.commit().await?;
    Ok((user, token))
}

/// 제공자 콜백: 코드 교환, ID 토큰 검증, 세션 발급.
#[utoipa::path(get, path = "/auth/callback", params(CallbackQuery),
    responses((status = 303, description = "로그인 완료 후 원래 경로로 이동"),
              (status = 400, description = "상태 불일치·만료", body = ErrorBody)), tag = "auth")]
pub async fn callback(
    State(state): State<AppState>,
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
    let cookie_state = jar.get(LOGIN_COOKIE).map(|c| c.value().to_owned());
    let (Some(code), Some(returned_state), Some(cookie_state)) =
        (query.code, query.state, cookie_state)
    else {
        return Err(AppError::BadRequest("missing login state".into()));
    };
    // 브라우저가 시작한 로그인과 돌아온 콜백이 같은지 확인한다 (CSRF).
    if returned_state != cookie_state {
        return Err(AppError::BadRequest("login state mismatch".into()));
    }

    let mut tx = state.db.begin_anonymous().await?;
    let login = take_login_state(&mut tx, &cookie_state)
        .await?
        .ok_or_else(|| AppError::BadRequest("login state expired".into()))?;
    tx.commit().await?;

    let meta = provider.metadata().await?;
    let client = CoreClient::from_provider_metadata(
        meta,
        ClientId::new(provider.config.client_id.clone()),
        Some(ClientSecret::new(
            provider.config.client_secret.expose().to_owned(),
        )),
    )
    .set_redirect_uri(provider.redirect_url.clone());

    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| AppError::Oidc(format!("token endpoint: {e}")))?
        .set_pkce_verifier(PkceCodeVerifier::new(login.pkce_verifier))
        .request_async(&provider.http)
        .await
        .map_err(|e| AppError::Oidc(format!("code exchange failed: {e}")))?;
    let id_token = token
        .id_token()
        .ok_or_else(|| AppError::Oidc("no id_token".into()))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(login.nonce))
        .map_err(|e| AppError::Oidc(format!("id_token rejected: {e}")))?;

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
        display_name,
        id_token: id_token.to_string(),
    };

    let (_user, session_token) =
        start_session(&state.db, state.config.session_ttl, &identity).await?;
    let jar = jar
        .remove(removal(&state.config, LOGIN_COOKIE, "/auth"))
        .add(cookie(&state.config, SESSION_COOKIE, session_token, "/"));
    Ok((jar, Redirect::to(&login.return_to)).into_response())
}

/// 로그아웃 결과. 클라이언트는 `redirect_to`로 이동해 제공자 세션도 끝낸다.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub redirect_to: String,
}

/// 로그아웃: 서버 세션 삭제 + 제공자 로그아웃 URL.
#[utoipa::path(post, path = "/auth/logout",
    responses((status = 200, description = "세션 삭제됨", body = LogoutResponse)), tag = "auth")]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<Response, AppError> {
    let home = state.config.public_url.as_str().to_owned();
    let mut redirect_to = home.clone();
    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_owned()) {
        let mut tx = state.db.begin_anonymous().await?;
        let id_token: Option<String> =
            sqlx::query_scalar("DELETE FROM sessions WHERE token_hash = $1 RETURNING id_token")
                .bind(token_hash(&token))
                .fetch_optional(tx.conn())
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
    let jar = jar.remove(removal(&state.config, SESSION_COOKIE, "/"));
    Ok((jar, Json(LogoutResponse { redirect_to })).into_response())
}

/// 요청한 사용자. 세션 쿠키가 없거나 만료되면 401.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser(pub UserId);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .ok_or(AppError::Unauthorized)?
            .value()
            .to_owned();
        let mut tx = state.db.begin_anonymous().await?;
        let user: Option<UserId> = sqlx::query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash = $1 AND expires_at > now()",
        )
        .bind(token_hash(&token))
        .fetch_optional(tx.conn())
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
}

/// 현재 로그인한 사용자.
#[utoipa::path(get, path = "/api/me",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody)), tag = "auth")]
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Me>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let (display_name, email): (String, Option<String>) =
        sqlx::query_as("SELECT display_name, email FROM users WHERE id = $1")
            .bind(user)
            .fetch_optional(tx.conn())
            .await?
            .ok_or(AppError::Unauthorized)?;
    tx.commit().await?;
    Ok(Json(Me {
        id: user,
        display_name,
        email,
    }))
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
    use super::safe_return_to;

    #[test]
    fn return_to_accepts_only_same_origin_paths() {
        assert_eq!(safe_return_to(Some("/w/abc")), "/w/abc");
        assert_eq!(safe_return_to(Some("//evil.example")), "/");
        assert_eq!(safe_return_to(Some("https://evil.example")), "/");
        assert_eq!(safe_return_to(Some("/\\evil")), "/");
        assert_eq!(safe_return_to(Some("/a\nb")), "/");
        assert_eq!(safe_return_to(None), "/");
    }
}
