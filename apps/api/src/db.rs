use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::AppConfig;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(config: &AppConfig) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(config.database_max_connections())
        .acquire_timeout(Duration::from_secs(10))
        .connect(config.database_url())
        .await
        .context("failed to connect to PostgreSQL")
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("failed to run database migrations")
}

pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .persistent(false)
        .fetch_one(pool)
        .await
        .context("database health check failed")?;

    Ok(())
}
