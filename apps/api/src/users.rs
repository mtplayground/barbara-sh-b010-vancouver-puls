use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "user_role", rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Editor,
}

impl UserRole {
    pub fn can_manage_users(self) -> bool {
        matches!(self, Self::Admin)
    }

    pub fn can_manage_all_content(self) -> bool {
        matches!(self, Self::Admin)
    }

    pub fn can_draft_content(self) -> bool {
        matches!(self, Self::Admin | Self::Editor)
    }

    pub fn can_edit_content(self) -> bool {
        matches!(self, Self::Admin | Self::Editor)
    }

    pub fn can_schedule_content(self) -> bool {
        matches!(self, Self::Admin | Self::Editor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct User {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    pub picture_url: Option<String>,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUserProfile {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    pub picture_url: Option<String>,
}

pub async fn upsert_authenticated_user(
    pool: &PgPool,
    profile: &AuthUserProfile,
    default_role: UserRole,
) -> Result<User> {
    sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (sub, email, name, picture_url, role)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (sub) DO UPDATE
        SET
            email = EXCLUDED.email,
            name = EXCLUDED.name,
            picture_url = EXCLUDED.picture_url,
            last_seen_at = NOW()
        RETURNING sub, email, name, picture_url, role, created_at, updated_at, last_seen_at
        "#,
    )
    .bind(&profile.sub)
    .bind(&profile.email)
    .bind(&profile.name)
    .bind(&profile.picture_url)
    .bind(default_role)
    .persistent(false)
    .fetch_one(pool)
    .await
    .with_context(|| format!("failed to upsert user `{}`", profile.sub))
}

pub async fn find_user_by_sub(pool: &PgPool, sub: &str) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT sub, email, name, picture_url, role, created_at, updated_at, last_seen_at
        FROM users
        WHERE sub = $1
        "#,
    )
    .bind(sub)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to find user `{sub}`"))
}

pub async fn list_users(pool: &PgPool) -> Result<Vec<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT sub, email, name, picture_url, role, created_at, updated_at, last_seen_at
        FROM users
        ORDER BY lower(email) ASC
        "#,
    )
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list users")
}

pub async fn update_user_role(pool: &PgPool, sub: &str, role: UserRole) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET role = $2
        WHERE sub = $1
        RETURNING sub, email, name, picture_url, role, created_at, updated_at, last_seen_at
        "#,
    )
    .bind(sub)
    .bind(role)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to update role for user `{sub}`"))
}

#[cfg(test)]
mod tests {
    use super::UserRole;

    #[test]
    fn admin_has_full_management_permissions() {
        let role = UserRole::Admin;

        assert!(role.can_manage_users());
        assert!(role.can_manage_all_content());
        assert!(role.can_draft_content());
        assert!(role.can_edit_content());
        assert!(role.can_schedule_content());
    }

    #[test]
    fn editor_can_work_on_content_without_user_management() {
        let role = UserRole::Editor;

        assert!(!role.can_manage_users());
        assert!(!role.can_manage_all_content());
        assert!(role.can_draft_content());
        assert!(role.can_edit_content());
        assert!(role.can_schedule_content());
    }
}
