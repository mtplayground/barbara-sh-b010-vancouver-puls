use anyhow::Result;
use reqwest::{header::HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    config::ClaudeConfig,
    retry::{is_retryable_status_code, retry_transient, EXTERNAL_HTTP_RETRY},
};

pub const CLAUDE_DRAFTING_MODEL: &str = "claude-opus-4-8";

const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct ClaudeClient {
    client: reqwest::Client,
    config: ClaudeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeError {
    RateLimited,
    Transient(String),
    Failed(String),
}

#[derive(Debug, Serialize)]
struct ClaudeMessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    temperature: f32,
    system: &'a str,
    messages: Vec<ClaudeMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessagesResponse {
    content: Vec<ClaudeContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

impl ClaudeClient {
    pub fn new(config: ClaudeConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    pub async fn complete_text(
        &self,
        system: &str,
        user_prompt: &str,
    ) -> Result<String, ClaudeError> {
        retry_transient(
            EXTERNAL_HTTP_RETRY,
            "Claude text completion",
            |_| self.complete_text_once(system, user_prompt),
            is_transient_claude_error,
        )
        .await
    }

    async fn complete_text_once(
        &self,
        system: &str,
        user_prompt: &str,
    ) -> Result<String, ClaudeError> {
        let response = self
            .client
            .post(ANTHROPIC_MESSAGES_URL)
            .header(
                "anthropic-version",
                HeaderValue::from_static(ANTHROPIC_VERSION),
            )
            .header("x-api-key", &self.config.api_key)
            .json(&ClaudeMessagesRequest {
                model: CLAUDE_DRAFTING_MODEL,
                max_tokens: 900,
                temperature: 0.6,
                system,
                messages: vec![ClaudeMessage {
                    role: "user",
                    content: user_prompt,
                }],
            })
            .send()
            .await
            .map_err(|error| ClaudeError::Transient(format!("claude request failed: {error}")))?;

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(ClaudeError::RateLimited);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|error| format!("failed to read error response: {error}"));
            let message = format!("claude request failed with status {status}: {body}");

            if is_retryable_status_code(status) {
                return Err(ClaudeError::Transient(message));
            }

            return Err(ClaudeError::Failed(message));
        }

        let body = response
            .json::<ClaudeMessagesResponse>()
            .await
            .map_err(|error| {
                ClaudeError::Failed(format!("claude response parse failed: {error}"))
            })?;

        first_text_block(body).ok_or_else(|| {
            ClaudeError::Failed("claude response did not include a text block".to_owned())
        })
    }
}

pub fn claude_error_to_anyhow(error: ClaudeError) -> anyhow::Error {
    match error {
        ClaudeError::RateLimited => anyhow::anyhow!("claude request rate limited"),
        ClaudeError::Transient(message) => anyhow::anyhow!(message),
        ClaudeError::Failed(message) => anyhow::anyhow!(message),
    }
}

fn is_transient_claude_error(error: &ClaudeError) -> bool {
    matches!(error, ClaudeError::RateLimited | ClaudeError::Transient(_))
}

fn first_text_block(response: ClaudeMessagesResponse) -> Option<String> {
    response.content.into_iter().find_map(|block| match block {
        ClaudeContentBlock::Text { text } => Some(text),
        ClaudeContentBlock::Other => None,
    })
}

pub fn parse_text_response(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if without_fence.is_empty() {
        anyhow::bail!("claude text response was empty");
    }

    Ok(without_fence.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{is_transient_claude_error, parse_text_response, ClaudeError};

    #[test]
    fn strips_json_code_fence_from_response() -> anyhow::Result<()> {
        assert_eq!(
            parse_text_response("```json\n{\"caption_en\":\"A\"}\n```")?,
            "{\"caption_en\":\"A\"}"
        );

        Ok(())
    }

    #[test]
    fn rejects_empty_text_response() {
        assert!(parse_text_response("   ").is_err());
    }

    #[test]
    fn classifies_only_retryable_claude_errors_as_transient() {
        assert!(is_transient_claude_error(&ClaudeError::RateLimited));
        assert!(is_transient_claude_error(&ClaudeError::Transient(
            "status 503".to_owned()
        )));
        assert!(!is_transient_claude_error(&ClaudeError::Failed(
            "parse failed".to_owned()
        )));
    }
}
