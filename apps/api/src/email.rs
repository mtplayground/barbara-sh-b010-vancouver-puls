use anyhow::{bail, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::EmailConfig;

#[derive(Debug, Clone)]
pub struct EmailService {
    client: reqwest::Client,
    config: EmailConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailSendError {
    RateLimited,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EmailDelivery {
    pub message_id: String,
}

#[derive(Debug, Serialize)]
struct SendEmailRequest<'a> {
    to: &'a str,
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct SendEmailResponse {
    id: String,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    pub async fn send_invite(
        &self,
        to: &str,
        invite_url: &str,
    ) -> Result<EmailDelivery, EmailSendError> {
        let safe_url = escape_html(invite_url);
        let html = format!(
            "<p>You have been invited to collaborate as an editor.</p><p><a href=\"{safe_url}\">Accept your invitation</a></p>"
        );
        let text = format!("You have been invited to collaborate as an editor: {invite_url}");

        self.send_email(to, "You have been invited", &html, &text)
            .await
    }

    async fn send_email(
        &self,
        to: &str,
        subject: &str,
        html: &str,
        text: &str,
    ) -> Result<EmailDelivery, EmailSendError> {
        let response = self
            .client
            .post(&self.config.url)
            .bearer_auth(&self.config.app_token)
            .json(&SendEmailRequest {
                to,
                subject,
                html,
                text,
            })
            .send()
            .await
            .map_err(|error| {
                EmailSendError::Failed(format!("email send request failed: {error}"))
            })?;

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(EmailSendError::RateLimited);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|error| format!("failed to read error response: {error}"));
            return Err(EmailSendError::Failed(format!(
                "email send failed with status {status}: {body}"
            )));
        }

        let body = response
            .json::<SendEmailResponse>()
            .await
            .map_err(|error| {
                EmailSendError::Failed(format!("email response parse failed: {error}"))
            })?;

        Ok(EmailDelivery {
            message_id: body.id,
        })
    }
}

pub fn email_error_to_anyhow(error: EmailSendError) -> anyhow::Error {
    match error {
        EmailSendError::RateLimited => anyhow::anyhow!("email send rate limited"),
        EmailSendError::Failed(message) => anyhow::anyhow!(message),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn validate_email_address(email: &str) -> Result<String> {
    let normalized = email.trim().to_lowercase();

    if normalized.is_empty()
        || normalized.len() > 254
        || normalized.contains(char::is_whitespace)
        || !normalized.contains('@')
    {
        bail!("email address is invalid");
    }

    let mut parts = normalized.split('@');
    let Some(local) = parts.next() else {
        bail!("email address is invalid");
    };
    let Some(domain) = parts.next() else {
        bail!("email address is invalid");
    };

    if parts.next().is_some()
        || local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        bail!("email address is invalid");
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{escape_html, validate_email_address};

    #[test]
    fn normalizes_email_address() -> anyhow::Result<()> {
        assert_eq!(
            validate_email_address(" Editor@Example.COM ")?,
            "editor@example.com"
        );

        Ok(())
    }

    #[test]
    fn rejects_invalid_email_address() {
        assert!(validate_email_address("not-an-email").is_err());
        assert!(validate_email_address("bad @example.com").is_err());
    }

    #[test]
    fn escapes_invite_links_for_html() {
        assert_eq!(
            escape_html("https://app.test/a?x=1&y=\"z\""),
            "https://app.test/a?x=1&amp;y=&quot;z&quot;"
        );
    }
}
