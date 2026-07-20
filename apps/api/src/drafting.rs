use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    claude::{claude_error_to_anyhow, parse_text_response, ClaudeClient},
    sources::IngestedItem,
};

const SYSTEM_PROMPT: &str = r#"You are the bilingual editorial drafting service for a Vancouver social content desk.
Write concise, useful captions for people deciding what's worth doing or knowing in Vancouver this week or today.
Avoid hype, invented facts, fake dates, and direct translation stiffness.
Return only strict JSON with these string keys: caption_en, caption_zh."#;

#[derive(Debug, Clone)]
pub struct DraftingService {
    claude: ClaudeClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualDraftTopic {
    pub topic: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BilingualCaptionDraft {
    pub caption_en: String,
    pub caption_zh: String,
}

impl DraftingService {
    pub fn new(claude: ClaudeClient) -> Self {
        Self { claude }
    }

    pub async fn generate_from_ingested_item(
        &self,
        item: &IngestedItem,
    ) -> Result<BilingualCaptionDraft> {
        let prompt = ingested_item_prompt(item);
        self.generate(&prompt).await
    }

    pub async fn generate_from_manual_topic(
        &self,
        topic: &ManualDraftTopic,
    ) -> Result<BilingualCaptionDraft> {
        let prompt = manual_topic_prompt(topic)?;
        self.generate(&prompt).await
    }

    async fn generate(&self, prompt: &str) -> Result<BilingualCaptionDraft> {
        let raw = self
            .claude
            .complete_text(SYSTEM_PROMPT, prompt)
            .await
            .map_err(claude_error_to_anyhow)?;

        parse_caption_json(&raw)
    }
}

fn ingested_item_prompt(item: &IngestedItem) -> String {
    let summary = item.summary.as_deref().unwrap_or("No summary provided.");
    let published_at = item
        .source_published_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Unknown".to_owned());

    format!(
        r#"Draft paired captions from this ingested Vancouver item.

Content spine:
- Help the audience understand what's worth doing or knowing in Vancouver this week/day.
- Lead with utility, timing, place, and why it matters.
- If details are incomplete, keep the caption careful and do not invent specifics.

Item:
Title: {title}
Summary: {summary}
Source link: {link}
Published at: {published_at}

Output JSON shape:
{{"caption_en":"English caption here","caption_zh":"中文 caption here"}}"#,
        title = item.title,
        summary = summary,
        link = item.link,
    )
}

fn manual_topic_prompt(topic: &ManualDraftTopic) -> Result<String> {
    let normalized_topic = topic.topic.trim();

    if normalized_topic.is_empty() {
        anyhow::bail!("manual draft topic is required");
    }

    let notes = topic
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No additional notes provided.");

    Ok(format!(
        r#"Draft paired captions from this manual Vancouver topic.

Content spine:
- Help the audience understand what's worth doing or knowing in Vancouver this week/day.
- Lead with utility, timing, place, and why it matters.
- If details are incomplete, keep the caption careful and do not invent specifics.

Manual topic: {normalized_topic}
Notes: {notes}

Output JSON shape:
{{"caption_en":"English caption here","caption_zh":"中文 caption here"}}"#,
    ))
}

fn parse_caption_json(raw: &str) -> Result<BilingualCaptionDraft> {
    let body = parse_text_response(raw)?;
    let parsed = serde_json::from_str::<BilingualCaptionDraft>(&body)
        .context("failed to parse bilingual caption JSON")?;

    validate_caption_draft(parsed)
}

fn validate_caption_draft(draft: BilingualCaptionDraft) -> Result<BilingualCaptionDraft> {
    let caption_en = draft.caption_en.trim().to_owned();
    let caption_zh = draft.caption_zh.trim().to_owned();

    if caption_en.is_empty() {
        anyhow::bail!("English caption was empty");
    }

    if caption_zh.is_empty() {
        anyhow::bail!("Chinese caption was empty");
    }

    Ok(BilingualCaptionDraft {
        caption_en,
        caption_zh,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ingested_item_prompt, manual_topic_prompt, parse_caption_json, BilingualCaptionDraft,
        ManualDraftTopic,
    };
    use crate::sources::IngestedItem;
    use chrono::Utc;

    #[test]
    fn ingested_item_prompt_keeps_vancouver_content_spine() {
        let item = IngestedItem {
            id: 1,
            source_id: 2,
            title: "Night market opens".to_owned(),
            summary: Some("Food vendors and live music return Friday.".to_owned()),
            link: "https://example.test/night-market".to_owned(),
            media_ref: None,
            dedup_key: "abc".to_owned(),
            source_published_at: None,
            discovered_at: Utc::now(),
            ingested_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let prompt = ingested_item_prompt(&item);

        assert!(prompt.contains("what's worth doing or knowing in Vancouver"));
        assert!(prompt.contains("Night market opens"));
        assert!(prompt.contains("caption_zh"));
    }

    #[test]
    fn manual_topic_requires_non_empty_topic() {
        let topic = ManualDraftTopic {
            topic: " ".to_owned(),
            notes: None,
        };

        assert!(manual_topic_prompt(&topic).is_err());
    }

    #[test]
    fn parses_and_trims_caption_json() -> anyhow::Result<()> {
        let parsed = parse_caption_json(
            r#"```json
{"caption_en":"  Check the seawall closures before heading out. ","caption_zh":" 出門前先確認海堤封路資訊。 "}
```"#,
        )?;

        assert_eq!(
            parsed,
            BilingualCaptionDraft {
                caption_en: "Check the seawall closures before heading out.".to_owned(),
                caption_zh: "出門前先確認海堤封路資訊。".to_owned()
            }
        );

        Ok(())
    }
}
