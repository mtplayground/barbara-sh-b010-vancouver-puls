mod config;

use anyhow::{Context, Result};
use axum::{routing::get, Json, Router};
use config::AppConfig;
use serde::Serialize;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let bind_addr = config.bind_addr()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on {bind_addr}"))?;

    info!("api listening on {bind_addr}");

    axum::serve(listener, app())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("api server failed")?;

    Ok(())
}

fn app() -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "api",
    })
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
