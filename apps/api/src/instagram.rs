use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "instagram_account_type", rename_all = "snake_case")]
pub enum InstagramAccountType {
    Business,
    Creator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct InstagramConnection {
    pub id: bool,
    pub instagram_account_id: String,
    pub username: Option<String>,
    pub account_type: InstagramAccountType,
    pub graph_api_version: String,
    pub app_id: String,
    pub access_token: String,
    pub token_source: String,
    pub connected_by_sub: Option<String>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub connected_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstagramConnection {
    pub instagram_account_id: String,
    pub username: Option<String>,
    pub account_type: InstagramAccountType,
    pub graph_api_version: String,
    pub app_id: String,
    pub access_token: String,
    pub token_source: String,
    pub connected_by_sub: Option<String>,
}

pub async fn connect_instagram_account(
    pool: &PgPool,
    connection: &NewInstagramConnection,
) -> Result<InstagramConnection> {
    let normalized = ValidatedInstagramConnection::from_new(connection)?;
    let connected_by_sub = trimmed_optional(connection.connected_by_sub.as_deref());

    sqlx::query_as::<_, InstagramConnection>(
        r#"
        INSERT INTO instagram_connections (
            id,
            instagram_account_id,
            username,
            account_type,
            graph_api_version,
            app_id,
            access_token,
            token_source,
            connected_by_sub,
            disconnected_at
        )
        VALUES (TRUE, $1, $2, $3, $4, $5, $6, $7, $8, NULL)
        ON CONFLICT (id) DO UPDATE
        SET
            instagram_account_id = EXCLUDED.instagram_account_id,
            username = EXCLUDED.username,
            account_type = EXCLUDED.account_type,
            graph_api_version = EXCLUDED.graph_api_version,
            app_id = EXCLUDED.app_id,
            access_token = EXCLUDED.access_token,
            token_source = EXCLUDED.token_source,
            connected_by_sub = EXCLUDED.connected_by_sub,
            disconnected_at = NULL
        RETURNING
            id,
            instagram_account_id,
            username,
            account_type,
            graph_api_version,
            app_id,
            access_token,
            token_source,
            connected_by_sub,
            disconnected_at,
            connected_at,
            updated_at
        "#,
    )
    .bind(&normalized.instagram_account_id)
    .bind(&normalized.username)
    .bind(normalized.account_type)
    .bind(&normalized.graph_api_version)
    .bind(&normalized.app_id)
    .bind(&normalized.access_token)
    .bind(&normalized.token_source)
    .bind(&connected_by_sub)
    .persistent(false)
    .fetch_one(pool)
    .await
    .context("failed to connect Instagram account")
}

pub async fn find_instagram_connection(pool: &PgPool) -> Result<Option<InstagramConnection>> {
    sqlx::query_as::<_, InstagramConnection>(
        r#"
        SELECT
            id,
            instagram_account_id,
            username,
            account_type,
            graph_api_version,
            app_id,
            access_token,
            token_source,
            connected_by_sub,
            disconnected_at,
            connected_at,
            updated_at
        FROM instagram_connections
        WHERE id = TRUE
        "#,
    )
    .persistent(false)
    .fetch_optional(pool)
    .await
    .context("failed to find Instagram connection")
}

pub async fn disconnect_instagram_account(pool: &PgPool) -> Result<Option<InstagramConnection>> {
    sqlx::query_as::<_, InstagramConnection>(
        r#"
        UPDATE instagram_connections
        SET disconnected_at = NOW()
        WHERE id = TRUE AND disconnected_at IS NULL
        RETURNING
            id,
            instagram_account_id,
            username,
            account_type,
            graph_api_version,
            app_id,
            access_token,
            token_source,
            connected_by_sub,
            disconnected_at,
            connected_at,
            updated_at
        "#,
    )
    .persistent(false)
    .fetch_optional(pool)
    .await
    .context("failed to disconnect Instagram account")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedInstagramConnection {
    instagram_account_id: String,
    username: Option<String>,
    account_type: InstagramAccountType,
    graph_api_version: String,
    app_id: String,
    access_token: String,
    token_source: String,
}

impl ValidatedInstagramConnection {
    fn from_new(connection: &NewInstagramConnection) -> Result<Self> {
        let instagram_account_id = required_trimmed(
            &connection.instagram_account_id,
            "Instagram account id is required",
        )?;
        let graph_api_version = required_trimmed(
            &connection.graph_api_version,
            "Instagram Graph API version is required",
        )?;
        let app_id = required_trimmed(&connection.app_id, "Instagram app id is required")?;
        let access_token = required_trimmed(
            &connection.access_token,
            "Instagram access token is required",
        )?;
        let token_source = required_trimmed(
            &connection.token_source,
            "Instagram token source is required",
        )?;

        Ok(Self {
            instagram_account_id,
            username: trimmed_optional(connection.username.as_deref()),
            account_type: connection.account_type,
            graph_api_version,
            app_id,
            access_token,
            token_source,
        })
    }
}

fn required_trimmed(value: &str, message: &'static str) -> Result<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        anyhow::bail!(message);
    }

    Ok(trimmed.to_owned())
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{InstagramAccountType, NewInstagramConnection, ValidatedInstagramConnection};

    #[test]
    fn validates_and_trims_connection_inputs() {
        let connection = NewInstagramConnection {
            instagram_account_id: " 17841400000000000 ".to_owned(),
            username: Some(" vancouver_puls ".to_owned()),
            account_type: InstagramAccountType::Business,
            graph_api_version: " v20.0 ".to_owned(),
            app_id: " app-id ".to_owned(),
            access_token: " token ".to_owned(),
            token_source: " environment ".to_owned(),
            connected_by_sub: Some("user-1".to_owned()),
        };

        let validated = match ValidatedInstagramConnection::from_new(&connection) {
            Ok(validated) => validated,
            Err(error) => panic!("valid connection should pass: {error}"),
        };

        assert_eq!(validated.instagram_account_id, "17841400000000000");
        assert_eq!(validated.username, Some("vancouver_puls".to_owned()));
        assert_eq!(validated.graph_api_version, "v20.0");
        assert_eq!(validated.app_id, "app-id");
        assert_eq!(validated.access_token, "token");
        assert_eq!(validated.token_source, "environment");
    }

    #[test]
    fn rejects_missing_access_token() {
        let connection = NewInstagramConnection {
            instagram_account_id: "17841400000000000".to_owned(),
            username: None,
            account_type: InstagramAccountType::Creator,
            graph_api_version: "v20.0".to_owned(),
            app_id: "app-id".to_owned(),
            access_token: " ".to_owned(),
            token_source: "environment".to_owned(),
            connected_by_sub: None,
        };

        let error = match ValidatedInstagramConnection::from_new(&connection) {
            Ok(_) => panic!("missing access token should be rejected"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("access token"));
    }
}
