use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "content_source_kind", rename_all = "snake_case")]
pub enum ContentSourceKind {
    Rss,
    Website,
    Instagram,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct ContentSource {
    pub id: i64,
    pub name: String,
    pub kind: ContentSourceKind,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub created_by_sub: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewContentSource {
    pub name: String,
    pub kind: ContentSourceKind,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub created_by_sub: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateContentSource {
    pub name: Option<String>,
    pub kind: Option<ContentSourceKind>,
    pub url: Option<Option<String>>,
    pub external_id: Option<Option<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct IngestedItem {
    pub id: i64,
    pub source_id: i64,
    pub title: String,
    pub summary: Option<String>,
    pub link: String,
    pub media_ref: Option<String>,
    pub dedup_key: String,
    pub source_published_at: Option<DateTime<Utc>>,
    pub discovered_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewIngestedItem {
    pub source_id: i64,
    pub title: String,
    pub summary: Option<String>,
    pub link: String,
    pub media_ref: Option<String>,
    pub dedup_key: String,
    pub source_published_at: Option<DateTime<Utc>>,
}

pub async fn create_content_source(
    pool: &PgPool,
    source: &NewContentSource,
) -> Result<ContentSource> {
    let normalized = ValidatedContentSourceInput::from_new(source)?;

    sqlx::query_as::<_, ContentSource>(
        r#"
        INSERT INTO content_sources (name, kind, url, external_id, created_by_sub)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        "#,
    )
    .bind(&normalized.name)
    .bind(normalized.kind)
    .bind(&normalized.url)
    .bind(&normalized.external_id)
    .bind(trimmed_optional(source.created_by_sub.as_deref()))
    .persistent(false)
    .fetch_one(pool)
    .await
    .with_context(|| format!("failed to create content source `{}`", source.name))
}

pub async fn list_content_sources(pool: &PgPool) -> Result<Vec<ContentSource>> {
    sqlx::query_as::<_, ContentSource>(
        r#"
        SELECT id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        FROM content_sources
        ORDER BY enabled DESC, lower(name) ASC
        "#,
    )
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list content sources")
}

pub async fn find_content_source(pool: &PgPool, source_id: i64) -> Result<Option<ContentSource>> {
    sqlx::query_as::<_, ContentSource>(
        r#"
        SELECT id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        FROM content_sources
        WHERE id = $1
        "#,
    )
    .bind(source_id)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to find content source `{source_id}`"))
}

pub async fn update_content_source(
    pool: &PgPool,
    source_id: i64,
    update: &UpdateContentSource,
) -> Result<Option<ContentSource>> {
    let existing = find_content_source(pool, source_id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    let normalized = ValidatedContentSourceInput::from_update(&existing, update)?;

    sqlx::query_as::<_, ContentSource>(
        r#"
        UPDATE content_sources
        SET
            name = $2,
            kind = $3,
            url = $4,
            external_id = $5,
            enabled = $6
        WHERE id = $1
        RETURNING id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        "#,
    )
    .bind(source_id)
    .bind(&normalized.name)
    .bind(normalized.kind)
    .bind(&normalized.url)
    .bind(&normalized.external_id)
    .bind(update.enabled.unwrap_or(existing.enabled))
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to update content source `{source_id}`"))
}

pub async fn set_content_source_enabled(
    pool: &PgPool,
    source_id: i64,
    enabled: bool,
) -> Result<Option<ContentSource>> {
    sqlx::query_as::<_, ContentSource>(
        r#"
        UPDATE content_sources
        SET enabled = $2
        WHERE id = $1
        RETURNING id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        "#,
    )
    .bind(source_id)
    .bind(enabled)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to update content source `{source_id}`"))
}

pub async fn delete_content_source(pool: &PgPool, source_id: i64) -> Result<Option<ContentSource>> {
    sqlx::query_as::<_, ContentSource>(
        r#"
        DELETE FROM content_sources
        WHERE id = $1
        RETURNING id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        "#,
    )
    .bind(source_id)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to delete content source `{source_id}`"))
}

pub async fn upsert_ingested_item(pool: &PgPool, item: &NewIngestedItem) -> Result<IngestedItem> {
    sqlx::query_as::<_, IngestedItem>(
        r#"
        INSERT INTO ingested_items (
            source_id,
            title,
            summary,
            link,
            media_ref,
            dedup_key,
            source_published_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (source_id, dedup_key) DO UPDATE
        SET
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            link = EXCLUDED.link,
            media_ref = EXCLUDED.media_ref,
            source_published_at = EXCLUDED.source_published_at
        RETURNING
            id,
            source_id,
            title,
            summary,
            link,
            media_ref,
            dedup_key,
            source_published_at,
            discovered_at,
            ingested_at,
            updated_at
        "#,
    )
    .bind(item.source_id)
    .bind(item.title.trim())
    .bind(trimmed_optional(item.summary.as_deref()))
    .bind(item.link.trim())
    .bind(trimmed_optional(item.media_ref.as_deref()))
    .bind(item.dedup_key.trim())
    .bind(item.source_published_at)
    .persistent(false)
    .fetch_one(pool)
    .await
    .with_context(|| {
        format!(
            "failed to upsert ingested item `{}` for source `{}`",
            item.dedup_key, item.source_id
        )
    })
}

pub async fn find_ingested_item_by_dedup_key(
    pool: &PgPool,
    source_id: i64,
    dedup_key: &str,
) -> Result<Option<IngestedItem>> {
    sqlx::query_as::<_, IngestedItem>(
        r#"
        SELECT
            id,
            source_id,
            title,
            summary,
            link,
            media_ref,
            dedup_key,
            source_published_at,
            discovered_at,
            ingested_at,
            updated_at
        FROM ingested_items
        WHERE source_id = $1 AND dedup_key = $2
        "#,
    )
    .bind(source_id)
    .bind(dedup_key)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to find ingested item `{dedup_key}` for source `{source_id}`"))
}

pub async fn find_ingested_item_by_dedup_key_any_source(
    pool: &PgPool,
    dedup_key: &str,
) -> Result<Option<IngestedItem>> {
    sqlx::query_as::<_, IngestedItem>(
        r#"
        SELECT
            id,
            source_id,
            title,
            summary,
            link,
            media_ref,
            dedup_key,
            source_published_at,
            discovered_at,
            ingested_at,
            updated_at
        FROM ingested_items
        WHERE dedup_key = $1
        ORDER BY ingested_at DESC
        LIMIT 1
        "#,
    )
    .bind(dedup_key)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to find ingested item by dedup key `{dedup_key}`"))
}

pub async fn list_recent_ingested_items(pool: &PgPool, limit: i64) -> Result<Vec<IngestedItem>> {
    let bounded_limit = limit.clamp(1, 100);

    sqlx::query_as::<_, IngestedItem>(
        r#"
        SELECT
            id,
            source_id,
            title,
            summary,
            link,
            media_ref,
            dedup_key,
            source_published_at,
            discovered_at,
            ingested_at,
            updated_at
        FROM ingested_items
        ORDER BY ingested_at DESC
        LIMIT $1
        "#,
    )
    .bind(bounded_limit)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list recent ingested items")
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedContentSourceInput {
    name: String,
    kind: ContentSourceKind,
    url: Option<String>,
    external_id: Option<String>,
}

impl ValidatedContentSourceInput {
    fn from_new(source: &NewContentSource) -> Result<Self> {
        validate_source_input(
            source.name.trim(),
            source.kind,
            trimmed_optional(source.url.as_deref()),
            trimmed_optional(source.external_id.as_deref()),
        )
    }

    fn from_update(existing: &ContentSource, update: &UpdateContentSource) -> Result<Self> {
        validate_source_input(
            update
                .name
                .as_deref()
                .map(str::trim)
                .unwrap_or(existing.name.as_str()),
            update.kind.unwrap_or(existing.kind),
            match &update.url {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.url.clone(),
            },
            match &update.external_id {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.external_id.clone(),
            },
        )
    }
}

fn validate_source_input(
    name: &str,
    kind: ContentSourceKind,
    url: Option<String>,
    external_id: Option<String>,
) -> Result<ValidatedContentSourceInput> {
    if name.is_empty() {
        anyhow::bail!("source name is required");
    }

    if let Some(source_url) = &url {
        let parsed = Url::parse(source_url)
            .with_context(|| format!("source url `{source_url}` is invalid"))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            anyhow::bail!("source url must use http or https");
        }
    }

    if url.is_none() && external_id.is_none() {
        anyhow::bail!("source url or external id is required");
    }

    Ok(ValidatedContentSourceInput {
        name: name.to_owned(),
        kind,
        url,
        external_id,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        validate_source_input, ContentSource, ContentSourceKind, UpdateContentSource,
        ValidatedContentSourceInput,
    };
    use chrono::Utc;

    #[test]
    fn trimmed_optional_drops_blank_values() {
        assert_eq!(super::trimmed_optional(None), None);
        assert_eq!(super::trimmed_optional(Some("   ")), None);
        assert_eq!(
            super::trimmed_optional(Some(" https://example.com ")),
            Some("https://example.com".to_owned())
        );
    }

    #[test]
    fn validate_source_input_requires_http_url_or_external_id() {
        assert!(validate_source_input("News", ContentSourceKind::Website, None, None).is_err());
        assert!(validate_source_input(
            "News",
            ContentSourceKind::Website,
            Some("ftp://example.com".to_owned()),
            None
        )
        .is_err());
        let validated = match validate_source_input(
            "News",
            ContentSourceKind::Website,
            Some("https://example.com".to_owned()),
            None,
        ) {
            Ok(validated) => validated,
            Err(error) => panic!("valid source was rejected: {error}"),
        };

        assert_eq!(
            validated,
            ValidatedContentSourceInput {
                name: "News".to_owned(),
                kind: ContentSourceKind::Website,
                url: Some("https://example.com".to_owned()),
                external_id: None
            }
        );
    }

    #[test]
    fn update_input_preserves_existing_values() {
        let existing = ContentSource {
            id: 1,
            name: "Events".to_owned(),
            kind: ContentSourceKind::Rss,
            url: Some("https://example.com/rss".to_owned()),
            external_id: None,
            created_by_sub: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let update = UpdateContentSource {
            name: Some("Event feed".to_owned()),
            kind: None,
            url: None,
            external_id: None,
            enabled: Some(false),
        };

        let normalized = match ValidatedContentSourceInput::from_update(&existing, &update) {
            Ok(normalized) => normalized,
            Err(error) => panic!("valid update was rejected: {error}"),
        };

        assert_eq!(normalized.name, "Event feed");
        assert_eq!(normalized.kind, ContentSourceKind::Rss);
        assert_eq!(normalized.url, Some("https://example.com/rss".to_owned()));
    }
}
