use anyhow::{bail, Context, Result};
use axum::http::HeaderValue;
use cookie::Cookie;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{
    config::AuthConfig,
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

impl AuthService {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
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

fn session_cookie_value(cookie_header: Option<&HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;

    Cookie::split_parse(cookie_header)
        .filter_map(|cookie| cookie.ok())
        .find(|cookie| cookie.name() == SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value_trimmed().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::{session_cookie_value, Audience, SessionClaims};

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
}
