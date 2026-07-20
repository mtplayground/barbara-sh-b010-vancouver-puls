use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "backup_content_kind", rename_all = "snake_case")]
pub enum BackupContentKind {
    PastRecap,
    DidYouKnow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct BackupContentItem {
    pub id: i64,
    pub kind: BackupContentKind,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    pub media_ref: Option<String>,
    pub active: bool,
    pub created_by_sub: Option<String>,
    pub updated_by_sub: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBackupContentItem {
    pub kind: BackupContentKind,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    pub media_ref: Option<String>,
    pub active: Option<bool>,
    pub created_by_sub: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateBackupContentItem {
    pub kind: Option<BackupContentKind>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub source_url: Option<Option<String>>,
    pub media_ref: Option<Option<String>>,
    pub active: Option<bool>,
    pub updated_by_sub: Option<String>,
}

pub async fn create_backup_content_item(
    pool: &PgPool,
    item: &NewBackupContentItem,
) -> Result<BackupContentItem> {
    let normalized = ValidatedBackupContentInput::from_new(item)?;
    let created_by_sub = trimmed_optional(item.created_by_sub.as_deref());

    sqlx::query_as::<_, BackupContentItem>(
        r#"
        INSERT INTO backup_content_items (
            kind,
            title,
            body,
            source_url,
            media_ref,
            active,
            created_by_sub,
            updated_by_sub
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
        RETURNING
            id,
            kind,
            title,
            body,
            source_url,
            media_ref,
            active,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(normalized.kind)
    .bind(&normalized.title)
    .bind(&normalized.body)
    .bind(&normalized.source_url)
    .bind(&normalized.media_ref)
    .bind(normalized.active)
    .bind(&created_by_sub)
    .persistent(false)
    .fetch_one(pool)
    .await
    .context("failed to create backup content item")
}

pub async fn find_backup_content_item(
    pool: &PgPool,
    item_id: i64,
) -> Result<Option<BackupContentItem>> {
    sqlx::query_as::<_, BackupContentItem>(BACKUP_CONTENT_SELECT_BY_ID)
        .bind(item_id)
        .persistent(false)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("failed to find backup content item `{item_id}`"))
}

pub async fn list_backup_content_items(
    pool: &PgPool,
    limit: i64,
    active: Option<bool>,
) -> Result<Vec<BackupContentItem>> {
    let bounded_limit = limit.clamp(1, 100);

    sqlx::query_as::<_, BackupContentItem>(
        r#"
        SELECT
            id,
            kind,
            title,
            body,
            source_url,
            media_ref,
            active,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        FROM backup_content_items
        WHERE $2::BOOLEAN IS NULL OR active = $2
        ORDER BY updated_at DESC
        LIMIT $1
        "#,
    )
    .bind(bounded_limit)
    .bind(active)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list backup content items")
}

pub async fn update_backup_content_item(
    pool: &PgPool,
    item_id: i64,
    update: &UpdateBackupContentItem,
) -> Result<Option<BackupContentItem>> {
    let existing = find_backup_content_item(pool, item_id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    let normalized = ValidatedBackupContentInput::from_update(&existing, update)?;
    let updated_by_sub = trimmed_optional(update.updated_by_sub.as_deref());

    sqlx::query_as::<_, BackupContentItem>(
        r#"
        UPDATE backup_content_items
        SET
            kind = $2,
            title = $3,
            body = $4,
            source_url = $5,
            media_ref = $6,
            active = $7,
            updated_by_sub = COALESCE($8, updated_by_sub)
        WHERE id = $1
        RETURNING
            id,
            kind,
            title,
            body,
            source_url,
            media_ref,
            active,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(item_id)
    .bind(normalized.kind)
    .bind(&normalized.title)
    .bind(&normalized.body)
    .bind(&normalized.source_url)
    .bind(&normalized.media_ref)
    .bind(normalized.active)
    .bind(&updated_by_sub)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to update backup content item `{item_id}`"))
}

pub async fn delete_backup_content_item(
    pool: &PgPool,
    item_id: i64,
) -> Result<Option<BackupContentItem>> {
    sqlx::query_as::<_, BackupContentItem>(
        r#"
        DELETE FROM backup_content_items
        WHERE id = $1
        RETURNING
            id,
            kind,
            title,
            body,
            source_url,
            media_ref,
            active,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(item_id)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to delete backup content item `{item_id}`"))
}

const BACKUP_CONTENT_SELECT_BY_ID: &str = r#"
    SELECT
        id,
        kind,
        title,
        body,
        source_url,
        media_ref,
        active,
        created_by_sub,
        updated_by_sub,
        created_at,
        updated_at
    FROM backup_content_items
    WHERE id = $1
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedBackupContentInput {
    kind: BackupContentKind,
    title: String,
    body: String,
    source_url: Option<String>,
    media_ref: Option<String>,
    active: bool,
}

impl ValidatedBackupContentInput {
    fn from_new(item: &NewBackupContentItem) -> Result<Self> {
        validate_backup_content_input(
            item.kind,
            &item.title,
            &item.body,
            trimmed_optional(item.source_url.as_deref()),
            trimmed_optional(item.media_ref.as_deref()),
            item.active.unwrap_or(true),
        )
    }

    fn from_update(existing: &BackupContentItem, update: &UpdateBackupContentItem) -> Result<Self> {
        validate_backup_content_input(
            update.kind.unwrap_or(existing.kind),
            update.title.as_deref().unwrap_or(existing.title.as_str()),
            update.body.as_deref().unwrap_or(existing.body.as_str()),
            match &update.source_url {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.source_url.clone(),
            },
            match &update.media_ref {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.media_ref.clone(),
            },
            update.active.unwrap_or(existing.active),
        )
    }
}

fn validate_backup_content_input(
    kind: BackupContentKind,
    title: &str,
    body: &str,
    source_url: Option<String>,
    media_ref: Option<String>,
    active: bool,
) -> Result<ValidatedBackupContentInput> {
    let title = title.trim();
    let body = body.trim();

    if title.is_empty() {
        anyhow::bail!("backup content title is required");
    }

    if body.is_empty() {
        anyhow::bail!("backup content body is required");
    }

    if let Some(source_url) = &source_url {
        let parsed = Url::parse(source_url)
            .with_context(|| format!("backup content source url `{source_url}` is invalid"))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            anyhow::bail!("backup content source url must use http or https");
        }
    }

    Ok(ValidatedBackupContentInput {
        kind,
        title: title.to_owned(),
        body: body.to_owned(),
        source_url,
        media_ref,
        active,
    })
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
        validate_backup_content_input, BackupContentItem, BackupContentKind,
        UpdateBackupContentItem, ValidatedBackupContentInput,
    };
    use chrono::Utc;

    #[test]
    fn trimmed_optional_drops_blank_values() {
        assert_eq!(super::trimmed_optional(None), None);
        assert_eq!(super::trimmed_optional(Some("   ")), None);
        assert_eq!(
            super::trimmed_optional(Some(" media/item.png ")),
            Some("media/item.png".to_owned())
        );
    }

    #[test]
    fn validate_backup_content_requires_title_and_body() {
        assert!(validate_backup_content_input(
            BackupContentKind::DidYouKnow,
            "",
            "Body",
            None,
            None,
            true
        )
        .is_err());
        assert!(validate_backup_content_input(
            BackupContentKind::PastRecap,
            "Title",
            " ",
            None,
            None,
            true
        )
        .is_err());
    }

    #[test]
    fn validate_backup_content_rejects_non_http_source_url() {
        let error = match validate_backup_content_input(
            BackupContentKind::DidYouKnow,
            "Fact",
            "Body",
            Some("ftp://example.com/fact".to_owned()),
            None,
            true,
        ) {
            Ok(_) => panic!("non-http source url should be rejected"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("source url"));
    }

    #[test]
    fn update_validation_preserves_existing_values() {
        let existing = BackupContentItem {
            id: 10,
            kind: BackupContentKind::PastRecap,
            title: "Summer recap".to_owned(),
            body: "Last summer's night market highlights".to_owned(),
            source_url: Some("https://example.com/recap".to_owned()),
            media_ref: Some("backup/recap.jpg".to_owned()),
            active: true,
            created_by_sub: Some("user-1".to_owned()),
            updated_by_sub: Some("user-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let update = UpdateBackupContentItem {
            kind: Some(BackupContentKind::DidYouKnow),
            title: Some(" Local fact ".to_owned()),
            body: None,
            source_url: Some(Some(" ".to_owned())),
            media_ref: None,
            active: Some(false),
            updated_by_sub: Some("user-2".to_owned()),
        };

        let validated = match ValidatedBackupContentInput::from_update(&existing, &update) {
            Ok(validated) => validated,
            Err(error) => panic!("valid update should pass: {error}"),
        };

        assert_eq!(validated.kind, BackupContentKind::DidYouKnow);
        assert_eq!(validated.title, "Local fact");
        assert_eq!(validated.body, "Last summer's night market highlights");
        assert_eq!(validated.source_url, None);
        assert_eq!(validated.media_ref, Some("backup/recap.jpg".to_owned()));
        assert!(!validated.active);
    }
}
