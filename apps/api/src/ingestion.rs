use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use reqwest::{header::CONTENT_TYPE, Client, StatusCode};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{collections::HashSet, time::Duration};
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};
use url::Url;

use crate::{
    sources::{self, ContentSource, ContentSourceKind, NewIngestedItem},
    storage::ObjectStorage,
};

const INGESTION_INTERVAL: Duration = Duration::from_secs(15 * 60);
const MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestionRunSummary {
    pub sources_seen: usize,
    pub sources_polled: usize,
    pub items_seen: usize,
    pub items_queued: usize,
    pub items_deduplicated: usize,
    pub media_cached: usize,
}

#[derive(Clone)]
pub struct IngestionService {
    pool: PgPool,
    storage: Option<ObjectStorage>,
    http: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedSourceItem {
    title: String,
    summary: Option<String>,
    link: String,
    media_url: Option<String>,
    dedup_key: String,
    source_published_at: Option<DateTime<Utc>>,
}

impl IngestionService {
    pub fn new(pool: PgPool, storage: Option<ObjectStorage>) -> Result<Self> {
        let http = Client::builder()
            .user_agent("barbara-sh-b010-vancouver-puls-ingestion/0.1")
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .context("failed to build ingestion http client")?;

        Ok(Self {
            pool,
            storage,
            http,
        })
    }

    pub async fn run_once(&self) -> Result<IngestionRunSummary> {
        let all_sources = sources::list_content_sources(&self.pool).await?;
        let mut summary = IngestionRunSummary {
            sources_seen: all_sources.len(),
            sources_polled: 0,
            items_seen: 0,
            items_queued: 0,
            items_deduplicated: 0,
            media_cached: 0,
        };

        for source in all_sources.into_iter().filter(|source| source.enabled) {
            match self.ingest_source(&source).await {
                Ok(source_summary) => {
                    summary.sources_polled += 1;
                    summary.items_seen += source_summary.items_seen;
                    summary.items_queued += source_summary.items_queued;
                    summary.items_deduplicated += source_summary.items_deduplicated;
                    summary.media_cached += source_summary.media_cached;
                }
                Err(error) => {
                    warn!(
                        source_id = source.id,
                        source_name = %source.name,
                        error = ?error,
                        "source ingestion failed"
                    );
                }
            }
        }

        Ok(summary)
    }

    async fn ingest_source(&self, source: &ContentSource) -> Result<IngestionRunSummary> {
        let Some(source_url) = source.url.as_deref() else {
            info!(
                source_id = source.id,
                source_name = %source.name,
                "source has no url; skipping scheduled fetch"
            );
            return Ok(empty_polled_summary());
        };

        if matches!(
            source.kind,
            ContentSourceKind::Instagram | ContentSourceKind::Manual
        ) {
            info!(
                source_id = source.id,
                source_kind = ?source.kind,
                "source kind is not fetched by scheduled web ingestion"
            );
            return Ok(empty_polled_summary());
        }

        let body = self
            .fetch_text(source_url)
            .await
            .with_context(|| format!("failed to fetch source `{source_url}`"))?;
        let normalized_items = normalize_items(source, source_url, &body);
        let mut summary = IngestionRunSummary {
            sources_seen: 1,
            sources_polled: 1,
            items_seen: normalized_items.len(),
            items_queued: 0,
            items_deduplicated: 0,
            media_cached: 0,
        };
        let mut seen_dedup_keys = HashSet::new();

        for normalized in normalized_items {
            if !seen_dedup_keys.insert(normalized.dedup_key.clone())
                || self
                    .is_duplicate_topic(source.id, &normalized.dedup_key)
                    .await?
            {
                summary.items_deduplicated += 1;
                continue;
            }

            let (media_ref, cached) = self.cache_media_if_available(source.id, &normalized).await;
            if cached {
                summary.media_cached += 1;
            }

            let item = NewIngestedItem {
                source_id: source.id,
                title: normalized.title,
                summary: normalized.summary,
                link: normalized.link,
                media_ref,
                dedup_key: normalized.dedup_key,
                source_published_at: normalized.source_published_at,
            };

            sources::upsert_ingested_item(&self.pool, &item).await?;
            summary.items_queued += 1;
        }

        Ok(summary)
    }

    async fn is_duplicate_topic(&self, source_id: i64, dedup_key: &str) -> Result<bool> {
        let existing =
            sources::find_ingested_item_by_dedup_key_any_source(&self.pool, dedup_key).await?;

        Ok(existing.is_some_and(|item| item.source_id != source_id))
    }

    async fn fetch_text(&self, url: &str) -> Result<String> {
        let response = self.http.get(url).send().await?;
        ensure_success(response.status(), url)?;
        response
            .text()
            .await
            .context("failed to read response body")
    }

    async fn cache_media_if_available(
        &self,
        source_id: i64,
        item: &NormalizedSourceItem,
    ) -> (Option<String>, bool) {
        let Some(media_url) = item.media_url.as_deref() else {
            return (None, false);
        };
        let Some(storage) = &self.storage else {
            return (Some(media_url.to_owned()), false);
        };

        match self.fetch_media(media_url).await {
            Ok(media) => {
                let key = format!(
                    "cached/source-media/{}/{}",
                    source_id,
                    media_cache_key(media_url, &media.bytes)
                );
                match storage
                    .put_bytes(&key, media.bytes, media.content_type.as_deref())
                    .await
                {
                    Ok(stored) => (Some(stored.key), true),
                    Err(error) => {
                        warn!(
                            source_id,
                            media_url,
                            error = ?error,
                            "failed to cache media in object storage"
                        );
                        (Some(media_url.to_owned()), false)
                    }
                }
            }
            Err(error) => {
                warn!(
                    source_id,
                    media_url,
                    error = ?error,
                    "failed to fetch media"
                );
                (Some(media_url.to_owned()), false)
            }
        }
    }

    async fn fetch_media(&self, media_url: &str) -> Result<FetchedMedia> {
        let response = self.http.get(media_url).send().await?;
        ensure_success(response.status(), media_url)?;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let bytes = response
            .bytes()
            .await
            .context("failed to read media response body")?;

        if bytes.len() > MAX_MEDIA_BYTES {
            anyhow::bail!("media response exceeded {} bytes", MAX_MEDIA_BYTES);
        }

        Ok(FetchedMedia {
            bytes: bytes.to_vec(),
            content_type,
        })
    }
}

struct FetchedMedia {
    bytes: Vec<u8>,
    content_type: Option<String>,
}

pub fn spawn_ingestion_job(pool: PgPool, storage: Option<ObjectStorage>) -> Result<JoinHandle<()>> {
    let service = IngestionService::new(pool, storage)?;

    Ok(tokio::spawn(async move {
        let mut ticker = interval(INGESTION_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            match service.run_once().await {
                Ok(summary) => {
                    info!(
                        sources_seen = summary.sources_seen,
                        sources_polled = summary.sources_polled,
                        items_seen = summary.items_seen,
                        items_queued = summary.items_queued,
                        items_deduplicated = summary.items_deduplicated,
                        media_cached = summary.media_cached,
                        "scheduled ingestion run completed"
                    );
                }
                Err(error) => {
                    warn!(error = ?error, "scheduled ingestion run failed");
                }
            }
        }
    }))
}

fn normalize_items(
    source: &ContentSource,
    source_url: &str,
    body: &str,
) -> Vec<NormalizedSourceItem> {
    match source.kind {
        ContentSourceKind::Rss => normalize_feed_items(source, source_url, body),
        ContentSourceKind::Website => vec![normalize_website_item(source, source_url, body)],
        ContentSourceKind::Instagram | ContentSourceKind::Manual => Vec::new(),
    }
}

fn normalize_feed_items(
    source: &ContentSource,
    source_url: &str,
    body: &str,
) -> Vec<NormalizedSourceItem> {
    let mut items = extract_blocks(body, "item");

    if items.is_empty() {
        items = extract_blocks(body, "entry");
    }

    items
        .into_iter()
        .filter_map(|block| normalize_feed_block(source, source_url, &block))
        .collect()
}

fn normalize_feed_block(
    source: &ContentSource,
    source_url: &str,
    block: &str,
) -> Option<NormalizedSourceItem> {
    let title = first_tag_text(block, &["title"]).unwrap_or_else(|| source.name.clone());
    let link = first_tag_text(block, &["link"])
        .or_else(|| first_attribute(block, "link", "href"))
        .unwrap_or_else(|| source_url.to_owned());
    let summary = first_tag_text(block, &["description", "summary", "content"]);
    let media_url = first_attribute(block, "enclosure", "url")
        .or_else(|| first_attribute(block, "media:content", "url"))
        .or_else(|| first_attribute(block, "media:thumbnail", "url"));
    let link = trim_to_non_empty(&resolve_url(source_url, &link))?;
    let summary = summary.and_then(|value| trim_to_non_empty(&value));
    let title = trim_to_non_empty(&title)?;

    Some(NormalizedSourceItem {
        dedup_key: topic_dedup_key(&title, &link, summary.as_deref()),
        title,
        summary,
        link,
        media_url: media_url
            .and_then(|value| trim_to_non_empty(&value))
            .map(|value| resolve_url(source_url, &value)),
        source_published_at: None,
    })
}

fn normalize_website_item(
    source: &ContentSource,
    source_url: &str,
    body: &str,
) -> NormalizedSourceItem {
    let title = html_title(body)
        .and_then(|value| trim_to_non_empty(&value))
        .unwrap_or_else(|| source.name.clone());
    let summary =
        html_meta_content(body, "description").and_then(|value| trim_to_non_empty(&value));
    let media_url = html_meta_property(body, "og:image")
        .and_then(|value| trim_to_non_empty(&value))
        .map(|value| resolve_url(source_url, &value));
    let dedup_key = topic_dedup_key(&title, source_url, summary.as_deref());

    NormalizedSourceItem {
        title,
        summary,
        link: source_url.to_owned(),
        media_url,
        dedup_key,
        source_published_at: None,
    }
}

fn extract_blocks(body: &str, tag: &str) -> Vec<String> {
    let lower = body.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut cursor = 0;
    let mut blocks = Vec::new();

    while let Some(relative_start) = lower[cursor..].find(&open) {
        let start = cursor + relative_start;
        let Some(open_end) = lower[start..].find('>') else {
            break;
        };
        let content_start = start + open_end + 1;
        let Some(relative_end) = lower[content_start..].find(&close) else {
            break;
        };
        let end = content_start + relative_end;
        blocks.push(body[content_start..end].to_owned());
        cursor = end + close.len();
    }

    blocks
}

fn first_tag_text(block: &str, tags: &[&str]) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag_text(block, tag))
        .map(|value| decode_entities(strip_cdata(&value).trim()))
}

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let lower = block.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = lower.find(&open)?;
    let open_end = lower[start..].find('>')?;
    let content_start = start + open_end + 1;
    let end = lower[content_start..].find(&close)?;

    Some(block[content_start..content_start + end].to_owned())
}

fn first_attribute(block: &str, tag: &str, attribute: &str) -> Option<String> {
    let lower = block.to_lowercase();
    let open = format!("<{tag}");
    let start = lower.find(&open)?;
    let tag_end = lower[start..].find('>')?;
    let tag_body = &block[start..start + tag_end + 1];

    attribute_value(tag_body, attribute)
}

fn html_title(body: &str) -> Option<String> {
    first_tag_text(body, &["title"])
}

fn html_meta_content(body: &str, name: &str) -> Option<String> {
    find_meta_attribute(body, "name", name, "content")
}

fn html_meta_property(body: &str, property: &str) -> Option<String> {
    find_meta_attribute(body, "property", property, "content")
}

fn find_meta_attribute(body: &str, key: &str, expected: &str, value_attr: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let mut cursor = 0;

    while let Some(relative_start) = lower[cursor..].find("<meta") {
        let start = cursor + relative_start;
        let Some(relative_end) = lower[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        let tag = &body[start..end];
        let key_value = attribute_value(tag, key);

        if key_value
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
        {
            return attribute_value(tag, value_attr);
        }

        cursor = end;
    }

    None
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let needle = format!("{attribute}=");
    let start = lower.find(&needle)? + needle.len();
    let quote = tag[start..].chars().next()?;

    if quote != '"' && quote != '\'' {
        return None;
    }

    let value_start = start + quote.len_utf8();
    let value_end = tag[value_start..].find(quote)?;

    Some(decode_entities(&tag[value_start..value_start + value_end]))
}

fn strip_cdata(value: &str) -> &str {
    value
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(value)
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn trim_to_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn topic_dedup_key(title: &str, link: &str, summary: Option<&str>) -> String {
    let topic = normalize_topic_text(title);

    if topic.len() >= 12 {
        return digest_key(&format!("topic:{topic}"));
    }

    let canonical_link = canonicalize_url(link);
    let fallback = summary
        .map(normalize_topic_text)
        .filter(|value| value.len() >= 12)
        .unwrap_or(canonical_link);

    digest_key(&format!("item:{fallback}"))
}

fn normalize_topic_text(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_space = true;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_owned()
}

fn canonicalize_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return normalize_topic_text(value);
    };

    url.set_query(None);
    url.set_fragment(None);
    let host = url.host_str().map(str::to_ascii_lowercase);

    if let Some(host) = host {
        let _ = url.set_host(Some(&host));
    }

    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(if path.is_empty() { "/" } else { &path });
    url.to_string()
}

fn resolve_url(base_url: &str, candidate: &str) -> String {
    if Url::parse(candidate).is_ok() {
        return candidate.to_owned();
    }

    Url::parse(base_url)
        .and_then(|base| base.join(candidate))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| candidate.to_owned())
}

fn digest_key(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
}

fn media_cache_key(media_url: &str, bytes: &[u8]) -> String {
    digest_key(&format!(
        "{}:{}",
        media_url,
        URL_SAFE_NO_PAD.encode(Sha256::digest(bytes))
    ))
}

fn ensure_success(status: StatusCode, url: &str) -> Result<()> {
    if !status.is_success() {
        anyhow::bail!("request to `{url}` failed with status {status}");
    }

    Ok(())
}

fn empty_polled_summary() -> IngestionRunSummary {
    IngestionRunSummary {
        sources_seen: 1,
        sources_polled: 1,
        items_seen: 0,
        items_queued: 0,
        items_deduplicated: 0,
        media_cached: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_blocks, html_meta_property, normalize_feed_items, normalize_website_item,
        topic_dedup_key,
    };
    use crate::sources::{ContentSource, ContentSourceKind};
    use chrono::Utc;

    #[test]
    fn feed_items_are_normalized_with_stable_dedup_keys() {
        let source = test_source(ContentSourceKind::Rss);
        let body = r#"
            <rss><channel>
                <item>
                    <guid>event-1</guid>
                    <title><![CDATA[Vancouver Night Market]]></title>
                    <description>Food and music &amp; vendors</description>
                    <link>https://events.example.com/night-market</link>
                    <enclosure url="https://cdn.example.com/night.jpg" />
                </item>
            </channel></rss>
        "#;

        let items = normalize_feed_items(&source, "https://events.example.com/rss", body);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Vancouver Night Market");
        assert_eq!(
            items[0].summary,
            Some("Food and music & vendors".to_owned())
        );
        assert_eq!(items[0].link, "https://events.example.com/night-market");
        assert_eq!(
            items[0].media_url,
            Some("https://cdn.example.com/night.jpg".to_owned())
        );
    }

    #[test]
    fn website_item_uses_title_description_and_open_graph_image() {
        let source = test_source(ContentSourceKind::Website);
        let body = r#"
            <html>
                <head>
                    <title>Vancouver Arts Update</title>
                    <meta name="description" content="Gallery openings this week">
                    <meta property="og:image" content="https://news.example.com/art.jpg">
                </head>
            </html>
        "#;

        let item = normalize_website_item(&source, "https://news.example.com", body);

        assert_eq!(item.title, "Vancouver Arts Update");
        assert_eq!(item.summary, Some("Gallery openings this week".to_owned()));
        assert_eq!(
            item.media_url,
            Some("https://news.example.com/art.jpg".to_owned())
        );
    }

    #[test]
    fn xml_block_extraction_handles_multiple_items() {
        let blocks = extract_blocks(
            "<item><title>A</title></item><item><title>B</title></item>",
            "item",
        );

        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn meta_lookup_matches_expected_property() {
        assert_eq!(
            html_meta_property(
                r#"<meta property="og:image" content="https://example.com/image.jpg">"#,
                "og:image"
            ),
            Some("https://example.com/image.jpg".to_owned())
        );
    }

    #[test]
    fn topic_dedup_key_collapses_same_title_across_links() {
        let first = topic_dedup_key(
            "Vancouver Night Market",
            "https://events.example.com/night-market?utm_source=feed",
            None,
        );
        let second = topic_dedup_key(
            "vancouver night market!",
            "https://other.example.com/story/123",
            None,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn topic_dedup_key_canonicalizes_tracking_urls_for_short_titles() {
        let first = topic_dedup_key(
            "BC",
            "https://news.example.com/story?utm_campaign=social#comments",
            None,
        );
        let second = topic_dedup_key("BC", "https://news.example.com/story", None);

        assert_eq!(first, second);
    }

    fn test_source(kind: ContentSourceKind) -> ContentSource {
        ContentSource {
            id: 42,
            name: "Vancouver Source".to_owned(),
            kind,
            url: Some("https://example.com".to_owned()),
            external_id: None,
            created_by_sub: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
