use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};

use crate::users::UserRole;

const INVITE_TOKEN_BYTES: usize = 32;
const INVITE_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct UserInvite {
    pub email: String,
    pub role: UserRole,
    pub invited_by_sub: String,
    pub accepted_by_sub: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedInvite {
    pub invite: UserInvite,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptInviteError {
    InvalidOrExpired,
}

pub async fn create_editor_invite(
    pool: &PgPool,
    email: &str,
    invited_by_sub: &str,
) -> Result<CreatedInvite> {
    let token = generate_invite_token();
    let token_hash = hash_invite_token(&token);
    let expires_at = Utc::now() + Duration::days(INVITE_TTL_DAYS);

    let invite = sqlx::query_as::<_, UserInvite>(
        r#"
        INSERT INTO user_invites (token_hash, email, role, invited_by_sub, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING email, role, invited_by_sub, accepted_by_sub, created_at, expires_at, accepted_at
        "#,
    )
    .bind(token_hash)
    .bind(email)
    .bind(UserRole::Editor)
    .bind(invited_by_sub)
    .bind(expires_at)
    .persistent(false)
    .fetch_one(pool)
    .await
    .with_context(|| format!("failed to create invite for `{email}`"))?;

    Ok(CreatedInvite { invite, token })
}

pub async fn accept_invite(
    pool: &PgPool,
    token: &str,
    accepted_by_sub: &str,
    accepted_email: &str,
) -> Result<std::result::Result<UserInvite, AcceptInviteError>> {
    let token_hash = hash_invite_token(token);
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin invite transaction")?;

    let invite = sqlx::query_as::<_, UserInvite>(
        r#"
        UPDATE user_invites
        SET accepted_at = NOW(), accepted_by_sub = $3
        WHERE token_hash = $1
            AND lower(email) = lower($2)
            AND accepted_at IS NULL
            AND expires_at > NOW()
        RETURNING email, role, invited_by_sub, accepted_by_sub, created_at, expires_at, accepted_at
        "#,
    )
    .bind(token_hash)
    .bind(accepted_email)
    .bind(accepted_by_sub)
    .persistent(false)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to accept invite")?;

    let Some(invite) = invite else {
        tx.rollback()
            .await
            .context("failed to roll back invalid invite transaction")?;
        return Ok(Err(AcceptInviteError::InvalidOrExpired));
    };

    sqlx::query(
        r#"
        UPDATE users
        SET role = $2, updated_at = NOW()
        WHERE sub = $1
        "#,
    )
    .bind(accepted_by_sub)
    .bind(invite.role)
    .persistent(false)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("failed to apply invited role for user `{accepted_by_sub}`"))?;

    tx.commit()
        .await
        .context("failed to commit invite transaction")?;

    Ok(Ok(invite))
}

pub fn generate_invite_token() -> String {
    let mut bytes = [0_u8; INVITE_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_invite_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::{generate_invite_token, hash_invite_token};

    #[test]
    fn generated_tokens_are_url_safe_and_randomized() {
        let first = generate_invite_token();
        let second = generate_invite_token();

        assert_ne!(first, second);
        assert!(!first.contains('='));
        assert!(first.len() >= 40);
    }

    #[test]
    fn token_hashes_are_stable_without_exposing_token() {
        let token = "invite-token";

        assert_eq!(hash_invite_token(token), hash_invite_token(token));
        assert_ne!(hash_invite_token(token), token);
    }
}
