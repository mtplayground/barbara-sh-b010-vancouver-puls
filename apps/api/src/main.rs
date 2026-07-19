mod config;
mod db;

use anyhow::{Context, Result};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use config::AppConfig;
use serde::Serialize;
use sqlx::PgPool;
use std::env;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
struct AppState {
    db: PgPool,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Serialize)]
struct DatabaseHealthResponse {
    status: &'static str,
    database: &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    config.log_summary();
    let pool = db::connect(&config).await?;
    db::migrate(&pool).await?;

    if env::args().nth(1).as_deref() == Some("migrate") {
        info!("database migrations completed");
        return Ok(());
    }

    let bind_addr = config.server.bind_addr()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on {bind_addr}"))?;

    info!("api listening on {bind_addr}");

    axum::serve(listener, app(pool))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("api server failed")?;

    Ok(())
}

fn app(pool: PgPool) -> Router {
    let state = AppState { db: pool };

    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .route("/api/health/db", get(database_health))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "api",
    })
}

async fn database_health(
    State(state): State<AppState>,
) -> Result<Json<DatabaseHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    db::ping(&state.db).await.map_err(internal_error)?;

    Ok(Json(DatabaseHealthResponse {
        status: "ok",
        database: "postgres",
    }))
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    tracing::error!(error = ?error, "request failed");

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            code: "internal_error",
            message: "request failed".to_owned(),
        }),
    )
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("api=info,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
}
