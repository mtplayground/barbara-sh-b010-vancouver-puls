use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "post_draft_status", rename_all = "snake_case")]
pub enum DraftStatus {
    Draft,
    InReview,
    Approved,
    Rejected,
    Scheduled,
    Published,
    Archived,
}

impl DraftStatus {
    pub fn is_editable(self) -> bool {
        matches!(self, Self::Draft | Self::InReview | Self::Approved)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (current, next) if current == next => true,
            (
                Self::Draft | Self::InReview,
                Self::Draft | Self::InReview | Self::Approved | Self::Rejected,
            ) => true,
            (
                Self::Approved,
                Self::Draft | Self::InReview | Self::Approved | Self::Rejected | Self::Scheduled,
            ) => true,
            (Self::Scheduled, Self::Published | Self::Archived) => true,
            (Self::Published, Self::Archived) => true,
            _ => false,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Archived)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct PostDraft {
    pub id: i64,
    pub source_item_id: Option<i64>,
    pub caption_en: String,
    pub caption_zh: String,
    pub status: DraftStatus,
    pub rendered_post_asset_ref: Option<String>,
    pub rendered_reel_asset_ref: Option<String>,
    pub created_by_sub: Option<String>,
    pub updated_by_sub: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPostDraft {
    pub source_item_id: Option<i64>,
    pub caption_en: String,
    pub caption_zh: String,
    pub status: Option<DraftStatus>,
    pub rendered_post_asset_ref: Option<String>,
    pub rendered_reel_asset_ref: Option<String>,
    pub created_by_sub: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePostDraft {
    pub source_item_id: Option<Option<i64>>,
    pub caption_en: Option<String>,
    pub caption_zh: Option<String>,
    pub status: Option<DraftStatus>,
    pub rendered_post_asset_ref: Option<Option<String>>,
    pub rendered_reel_asset_ref: Option<Option<String>>,
    pub updated_by_sub: Option<String>,
}

pub async fn create_post_draft(pool: &PgPool, draft: &NewPostDraft) -> Result<PostDraft> {
    let normalized = ValidatedPostDraftInput::from_new(draft)?;
    let creator_sub = trimmed_optional(draft.created_by_sub.as_deref());

    sqlx::query_as::<_, PostDraft>(
        r#"
        INSERT INTO post_drafts (
            source_item_id,
            caption_en,
            caption_zh,
            status,
            rendered_post_asset_ref,
            rendered_reel_asset_ref,
            created_by_sub,
            updated_by_sub
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
        RETURNING
            id,
            source_item_id,
            caption_en,
            caption_zh,
            status,
            rendered_post_asset_ref,
            rendered_reel_asset_ref,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(normalized.source_item_id)
    .bind(&normalized.caption_en)
    .bind(&normalized.caption_zh)
    .bind(normalized.status)
    .bind(&normalized.rendered_post_asset_ref)
    .bind(&normalized.rendered_reel_asset_ref)
    .bind(&creator_sub)
    .persistent(false)
    .fetch_one(pool)
    .await
    .context("failed to create post draft")
}

pub async fn find_post_draft(pool: &PgPool, draft_id: i64) -> Result<Option<PostDraft>> {
    sqlx::query_as::<_, PostDraft>(POST_DRAFT_SELECT_BY_ID)
        .bind(draft_id)
        .persistent(false)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("failed to find post draft `{draft_id}`"))
}

pub async fn list_post_drafts(pool: &PgPool, limit: i64) -> Result<Vec<PostDraft>> {
    let bounded_limit = limit.clamp(1, 100);

    sqlx::query_as::<_, PostDraft>(
        r#"
        SELECT
            id,
            source_item_id,
            caption_en,
            caption_zh,
            status,
            rendered_post_asset_ref,
            rendered_reel_asset_ref,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        FROM post_drafts
        ORDER BY updated_at DESC
        LIMIT $1
        "#,
    )
    .bind(bounded_limit)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list post drafts")
}

pub async fn update_post_draft(
    pool: &PgPool,
    draft_id: i64,
    update: &UpdatePostDraft,
) -> Result<Option<PostDraft>> {
    let existing = find_post_draft(pool, draft_id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    let normalized = ValidatedPostDraftInput::from_update(&existing, update)?;
    let updated_by_sub = trimmed_optional(update.updated_by_sub.as_deref());

    sqlx::query_as::<_, PostDraft>(
        r#"
        UPDATE post_drafts
        SET
            source_item_id = $2,
            caption_en = $3,
            caption_zh = $4,
            status = $5,
            rendered_post_asset_ref = $6,
            rendered_reel_asset_ref = $7,
            updated_by_sub = COALESCE($8, updated_by_sub)
        WHERE id = $1
        RETURNING
            id,
            source_item_id,
            caption_en,
            caption_zh,
            status,
            rendered_post_asset_ref,
            rendered_reel_asset_ref,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(draft_id)
    .bind(normalized.source_item_id)
    .bind(&normalized.caption_en)
    .bind(&normalized.caption_zh)
    .bind(normalized.status)
    .bind(&normalized.rendered_post_asset_ref)
    .bind(&normalized.rendered_reel_asset_ref)
    .bind(&updated_by_sub)
    .persistent(false)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to update post draft `{draft_id}`"))
}

const POST_DRAFT_SELECT_BY_ID: &str = r#"
    SELECT
        id,
        source_item_id,
        caption_en,
        caption_zh,
        status,
        rendered_post_asset_ref,
        rendered_reel_asset_ref,
        created_by_sub,
        updated_by_sub,
        created_at,
        updated_at
    FROM post_drafts
    WHERE id = $1
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedPostDraftInput {
    source_item_id: Option<i64>,
    caption_en: String,
    caption_zh: String,
    status: DraftStatus,
    rendered_post_asset_ref: Option<String>,
    rendered_reel_asset_ref: Option<String>,
}

impl ValidatedPostDraftInput {
    fn from_new(draft: &NewPostDraft) -> Result<Self> {
        validate_draft_input(
            draft.source_item_id,
            &draft.caption_en,
            &draft.caption_zh,
            draft.status.unwrap_or(DraftStatus::Draft),
            trimmed_optional(draft.rendered_post_asset_ref.as_deref()),
            trimmed_optional(draft.rendered_reel_asset_ref.as_deref()),
        )
    }

    fn from_update(existing: &PostDraft, update: &UpdatePostDraft) -> Result<Self> {
        validate_draft_input(
            update.source_item_id.unwrap_or(existing.source_item_id),
            update
                .caption_en
                .as_deref()
                .unwrap_or(existing.caption_en.as_str()),
            update
                .caption_zh
                .as_deref()
                .unwrap_or(existing.caption_zh.as_str()),
            validate_status_transition(existing, update.status.unwrap_or(existing.status))?,
            match &update.rendered_post_asset_ref {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.rendered_post_asset_ref.clone(),
            },
            match &update.rendered_reel_asset_ref {
                Some(value) => trimmed_optional(value.as_deref()),
                None => existing.rendered_reel_asset_ref.clone(),
            },
        )
    }
}

fn validate_draft_input(
    source_item_id: Option<i64>,
    caption_en: &str,
    caption_zh: &str,
    status: DraftStatus,
    rendered_post_asset_ref: Option<String>,
    rendered_reel_asset_ref: Option<String>,
) -> Result<ValidatedPostDraftInput> {
    if source_item_id.is_some_and(|id| id < 1) {
        anyhow::bail!("source item id must be positive");
    }

    let caption_en = caption_en.trim();
    let caption_zh = caption_zh.trim();

    if caption_en.is_empty() {
        anyhow::bail!("English caption is required");
    }

    if caption_zh.is_empty() {
        anyhow::bail!("Chinese caption is required");
    }

    Ok(ValidatedPostDraftInput {
        source_item_id,
        caption_en: caption_en.to_owned(),
        caption_zh: caption_zh.to_owned(),
        status,
        rendered_post_asset_ref,
        rendered_reel_asset_ref,
    })
}

fn validate_status_transition(
    existing: &PostDraft,
    next_status: DraftStatus,
) -> Result<DraftStatus> {
    if !existing.status.can_transition_to(next_status) {
        anyhow::bail!(
            "draft status cannot transition from {:?} to {:?}",
            existing.status,
            next_status
        );
    }

    if next_status == DraftStatus::Approved
        && (existing.rendered_post_asset_ref.is_none()
            || existing.rendered_reel_asset_ref.is_none())
    {
        anyhow::bail!("draft must have rendered post and reel assets before approval");
    }

    if next_status == DraftStatus::Scheduled && existing.status != DraftStatus::Approved {
        anyhow::bail!("only approved drafts are schedulable");
    }

    Ok(next_status)
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{DraftStatus, PostDraft, UpdatePostDraft, ValidatedPostDraftInput};
    use chrono::Utc;

    #[test]
    fn draft_status_editability_matches_workflow() {
        assert!(DraftStatus::Draft.is_editable());
        assert!(DraftStatus::InReview.is_editable());
        assert!(DraftStatus::Approved.is_editable());
        assert!(!DraftStatus::Rejected.is_editable());
        assert!(!DraftStatus::Scheduled.is_editable());
        assert!(!DraftStatus::Published.is_editable());
        assert!(!DraftStatus::Archived.is_editable());
    }

    #[test]
    fn status_transitions_match_approval_workflow() {
        assert!(DraftStatus::Draft.can_transition_to(DraftStatus::Approved));
        assert!(DraftStatus::InReview.can_transition_to(DraftStatus::Rejected));
        assert!(DraftStatus::Approved.can_transition_to(DraftStatus::Scheduled));
        assert!(!DraftStatus::Draft.can_transition_to(DraftStatus::Scheduled));
        assert!(!DraftStatus::Rejected.can_transition_to(DraftStatus::Approved));
        assert!(DraftStatus::Rejected.is_terminal());
        assert!(DraftStatus::Archived.is_terminal());
    }

    #[test]
    fn update_validation_preserves_existing_fields_and_trims_assets() {
        let existing = PostDraft {
            id: 10,
            source_item_id: Some(4),
            caption_en: "English".to_owned(),
            caption_zh: "中文".to_owned(),
            status: DraftStatus::Draft,
            rendered_post_asset_ref: Some("rendered/post.png".to_owned()),
            rendered_reel_asset_ref: Some("rendered/reel.png".to_owned()),
            created_by_sub: Some("user-1".to_owned()),
            updated_by_sub: Some("user-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let update = UpdatePostDraft {
            source_item_id: None,
            caption_en: Some(" Updated English ".to_owned()),
            caption_zh: None,
            status: Some(DraftStatus::InReview),
            rendered_post_asset_ref: Some(Some(" rendered/new-post.png ".to_owned())),
            rendered_reel_asset_ref: Some(Some(" ".to_owned())),
            updated_by_sub: Some("user-2".to_owned()),
        };

        let validated = match ValidatedPostDraftInput::from_update(&existing, &update) {
            Ok(validated) => validated,
            Err(error) => panic!("valid update should pass: {error}"),
        };

        assert_eq!(validated.source_item_id, Some(4));
        assert_eq!(validated.caption_en, "Updated English");
        assert_eq!(validated.caption_zh, "中文");
        assert_eq!(validated.status, DraftStatus::InReview);
        assert_eq!(
            validated.rendered_post_asset_ref,
            Some("rendered/new-post.png".to_owned())
        );
        assert_eq!(validated.rendered_reel_asset_ref, None);
    }

    #[test]
    fn source_item_id_must_be_positive() {
        let error =
            match super::validate_draft_input(Some(0), "", "", DraftStatus::Draft, None, None) {
                Ok(_) => panic!("zero source item id should be rejected"),
                Err(error) => error,
            };

        assert!(error.to_string().contains("source item id"));
    }

    #[test]
    fn captions_are_required() {
        let error =
            match super::validate_draft_input(None, " ", "中文", DraftStatus::Draft, None, None) {
                Ok(_) => panic!("empty English caption should be rejected"),
                Err(error) => error,
            };

        assert!(error.to_string().contains("English caption"));
    }

    #[test]
    fn approval_requires_rendered_assets() {
        let existing = PostDraft {
            id: 10,
            source_item_id: None,
            caption_en: "English".to_owned(),
            caption_zh: "中文".to_owned(),
            status: DraftStatus::Draft,
            rendered_post_asset_ref: Some("rendered/post.png".to_owned()),
            rendered_reel_asset_ref: None,
            created_by_sub: Some("user-1".to_owned()),
            updated_by_sub: Some("user-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let update = UpdatePostDraft {
            source_item_id: None,
            caption_en: None,
            caption_zh: None,
            status: Some(DraftStatus::Approved),
            rendered_post_asset_ref: None,
            rendered_reel_asset_ref: None,
            updated_by_sub: Some("user-2".to_owned()),
        };

        let error = match ValidatedPostDraftInput::from_update(&existing, &update) {
            Ok(_) => panic!("approval without both rendered assets should be rejected"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("rendered post and reel assets"));
    }

    #[test]
    fn only_approved_drafts_can_be_scheduled() {
        let existing = PostDraft {
            id: 10,
            source_item_id: None,
            caption_en: "English".to_owned(),
            caption_zh: "中文".to_owned(),
            status: DraftStatus::Draft,
            rendered_post_asset_ref: Some("rendered/post.png".to_owned()),
            rendered_reel_asset_ref: Some("rendered/reel.png".to_owned()),
            created_by_sub: Some("user-1".to_owned()),
            updated_by_sub: Some("user-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let update = UpdatePostDraft {
            source_item_id: None,
            caption_en: None,
            caption_zh: None,
            status: Some(DraftStatus::Scheduled),
            rendered_post_asset_ref: None,
            rendered_reel_asset_ref: None,
            updated_by_sub: Some("user-2".to_owned()),
        };

        let error = match ValidatedPostDraftInput::from_update(&existing, &update) {
            Ok(_) => panic!("draft should not schedule before approval"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("Draft to Scheduled"));
    }
}
