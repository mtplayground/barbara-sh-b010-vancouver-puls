use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

use crate::{
    drafts::{self, DraftStatus, PostDraft, UpdatePostDraft},
    instagram::InstagramConnection,
    storage::ObjectStorage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "instagram_publish_target", rename_all = "snake_case")]
pub enum InstagramPublishTarget {
    Post,
    Reel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "publish_log_status", rename_all = "snake_case")]
pub enum PublishLogStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct PublishLog {
    pub id: i64,
    pub draft_id: i64,
    pub target: InstagramPublishTarget,
    pub status: PublishLogStatus,
    pub instagram_account_id: String,
    pub asset_ref: String,
    pub graph_container_id: Option<String>,
    pub graph_media_id: Option<String>,
    pub error_message: Option<String>,
    pub requested_by_sub: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPublishLog {
    pub draft_id: i64,
    pub target: InstagramPublishTarget,
    pub status: PublishLogStatus,
    pub instagram_account_id: String,
    pub asset_ref: String,
    pub graph_container_id: Option<String>,
    pub graph_media_id: Option<String>,
    pub error_message: Option<String>,
    pub requested_by_sub: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstagramPublisher {
    client: reqwest::Client,
    graph_base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstagramPublishInput {
    pub instagram_account_id: String,
    pub access_token: String,
    pub graph_api_version: String,
    pub target: InstagramPublishTarget,
    pub media_url: String,
    pub caption: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstagramPublishSuccess {
    pub graph_container_id: String,
    pub graph_media_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedDraft {
    pub draft: PostDraft,
    pub log: PublishLog,
}

#[derive(Debug, Deserialize)]
struct GraphIdResponse {
    id: String,
}

impl InstagramPublisher {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            graph_base_url: "https://graph.facebook.com".to_owned(),
        }
    }

    pub fn with_graph_base_url(graph_base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            graph_base_url: graph_base_url.into(),
        }
    }

    pub async fn publish(&self, input: &InstagramPublishInput) -> Result<InstagramPublishSuccess> {
        let normalized = ValidatedInstagramPublishInput::from_input(input)?;
        let container_url = graph_endpoint(
            &self.graph_base_url,
            &normalized.graph_api_version,
            &normalized.instagram_account_id,
            "media",
        )?;
        let container_params = media_container_params(&normalized);
        let container = self
            .client
            .post(container_url)
            .form(&container_params)
            .send()
            .await
            .context("failed to create Instagram media container")?;
        let container = graph_json_response(container, "media container").await?;
        let publish_url = graph_endpoint(
            &self.graph_base_url,
            &normalized.graph_api_version,
            &normalized.instagram_account_id,
            "media_publish",
        )?;
        let publish = self
            .client
            .post(publish_url)
            .form(&[
                ("creation_id", container.id.as_str()),
                ("access_token", normalized.access_token.as_str()),
            ])
            .send()
            .await
            .context("failed to publish Instagram media container")?;
        let published = graph_json_response(publish, "media publish").await?;

        Ok(InstagramPublishSuccess {
            graph_container_id: container.id,
            graph_media_id: published.id,
        })
    }
}

impl Default for InstagramPublisher {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn create_publish_log(pool: &PgPool, log: &NewPublishLog) -> Result<PublishLog> {
    let normalized = ValidatedPublishLog::from_new(log)?;

    sqlx::query_as::<_, PublishLog>(
        r#"
        INSERT INTO publish_logs (
            draft_id,
            target,
            status,
            instagram_account_id,
            asset_ref,
            graph_container_id,
            graph_media_id,
            error_message,
            requested_by_sub
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING
            id,
            draft_id,
            target,
            status,
            instagram_account_id,
            asset_ref,
            graph_container_id,
            graph_media_id,
            error_message,
            requested_by_sub,
            created_at
        "#,
    )
    .bind(normalized.draft_id)
    .bind(normalized.target)
    .bind(normalized.status)
    .bind(&normalized.instagram_account_id)
    .bind(&normalized.asset_ref)
    .bind(&normalized.graph_container_id)
    .bind(&normalized.graph_media_id)
    .bind(&normalized.error_message)
    .bind(&normalized.requested_by_sub)
    .persistent(false)
    .fetch_one(pool)
    .await
    .context("failed to create publish log")
}

pub async fn list_publish_logs_for_draft(pool: &PgPool, draft_id: i64) -> Result<Vec<PublishLog>> {
    if draft_id < 1 {
        anyhow::bail!("draft id must be positive");
    }

    sqlx::query_as::<_, PublishLog>(
        r#"
        SELECT
            id,
            draft_id,
            target,
            status,
            instagram_account_id,
            asset_ref,
            graph_container_id,
            graph_media_id,
            error_message,
            requested_by_sub,
            created_at
        FROM publish_logs
        WHERE draft_id = $1
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(draft_id)
    .persistent(false)
    .fetch_all(pool)
    .await
    .with_context(|| format!("failed to list publish logs for draft `{draft_id}`"))
}

pub async fn publish_draft_to_instagram(
    pool: &PgPool,
    storage: &ObjectStorage,
    publisher: &InstagramPublisher,
    draft: &PostDraft,
    connection: &InstagramConnection,
    target: InstagramPublishTarget,
    requested_by_sub: Option<&str>,
) -> Result<PublishedDraft> {
    let asset_ref = publish_asset_ref(draft, target)?;
    let media_url = storage.public_url_for_stored_key(asset_ref)?;
    let publish_input = InstagramPublishInput {
        instagram_account_id: connection.instagram_account_id.clone(),
        access_token: connection.access_token.clone(),
        graph_api_version: connection.graph_api_version.clone(),
        target,
        media_url,
        caption: instagram_caption(draft),
    };

    match publisher.publish(&publish_input).await {
        Ok(success) => {
            let log = create_publish_log(
                pool,
                &NewPublishLog {
                    draft_id: draft.id,
                    target,
                    status: PublishLogStatus::Success,
                    instagram_account_id: connection.instagram_account_id.clone(),
                    asset_ref: asset_ref.to_owned(),
                    graph_container_id: Some(success.graph_container_id),
                    graph_media_id: Some(success.graph_media_id),
                    error_message: None,
                    requested_by_sub: requested_by_sub.map(ToOwned::to_owned),
                },
            )
            .await?;
            let update = UpdatePostDraft {
                source_item_id: None,
                caption_en: None,
                caption_zh: None,
                status: Some(DraftStatus::Published),
                rendered_post_asset_ref: None,
                rendered_reel_asset_ref: None,
                updated_by_sub: requested_by_sub.map(ToOwned::to_owned),
            };
            let updated = drafts::update_post_draft(pool, draft.id, &update)
                .await?
                .context("draft was not found")?;

            Ok(PublishedDraft {
                draft: updated,
                log,
            })
        }
        Err(error) => {
            let message = error
                .chain()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(": ");
            create_publish_log(
                pool,
                &NewPublishLog {
                    draft_id: draft.id,
                    target,
                    status: PublishLogStatus::Failure,
                    instagram_account_id: connection.instagram_account_id.clone(),
                    asset_ref: asset_ref.to_owned(),
                    graph_container_id: None,
                    graph_media_id: None,
                    error_message: Some(message.clone()),
                    requested_by_sub: requested_by_sub.map(ToOwned::to_owned),
                },
            )
            .await?;

            anyhow::bail!(message);
        }
    }
}

pub fn publish_asset_ref(draft: &PostDraft, target: InstagramPublishTarget) -> Result<&str> {
    if !matches!(draft.status, DraftStatus::Approved | DraftStatus::Scheduled) {
        anyhow::bail!("only approved or scheduled drafts can be published");
    }

    let asset_ref = match target {
        InstagramPublishTarget::Post => draft.rendered_post_asset_ref.as_deref(),
        InstagramPublishTarget::Reel => draft.rendered_reel_asset_ref.as_deref(),
    };

    asset_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("draft must have a rendered asset before publishing")
}

pub fn instagram_caption(draft: &PostDraft) -> String {
    format!("{}\n\n{}", draft.caption_en.trim(), draft.caption_zh.trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedPublishLog {
    draft_id: i64,
    target: InstagramPublishTarget,
    status: PublishLogStatus,
    instagram_account_id: String,
    asset_ref: String,
    graph_container_id: Option<String>,
    graph_media_id: Option<String>,
    error_message: Option<String>,
    requested_by_sub: Option<String>,
}

impl ValidatedPublishLog {
    fn from_new(log: &NewPublishLog) -> Result<Self> {
        if log.draft_id < 1 {
            anyhow::bail!("draft id must be positive");
        }

        let instagram_account_id = required_trimmed(
            &log.instagram_account_id,
            "Instagram account id is required",
        )?;
        let asset_ref = required_trimmed(&log.asset_ref, "published asset ref is required")?;
        let graph_container_id = trimmed_optional(log.graph_container_id.as_deref());
        let graph_media_id = trimmed_optional(log.graph_media_id.as_deref());
        let error_message = trimmed_optional(log.error_message.as_deref());

        match log.status {
            PublishLogStatus::Success if graph_media_id.is_none() => {
                anyhow::bail!("successful publish log requires Graph media id");
            }
            PublishLogStatus::Failure if error_message.is_none() => {
                anyhow::bail!("failed publish log requires an error message");
            }
            _ => {}
        }

        Ok(Self {
            draft_id: log.draft_id,
            target: log.target,
            status: log.status,
            instagram_account_id,
            asset_ref,
            graph_container_id,
            graph_media_id,
            error_message,
            requested_by_sub: trimmed_optional(log.requested_by_sub.as_deref()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedInstagramPublishInput {
    instagram_account_id: String,
    access_token: String,
    graph_api_version: String,
    target: InstagramPublishTarget,
    media_url: String,
    caption: String,
}

impl ValidatedInstagramPublishInput {
    fn from_input(input: &InstagramPublishInput) -> Result<Self> {
        let instagram_account_id = required_trimmed(
            &input.instagram_account_id,
            "Instagram account id is required",
        )?;
        let access_token =
            required_trimmed(&input.access_token, "Instagram access token is required")?;
        let graph_api_version = required_trimmed(
            &input.graph_api_version,
            "Instagram Graph API version is required",
        )?;
        let media_url = required_trimmed(&input.media_url, "Instagram media URL is required")?;
        let caption = input.caption.trim();

        if !media_url.starts_with("https://") {
            anyhow::bail!("Instagram media URL must use https");
        }

        if caption.is_empty() {
            anyhow::bail!("Instagram caption is required");
        }

        Ok(Self {
            instagram_account_id,
            access_token,
            graph_api_version,
            target: input.target,
            media_url,
            caption: caption.to_owned(),
        })
    }
}

fn media_container_params(input: &ValidatedInstagramPublishInput) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("caption", input.caption.clone()),
        ("access_token", input.access_token.clone()),
    ];

    match input.target {
        InstagramPublishTarget::Post => {
            params.push(("image_url", input.media_url.clone()));
        }
        InstagramPublishTarget::Reel => {
            params.push(("media_type", "REELS".to_owned()));
            params.push(("video_url", input.media_url.clone()));
        }
    }

    params
}

fn graph_endpoint(
    base_url: &str,
    graph_api_version: &str,
    instagram_account_id: &str,
    action: &str,
) -> Result<String> {
    let base = base_url.trim_end_matches('/');
    let version = graph_api_version.trim_matches('/');
    let account_id = instagram_account_id.trim_matches('/');
    let action = action.trim_matches('/');

    if base.is_empty() || version.is_empty() || account_id.is_empty() || action.is_empty() {
        anyhow::bail!("Instagram Graph API endpoint parts must not be empty");
    }

    Ok(format!("{base}/{version}/{account_id}/{action}"))
}

async fn graph_json_response(response: reqwest::Response, action: &str) -> Result<GraphIdResponse> {
    let status = response.status();

    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "response body unavailable".to_owned());
        anyhow::bail!(
            "Instagram Graph API {action} request failed with status {}: {}",
            status_for_log(status),
            truncate_error(&error_body)
        );
    }

    let parsed = response
        .json::<GraphIdResponse>()
        .await
        .with_context(|| format!("failed to parse Instagram Graph API {action} response"))?;

    if parsed.id.trim().is_empty() {
        anyhow::bail!("Instagram Graph API {action} response did not include an id");
    }

    Ok(parsed)
}

fn status_for_log(status: StatusCode) -> u16 {
    status.as_u16()
}

fn truncate_error(value: &str) -> String {
    const MAX_ERROR_CHARS: usize = 500;
    let sanitized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = sanitized.chars();
    let truncated = chars.by_ref().take(MAX_ERROR_CHARS).collect::<String>();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
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
    use super::{
        graph_endpoint, media_container_params, InstagramPublishInput, InstagramPublishTarget,
        NewPublishLog, PublishLogStatus, ValidatedInstagramPublishInput, ValidatedPublishLog,
    };

    #[test]
    fn post_publish_params_use_image_url_without_token_in_url() {
        let input = match ValidatedInstagramPublishInput::from_input(&InstagramPublishInput {
            instagram_account_id: "17841400000000000".to_owned(),
            access_token: "secret-token".to_owned(),
            graph_api_version: "v20.0".to_owned(),
            target: InstagramPublishTarget::Post,
            media_url: "https://cdn.example.com/rendered/post.png".to_owned(),
            caption: "English\n\n中文".to_owned(),
        }) {
            Ok(input) => input,
            Err(error) => panic!("valid input should pass: {error}"),
        };

        let params = media_container_params(&input);

        assert!(params.contains(&(
            "image_url",
            "https://cdn.example.com/rendered/post.png".to_owned()
        )));
        assert!(params.contains(&("access_token", "secret-token".to_owned())));
        assert!(!params.iter().any(|(key, _)| *key == "video_url"));
    }

    #[test]
    fn reel_publish_params_use_reels_media_type() {
        let input = match ValidatedInstagramPublishInput::from_input(&InstagramPublishInput {
            instagram_account_id: "17841400000000000".to_owned(),
            access_token: "secret-token".to_owned(),
            graph_api_version: "v20.0".to_owned(),
            target: InstagramPublishTarget::Reel,
            media_url: "https://cdn.example.com/rendered/reel.mp4".to_owned(),
            caption: "English\n\n中文".to_owned(),
        }) {
            Ok(input) => input,
            Err(error) => panic!("valid input should pass: {error}"),
        };

        let params = media_container_params(&input);

        assert!(params.contains(&("media_type", "REELS".to_owned())));
        assert!(params.contains(&(
            "video_url",
            "https://cdn.example.com/rendered/reel.mp4".to_owned()
        )));
        assert!(!params.iter().any(|(key, _)| *key == "image_url"));
    }

    #[test]
    fn publish_input_requires_https_media_url() {
        let error = match ValidatedInstagramPublishInput::from_input(&InstagramPublishInput {
            instagram_account_id: "17841400000000000".to_owned(),
            access_token: "secret-token".to_owned(),
            graph_api_version: "v20.0".to_owned(),
            target: InstagramPublishTarget::Post,
            media_url: "http://cdn.example.com/rendered/post.png".to_owned(),
            caption: "English\n\n中文".to_owned(),
        }) {
            Ok(_) => panic!("http media URL should be rejected"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("https"));
    }

    #[test]
    fn graph_endpoint_uses_version_account_and_action_path() {
        let endpoint = match graph_endpoint(
            "https://graph.facebook.com/",
            "/v20.0/",
            "/17841400000000000/",
            "/media/",
        ) {
            Ok(endpoint) => endpoint,
            Err(error) => panic!("endpoint should be valid: {error}"),
        };

        assert_eq!(
            endpoint,
            "https://graph.facebook.com/v20.0/17841400000000000/media"
        );
    }

    #[test]
    fn publish_log_success_requires_media_id() {
        let error = match ValidatedPublishLog::from_new(&NewPublishLog {
            draft_id: 12,
            target: InstagramPublishTarget::Post,
            status: PublishLogStatus::Success,
            instagram_account_id: "17841400000000000".to_owned(),
            asset_ref: "rendered/post.svg".to_owned(),
            graph_container_id: Some("container".to_owned()),
            graph_media_id: None,
            error_message: None,
            requested_by_sub: Some("user-1".to_owned()),
        }) {
            Ok(_) => panic!("successful publish log without media id should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("media id"));
    }

    #[test]
    fn publish_log_failure_requires_error_message() {
        let error = match ValidatedPublishLog::from_new(&NewPublishLog {
            draft_id: 12,
            target: InstagramPublishTarget::Post,
            status: PublishLogStatus::Failure,
            instagram_account_id: "17841400000000000".to_owned(),
            asset_ref: "rendered/post.svg".to_owned(),
            graph_container_id: None,
            graph_media_id: None,
            error_message: Some(" ".to_owned()),
            requested_by_sub: Some("user-1".to_owned()),
        }) {
            Ok(_) => panic!("failed publish log without error should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("error message"));
    }
}
