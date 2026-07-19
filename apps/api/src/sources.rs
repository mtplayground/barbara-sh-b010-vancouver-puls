use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

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
    sqlx::query_as::<_, ContentSource>(
        r#"
        INSERT INTO content_sources (name, kind, url, external_id, created_by_sub)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, kind, url, external_id, created_by_sub, enabled, created_at, updated_at
        "#,
    )
    .bind(source.name.trim())
    .bind(source.kind)
    .bind(trimmed_optional(source.url.as_deref()))
    .bind(trimmed_optional(source.external_id.as_deref()))
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

#[cfg(test)]
mod tests {
    use super::trimmed_optional;

    #[test]
    fn trimmed_optional_drops_blank_values() {
        assert_eq!(trimmed_optional(None), None);
        assert_eq!(trimmed_optional(Some("   ")), None);
        assert_eq!(
            trimmed_optional(Some(" https://example.com ")),
            Some("https://example.com".to_owned())
        );
    }
}
