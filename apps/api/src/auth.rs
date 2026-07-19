use anyhow::{bail, Context, Result};
use axum::{
    extract::{Request, State},
    http::{header::ORIGIN, HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use cookie::Cookie;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;

use crate::{
    config::AuthConfig,
    error::ApiError,
    users::{self, AuthUserProfile, User, UserRole},
};

const SESSION_COOKIE_NAME: &str = "mctai_session";

#[derive(Debug, Clone)]
pub struct AuthService {
    client: reqwest::Client,
    config: AuthConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String,
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "picture")]
    pub picture_url: Option<String>,
    pub aud: Audience,
    pub iss: String,
    pub exp: u64,
    #[serde(default)]
    pub iat: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthenticatedUser {
    pub claims: SessionClaims,
    pub user: User,
}

#[derive(Debug, Clone)]
pub struct CurrentUser(pub AuthenticatedUser);

#[derive(Clone)]
pub struct AuthLayerState {
    pub pool: PgPool,
    pub auth: AuthService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Admin,
    ManageUsers,
    ManageAllContent,
    DraftContent,
    EditContent,
    ScheduleContent,
}

#[derive(Clone)]
pub struct RoleGuardState {
    pub auth: AuthLayerState,
    pub permission: Permission,
}

impl Permission {
    pub fn allows(self, role: UserRole) -> bool {
        match self {
            Self::Admin | Self::ManageUsers => role.can_manage_users(),
            Self::ManageAllContent => role.can_manage_all_content(),
            Self::DraftContent => role.can_draft_content(),
            Self::EditContent => role.can_edit_content(),
            Self::ScheduleContent => role.can_schedule_content(),
        }
    }
}

impl AuthService {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    pub fn login_url(&self, return_to: &str) -> Result<String> {
        let mut login_url = Url::parse(&self.config.url)
            .context("MCTAI_AUTH_URL must be a valid URL")?
            .join("login")
            .context("failed to build auth login URL")?;

        login_url
            .query_pairs_mut()
            .append_pair("app_token", &self.config.app_token)
            .append_pair("return_to", return_to);

        Ok(login_url.into())
    }

    pub async fn authenticate_cookie_header(
        &self,
        pool: &PgPool,
        cookie_header: Option<&HeaderValue>,
    ) -> Result<Option<AuthenticatedUser>> {
        let Some(token) = session_cookie_value(cookie_header) else {
            return Ok(None);
        };

        let Some(claims) = self.verify_session_token(&token).await? else {
            return Ok(None);
        };

        let profile = AuthUserProfile {
            sub: claims.sub.clone(),
            email: claims.email.clone(),
            name: claims.name.clone(),
            picture_url: claims.picture_url.clone(),
        };
        let user = users::upsert_authenticated_user(pool, &profile, UserRole::Editor).await?;

        Ok(Some(AuthenticatedUser { claims, user }))
    }

    pub async fn verify_session_token(&self, token: &str) -> Result<Option<SessionClaims>> {
        let header = match decode_header(token) {
            Ok(header) => header,
            Err(error) => {
                tracing::warn!(error = ?error, "invalid session token header");
                return Ok(None);
            }
        };
        let Some(key_id) = header.kid.as_deref() else {
            tracing::warn!("session token is missing a key id");
            return Ok(None);
        };

        let jwks = self.fetch_jwks().await?;
        let Some(jwk) = jwks.find(key_id) else {
            tracing::warn!(key_id, "session token key id was not found in JWKS");
            return Ok(None);
        };

        let decoding_key =
            DecodingKey::from_jwk(jwk).context("failed to build JWKS decoding key")?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[self.config.app_token.as_str()]);
        validation.set_issuer(&[self.config.url.as_str()]);
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);

        match decode::<SessionClaims>(token, &decoding_key, &validation) {
            Ok(token_data) => Ok(Some(token_data.claims)),
            Err(error) => {
                tracing::warn!(error = ?error, "session token validation failed");
                Ok(None)
            }
        }
    }

    async fn fetch_jwks(&self) -> Result<JwkSet> {
        let response = self
            .client
            .get(&self.config.jwks_url)
            .send()
            .await
            .with_context(|| format!("failed to fetch JWKS from `{}`", self.config.jwks_url))?;

        if !response.status().is_success() {
            bail!(
                "JWKS fetch failed from `{}` with status {}",
                self.config.jwks_url,
                response.status()
            );
        }

        response
            .json::<JwkSet>()
            .await
            .context("failed to parse JWKS response")
    }
}

pub async fn require_authenticated(
    State(state): State<AuthLayerState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let authenticated_user = authenticate_request(&state, &request).await?;
    request
        .extensions_mut()
        .insert(CurrentUser(authenticated_user));

    Ok(next.run(request).await)
}

pub async fn require_permission(
    State(state): State<RoleGuardState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let authenticated_user = authenticate_request(&state.auth, &request).await?;

    if !state.permission.allows(authenticated_user.user.role) {
        return Err(ApiError::forbidden("insufficient role permissions"));
    }

    request
        .extensions_mut()
        .insert(CurrentUser(authenticated_user));

    Ok(next.run(request).await)
}

async fn authenticate_request(
    state: &AuthLayerState,
    request: &Request,
) -> Result<AuthenticatedUser, ApiError> {
    state
        .auth
        .authenticate_cookie_header(
            &state.pool,
            request.headers().get(axum::http::header::COOKIE),
        )
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("valid session is required"))
}

fn session_cookie_value(cookie_header: Option<&HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;

    Cookie::split_parse(cookie_header)
        .filter_map(|cookie| cookie.ok())
        .find(|cookie| cookie.name() == SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value_trimmed().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn frontend_root_return_to(headers: &HeaderMap, configured_self_url: Option<&str>) -> String {
    if let Some(return_to) = configured_self_url.and_then(root_url_from_public_url) {
        return return_to;
    }

    if let Some(return_to) = root_url_from_forwarded_headers(headers) {
        return return_to;
    }

    if let Some(return_to) = headers
        .get(ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .and_then(root_url_from_public_url)
    {
        return return_to;
    }

    "/".to_owned()
}

fn root_url_from_forwarded_headers(headers: &HeaderMap) -> Option<String> {
    let host = first_forwarded_value(headers.get("x-forwarded-host")?)?;
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(first_forwarded_value)
        .unwrap_or("https");

    if proto != "http" && proto != "https" {
        return None;
    }

    root_url_from_public_url(&format!("{proto}://{host}"))
}

fn first_forwarded_value(value: &HeaderValue) -> Option<&str> {
    value.to_str().ok()?.split(',').next().map(str::trim)
}

fn root_url_from_public_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }

    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);

    Some(url.into())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use crate::config::AuthConfig;

    use super::{
        frontend_root_return_to, session_cookie_value, Audience, AuthService, Permission,
        SessionClaims,
    };

    #[test]
    fn extracts_mctai_session_cookie() {
        let cookie_header =
            HeaderValue::from_static("theme=light; mctai_session=session-token; other=value");

        assert_eq!(
            session_cookie_value(Some(&cookie_header)),
            Some("session-token".to_owned())
        );
    }

    #[test]
    fn ignores_missing_session_cookie() {
        let cookie_header = HeaderValue::from_static("theme=light");

        assert_eq!(session_cookie_value(Some(&cookie_header)), None);
    }

    #[test]
    fn supports_single_or_multiple_audiences() {
        let single = Audience::One("app_token".to_owned());
        let many = Audience::Many(vec!["app_token".to_owned(), "other".to_owned()]);

        assert_ne!(single, many);
    }

    #[test]
    fn session_claims_map_picture_to_picture_url() -> Result<(), serde_json::Error> {
        let claims = serde_json::from_str::<SessionClaims>(
            r#"{
                "sub": "user_1",
                "email": "user@example.com",
                "email_verified": true,
                "name": "User",
                "picture": "https://example.com/avatar.png",
                "aud": "app_token",
                "iss": "https://auth.mctai.app",
                "exp": 4102444800
            }"#,
        )?;

        assert_eq!(
            claims.picture_url.as_deref(),
            Some("https://example.com/avatar.png")
        );

        Ok(())
    }

    #[test]
    fn builds_managed_auth_login_url() -> anyhow::Result<()> {
        let service = AuthService::new(AuthConfig {
            url: "https://auth.mctai.app".to_owned(),
            app_token: "app_token".to_owned(),
            jwks_url: "https://auth.mctai.app/.well-known/jwks.json".to_owned(),
        });

        let login_url = service.login_url("https://app.example.com/")?;
        let parsed = url::Url::parse(&login_url)?;
        let query_pairs = parsed.query_pairs().collect::<Vec<_>>();

        assert_eq!(parsed.as_str(), login_url);
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("auth.mctai.app"));
        assert_eq!(parsed.path(), "/login");
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "app_token" && value == "app_token"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "return_to" && value == "https://app.example.com/"));

        Ok(())
    }

    #[test]
    fn configured_self_url_wins_for_return_to() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("proxy.example.com"),
        );

        assert_eq!(
            frontend_root_return_to(&headers, Some("https://app.example.com/dashboard")),
            "https://app.example.com/"
        );
    }

    #[test]
    fn forwarded_host_builds_public_return_to() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("app.example.com, internal"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        assert_eq!(
            frontend_root_return_to(&headers, None),
            "https://app.example.com/"
        );
    }

    #[test]
    fn admin_permissions_allow_admin_only_actions() {
        assert!(Permission::Admin.allows(crate::users::UserRole::Admin));
        assert!(Permission::ManageUsers.allows(crate::users::UserRole::Admin));
        assert!(Permission::ManageAllContent.allows(crate::users::UserRole::Admin));

        assert!(!Permission::Admin.allows(crate::users::UserRole::Editor));
        assert!(!Permission::ManageUsers.allows(crate::users::UserRole::Editor));
        assert!(!Permission::ManageAllContent.allows(crate::users::UserRole::Editor));
    }

    #[test]
    fn editor_permissions_allow_content_actions() {
        assert!(Permission::DraftContent.allows(crate::users::UserRole::Editor));
        assert!(Permission::EditContent.allows(crate::users::UserRole::Editor));
        assert!(Permission::ScheduleContent.allows(crate::users::UserRole::Editor));
    }
}
