use anyhow::{Context, Result};
use api::{config::AppConfig, db, storage::ObjectStorage};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
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
    storage: Option<ObjectStorage>,
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
struct StorageHealthResponse {
    status: &'static str,
    storage: &'static str,
    bucket: Option<String>,
    prefix: Option<String>,
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
    let storage = match &config.object_storage {
        Some(object_storage_config) => {
            Some(ObjectStorage::from_config(object_storage_config).await?)
        }
        None => None,
    };
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

    axum::serve(listener, app(pool, storage))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("api server failed")?;

    Ok(())
}

fn app(pool: PgPool, storage: Option<ObjectStorage>) -> Router {
    let state = AppState { db: pool, storage };

    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .route("/api/health/db", get(database_health))
        .route("/api/health/storage", get(storage_health))
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

async fn storage_health(
    State(state): State<AppState>,
) -> Result<Json<StorageHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    let Some(storage) = state.storage else {
        return Ok(Json(StorageHealthResponse {
            status: "disabled",
            storage: "s3",
            bucket: None,
            prefix: None,
        }));
    };

    storage.check_bucket().await.map_err(internal_error)?;

    Ok(Json(StorageHealthResponse {
        status: "ok",
        storage: "s3",
        bucket: Some(storage.bucket().to_owned()),
        prefix: Some(storage.prefix().to_owned()),
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
