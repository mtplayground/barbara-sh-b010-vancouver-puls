use anyhow::{Context, Result};
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::drafts::{DraftStatus, PostDraft};

pub const MAX_CALENDAR_DAYS: i64 = 60;
pub const DEFAULT_CALENDAR_DAYS: i64 = 14;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct ScheduleSlot {
    pub id: i64,
    pub slot_date: NaiveDate,
    pub slot_time: NaiveTime,
    pub draft_id: Option<i64>,
    pub created_by_sub: Option<String>,
    pub updated_by_sub: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScheduleAssignment {
    pub slot_date: NaiveDate,
    pub slot_time: Option<NaiveTime>,
    pub draft_id: i64,
    pub user_sub: Option<String>,
}

pub fn default_slot_time() -> NaiveTime {
    NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN)
}

pub fn calendar_end_date(start_date: NaiveDate, days: i64) -> Result<NaiveDate> {
    let bounded_days = days.clamp(1, MAX_CALENDAR_DAYS);
    start_date
        .checked_add_days(Days::new((bounded_days - 1) as u64))
        .context("calendar date range is invalid")
}

pub fn calendar_dates(start_date: NaiveDate, end_date: NaiveDate) -> Result<Vec<NaiveDate>> {
    if end_date < start_date {
        anyhow::bail!("calendar end date cannot be before start date");
    }

    let mut dates = Vec::new();
    let mut current = start_date;

    loop {
        dates.push(current);

        if current == end_date {
            break;
        }

        current = current
            .checked_add_days(Days::new(1))
            .context("calendar date range is invalid")?;
    }

    Ok(dates)
}

pub async fn list_schedule_slots(
    pool: &PgPool,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<ScheduleSlot>> {
    if end_date < start_date {
        anyhow::bail!("calendar end date cannot be before start date");
    }

    sqlx::query_as::<_, ScheduleSlot>(
        r#"
        SELECT
            id,
            slot_date,
            slot_time,
            draft_id,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        FROM schedule_slots
        WHERE slot_date >= $1 AND slot_date <= $2
        ORDER BY slot_date ASC
        "#,
    )
    .bind(start_date)
    .bind(end_date)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list schedule slots")
}

pub async fn assign_approved_draft_to_slot(
    pool: &PgPool,
    assignment: &NewScheduleAssignment,
) -> Result<ScheduleSlot> {
    validate_assignment_input(assignment)?;

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin schedule assignment transaction")?;

    let draft = sqlx::query_as::<_, PostDraft>(
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
        WHERE id = $1
        "#,
    )
    .bind(assignment.draft_id)
    .persistent(false)
    .fetch_optional(&mut *tx)
    .await
    .with_context(|| format!("failed to find post draft `{}`", assignment.draft_id))?
    .context("draft was not found")?;

    if draft.status != DraftStatus::Approved {
        anyhow::bail!("only approved drafts can be assigned to the calendar");
    }

    let existing_assignment = sqlx::query_as::<_, ScheduleSlot>(
        r#"
        SELECT
            id,
            slot_date,
            slot_time,
            draft_id,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        FROM schedule_slots
        WHERE draft_id = $1
        "#,
    )
    .bind(assignment.draft_id)
    .persistent(false)
    .fetch_optional(&mut *tx)
    .await
    .with_context(|| {
        format!(
            "failed to find schedule assignment for draft `{}`",
            assignment.draft_id
        )
    })?;

    if let Some(existing_assignment) = existing_assignment {
        anyhow::bail!(
            "draft is already assigned to {}",
            existing_assignment.slot_date
        );
    }

    let existing_slot = sqlx::query_as::<_, ScheduleSlot>(
        r#"
        SELECT
            id,
            slot_date,
            slot_time,
            draft_id,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        FROM schedule_slots
        WHERE slot_date = $1
        "#,
    )
    .bind(assignment.slot_date)
    .persistent(false)
    .fetch_optional(&mut *tx)
    .await
    .with_context(|| {
        format!(
            "failed to find schedule slot for date `{}`",
            assignment.slot_date
        )
    })?;

    if let Some(existing_slot) = &existing_slot {
        if existing_slot.draft_id.is_some() {
            anyhow::bail!("schedule slot already has an assigned draft");
        }
    }

    let slot_time = assignment.slot_time.unwrap_or_else(default_slot_time);
    let user_sub = trimmed_optional(assignment.user_sub.as_deref());

    let slot = sqlx::query_as::<_, ScheduleSlot>(
        r#"
        INSERT INTO schedule_slots (
            slot_date,
            slot_time,
            draft_id,
            created_by_sub,
            updated_by_sub
        )
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (slot_date) DO UPDATE
        SET
            slot_time = EXCLUDED.slot_time,
            draft_id = EXCLUDED.draft_id,
            updated_by_sub = EXCLUDED.updated_by_sub
        RETURNING
            id,
            slot_date,
            slot_time,
            draft_id,
            created_by_sub,
            updated_by_sub,
            created_at,
            updated_at
        "#,
    )
    .bind(assignment.slot_date)
    .bind(slot_time)
    .bind(assignment.draft_id)
    .bind(&user_sub)
    .persistent(false)
    .fetch_one(&mut *tx)
    .await
    .context("failed to assign draft to schedule slot")?;

    sqlx::query(
        r#"
        UPDATE post_drafts
        SET status = 'scheduled', updated_by_sub = COALESCE($2, updated_by_sub)
        WHERE id = $1
        "#,
    )
    .bind(assignment.draft_id)
    .bind(&user_sub)
    .persistent(false)
    .execute(&mut *tx)
    .await
    .with_context(|| {
        format!(
            "failed to mark post draft `{}` as scheduled",
            assignment.draft_id
        )
    })?;

    tx.commit()
        .await
        .context("failed to commit schedule assignment")?;

    Ok(slot)
}

fn validate_assignment_input(assignment: &NewScheduleAssignment) -> Result<()> {
    if assignment.draft_id < 1 {
        anyhow::bail!("draft id must be positive");
    }

    let today = Utc::now().date_naive();
    if assignment.slot_date < today {
        anyhow::bail!("schedule slot date cannot be in the past");
    }

    Ok(())
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{calendar_dates, calendar_end_date, default_slot_time, MAX_CALENDAR_DAYS};
    use chrono::{Days, NaiveDate, NaiveTime};

    #[test]
    fn calendar_end_date_bounds_days_to_daily_cadence_window() {
        let start = valid_date(2026, 7, 20);
        let end = match calendar_end_date(start, 999) {
            Ok(end) => end,
            Err(error) => panic!("bounded range should be valid: {error}"),
        };
        let expected = match start.checked_add_days(Days::new((MAX_CALENDAR_DAYS - 1) as u64)) {
            Some(expected) => expected,
            None => panic!("bounded end should be valid"),
        };

        assert_eq!(end, expected);
    }

    #[test]
    fn calendar_dates_includes_each_day_in_range() {
        let start = valid_date(2026, 7, 20);
        let end = valid_date(2026, 7, 22);

        let dates = match calendar_dates(start, end) {
            Ok(dates) => dates,
            Err(error) => panic!("valid date range should pass: {error}"),
        };

        let middle = match start.checked_add_days(Days::new(1)) {
            Some(middle) => middle,
            None => panic!("middle date should be valid"),
        };

        assert_eq!(dates, vec![start, middle, end]);
    }

    #[test]
    fn default_slot_time_is_morning_cadence() {
        assert_eq!(default_slot_time(), valid_time(9, 0, 0));
    }

    fn valid_date(year: i32, month: u32, day: u32) -> NaiveDate {
        match NaiveDate::from_ymd_opt(year, month, day) {
            Some(date) => date,
            None => panic!("test date should be valid"),
        }
    }

    fn valid_time(hour: u32, minute: u32, second: u32) -> NaiveTime {
        match NaiveTime::from_hms_opt(hour, minute, second) {
            Some(time) => time,
            None => panic!("test time should be valid"),
        }
    }
}
