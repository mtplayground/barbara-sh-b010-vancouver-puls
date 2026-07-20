use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{FromRow, PgPool};
use tokio::{
    task::JoinHandle,
    time::{interval, Duration, MissedTickBehavior},
};
use tracing::{info, warn};

use crate::{
    drafts::{DraftStatus, PostDraft},
    email::{EmailSendError, EmailService},
    instagram::InstagramConnection,
    publishing::{
        self, InstagramPublishTarget, InstagramPublisher, NewPublishLog, PublishLogStatus,
    },
    storage::ObjectStorage,
};

const PUBLISHER_INTERVAL: Duration = Duration::from_secs(60);
const MAX_PUBLISH_ATTEMPTS: i64 = 3;
const DUE_BATCH_SIZE: i64 = 10;
const DISCONNECTED_ACCOUNT_ID: &str = "not_connected";
const SCHEDULER_REQUESTED_BY: &str = "scheduled-publisher";

#[derive(Debug, Clone)]
pub struct ScheduledPublisherJob {
    pool: PgPool,
    storage: Option<ObjectStorage>,
    email: Option<EmailService>,
    publisher: InstagramPublisher,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScheduledPublisherSummary {
    pub due_seen: usize,
    pub published: usize,
    pub failed: usize,
    pub alerts_attempted: usize,
}

#[derive(Debug, Clone)]
struct DueScheduledDraft {
    slot_id: i64,
    failure_count: i64,
    draft: PostDraft,
}

#[derive(Debug, Clone, FromRow)]
struct DueScheduledDraftRow {
    slot_id: i64,
    failure_count: i64,
    draft_id: i64,
    source_item_id: Option<i64>,
    caption_en: String,
    caption_zh: String,
    status: DraftStatus,
    rendered_post_asset_ref: Option<String>,
    rendered_reel_asset_ref: Option<String>,
    created_by_sub: Option<String>,
    updated_by_sub: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ScheduledPublisherJob {
    pub fn new(
        pool: PgPool,
        storage: Option<ObjectStorage>,
        email: Option<EmailService>,
        publisher: InstagramPublisher,
    ) -> Self {
        Self {
            pool,
            storage,
            email,
            publisher,
        }
    }

    pub async fn run_once(&self) -> Result<ScheduledPublisherSummary> {
        let due = list_due_scheduled_drafts(
            &self.pool,
            Utc::now().naive_utc(),
            MAX_PUBLISH_ATTEMPTS,
            DUE_BATCH_SIZE,
        )
        .await?;
        let mut summary = ScheduledPublisherSummary {
            due_seen: due.len(),
            ..ScheduledPublisherSummary::default()
        };

        if due.is_empty() {
            return Ok(summary);
        }

        let connection = active_instagram_connection(&self.pool).await?;

        if connection.is_none() {
            for due_draft in due {
                self.record_unavailable_failure(
                    &due_draft,
                    DISCONNECTED_ACCOUNT_ID,
                    "Instagram account is disconnected",
                    true,
                    &mut summary,
                )
                .await?;
            }

            return Ok(summary);
        }

        let Some(storage) = &self.storage else {
            for due_draft in due {
                self.record_unavailable_failure(
                    &due_draft,
                    connection
                        .as_ref()
                        .map(|connection| connection.instagram_account_id.as_str())
                        .unwrap_or(DISCONNECTED_ACCOUNT_ID),
                    "object storage is not configured",
                    false,
                    &mut summary,
                )
                .await?;
            }

            return Ok(summary);
        };

        let Some(connection) = connection else {
            return Ok(summary);
        };

        for due_draft in due {
            match publishing::publish_draft_to_instagram(
                &self.pool,
                storage,
                &self.publisher,
                &due_draft.draft,
                &connection,
                InstagramPublishTarget::Post,
                Some(SCHEDULER_REQUESTED_BY),
            )
            .await
            {
                Ok(_) => {
                    summary.published += 1;
                }
                Err(error) => {
                    summary.failed += 1;
                    let attempts_after_failure = due_draft.failure_count + 1;

                    if should_alert_after_failure(
                        due_draft.failure_count,
                        attempts_after_failure,
                        MAX_PUBLISH_ATTEMPTS,
                    ) {
                        summary.alerts_attempted += 1;
                        self.send_alert(
                            "Scheduled Instagram publish failed repeatedly",
                            &format!(
                                "Draft {} failed to publish after {} attempts.\nLast error: {}",
                                due_draft.draft.id, attempts_after_failure, error
                            ),
                        )
                        .await;
                    }
                }
            }
        }

        Ok(summary)
    }

    async fn record_unavailable_failure(
        &self,
        due_draft: &DueScheduledDraft,
        instagram_account_id: &str,
        message: &str,
        alert_immediately: bool,
        summary: &mut ScheduledPublisherSummary,
    ) -> Result<()> {
        let asset_ref =
            publishing::publish_asset_ref(&due_draft.draft, InstagramPublishTarget::Post)
                .map(str::to_owned)?;

        publishing::create_publish_log(
            &self.pool,
            &NewPublishLog {
                draft_id: due_draft.draft.id,
                target: InstagramPublishTarget::Post,
                status: PublishLogStatus::Failure,
                instagram_account_id: instagram_account_id.to_owned(),
                asset_ref,
                graph_container_id: None,
                graph_media_id: None,
                error_message: Some(message.to_owned()),
                requested_by_sub: Some(SCHEDULER_REQUESTED_BY.to_owned()),
            },
        )
        .await?;
        summary.failed += 1;

        let attempts_after_failure = due_draft.failure_count + 1;
        if alert_immediately
            || should_alert_after_failure(
                due_draft.failure_count,
                attempts_after_failure,
                MAX_PUBLISH_ATTEMPTS,
            )
        {
            summary.alerts_attempted += 1;
            self.send_alert(
                if alert_immediately {
                    "Instagram account is disconnected"
                } else {
                    "Scheduled Instagram publish failed repeatedly"
                },
                &format!(
                    "Draft {} could not publish from scheduled slot {}.\nReason: {}\nAttempts: {}",
                    due_draft.draft.id, due_draft.slot_id, message, attempts_after_failure
                ),
            )
            .await;
        }

        Ok(())
    }

    async fn send_alert(&self, subject: &str, message: &str) {
        let Some(email) = &self.email else {
            warn!(
                subject,
                "publisher alert skipped because email service is not configured"
            );
            return;
        };
        let Some(to) = email.operator_alert_email() else {
            warn!(
                subject,
                "publisher alert skipped because OPERATOR_ALERT_EMAIL is not configured"
            );
            return;
        };

        match email.send_publisher_alert(to, subject, message).await {
            Ok(delivery) => {
                info!(
                    subject,
                    message_id = delivery.message_id,
                    "publisher alert sent"
                );
            }
            Err(EmailSendError::RateLimited) => {
                warn!(subject, "publisher alert email is rate limited");
            }
            Err(EmailSendError::Failed(error)) => {
                warn!(subject, error, "publisher alert email failed");
            }
        }
    }
}

pub fn spawn_scheduled_publisher_job(
    pool: PgPool,
    storage: Option<ObjectStorage>,
    email: Option<EmailService>,
    publisher: InstagramPublisher,
) -> Result<JoinHandle<()>> {
    let job = ScheduledPublisherJob::new(pool, storage, email, publisher);

    Ok(tokio::spawn(async move {
        let mut ticker = interval(PUBLISHER_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            match job.run_once().await {
                Ok(summary) => {
                    if summary.due_seen > 0 {
                        info!(
                            due_seen = summary.due_seen,
                            published = summary.published,
                            failed = summary.failed,
                            alerts_attempted = summary.alerts_attempted,
                            "scheduled publisher run completed"
                        );
                    }
                }
                Err(error) => {
                    warn!(error = ?error, "scheduled publisher run failed");
                }
            }
        }
    }))
}

async fn list_due_scheduled_drafts(
    pool: &PgPool,
    now: NaiveDateTime,
    max_publish_attempts: i64,
    limit: i64,
) -> Result<Vec<DueScheduledDraft>> {
    let bounded_limit = limit.clamp(1, 50);

    let rows = sqlx::query_as::<_, DueScheduledDraftRow>(
        r#"
        WITH failed_attempts AS (
            SELECT draft_id, COUNT(*)::BIGINT AS failure_count
            FROM publish_logs
            WHERE status = 'failure'
            GROUP BY draft_id
        )
        SELECT
            s.id AS slot_id,
            COALESCE(failed_attempts.failure_count, 0)::BIGINT AS failure_count,
            d.id AS draft_id,
            d.source_item_id,
            d.caption_en,
            d.caption_zh,
            d.status,
            d.rendered_post_asset_ref,
            d.rendered_reel_asset_ref,
            d.created_by_sub,
            d.updated_by_sub,
            d.created_at,
            d.updated_at
        FROM schedule_slots s
        JOIN post_drafts d ON d.id = s.draft_id
        LEFT JOIN failed_attempts ON failed_attempts.draft_id = d.id
        WHERE d.status = 'scheduled'
            AND (s.slot_date + s.slot_time) <= $1
            AND COALESCE(failed_attempts.failure_count, 0) < $2
            AND NOT EXISTS (
                SELECT 1
                FROM publish_logs success_logs
                WHERE success_logs.draft_id = d.id
                    AND success_logs.status = 'success'
            )
        ORDER BY s.slot_date ASC, s.slot_time ASC
        LIMIT $3
        "#,
    )
    .bind(now)
    .bind(max_publish_attempts)
    .bind(bounded_limit)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list due scheduled drafts")?;

    Ok(rows.into_iter().map(DueScheduledDraft::from).collect())
}

async fn active_instagram_connection(pool: &PgPool) -> Result<Option<InstagramConnection>> {
    Ok(crate::instagram::find_instagram_connection(pool)
        .await?
        .filter(|connection| connection.disconnected_at.is_none()))
}

fn should_alert_after_failure(
    previous_failures: i64,
    attempts_after_failure: i64,
    max_attempts: i64,
) -> bool {
    previous_failures < max_attempts && attempts_after_failure >= max_attempts
}

impl From<DueScheduledDraftRow> for DueScheduledDraft {
    fn from(row: DueScheduledDraftRow) -> Self {
        Self {
            slot_id: row.slot_id,
            failure_count: row.failure_count,
            draft: PostDraft {
                id: row.draft_id,
                source_item_id: row.source_item_id,
                caption_en: row.caption_en,
                caption_zh: row.caption_zh,
                status: row.status,
                rendered_post_asset_ref: row.rendered_post_asset_ref,
                rendered_reel_asset_ref: row.rendered_reel_asset_ref,
                created_by_sub: row.created_by_sub,
                updated_by_sub: row.updated_by_sub,
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_alert_after_failure;

    #[test]
    fn alerts_when_failure_reaches_retry_ceiling() {
        assert!(should_alert_after_failure(2, 3, 3));
        assert!(!should_alert_after_failure(1, 2, 3));
        assert!(!should_alert_after_failure(3, 4, 3));
    }
}
