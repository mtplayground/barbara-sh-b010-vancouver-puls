use anyhow::{Context, Result};
use api::{
    auth::{self, AuthService},
    config::AppConfig,
    cors::cors_layer,
    db,
    error::ApiError,
    storage::ObjectStorage,
    users::{User, UserRole},
};
use axum::{
    extract::State,
    http::{header::COOKIE, HeaderMap},
    response::Redirect,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use std::env;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
struct AppState {
    db: PgPool,
    storage: Option<ObjectStorage>,
    auth: Option<AuthService>,
    self_url: Option<String>,
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
struct AuthSessionResponse {
    authenticated: bool,
    user: Option<AuthSessionUser>,
}

#[derive(Debug, Serialize)]
struct AuthSessionUser {
    sub: String,
    email: String,
    name: Option<String>,
    picture_url: Option<String>,
    role: UserRole,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    config.log_summary();
    let auth = config.auth.clone().map(AuthService::new);
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

    axum::serve(listener, app(pool, storage, auth, config))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("api server failed")?;

    Ok(())
}

fn app(
    pool: PgPool,
    storage: Option<ObjectStorage>,
    auth: Option<AuthService>,
    config: AppConfig,
) -> Router {
    let state = AppState {
        db: pool,
        storage,
        auth,
        self_url: config.server.self_url.clone(),
    };

    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .route("/api/health/db", get(database_health))
        .route("/api/health/storage", get(storage_health))
        .route("/api/auth/login", get(auth_login))
        .route("/api/auth/session", get(auth_session))
        .fallback(api_not_found)
        .layer(cors_layer(&config.server))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_response(DefaultOnResponse::new().include_headers(true)),
        )
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
) -> Result<Json<DatabaseHealthResponse>, ApiError> {
    db::ping(&state.db).await.map_err(ApiError::internal)?;

    Ok(Json(DatabaseHealthResponse {
        status: "ok",
        database: "postgres",
    }))
}

async fn storage_health(
    State(state): State<AppState>,
) -> Result<Json<StorageHealthResponse>, ApiError> {
    let Some(storage) = state.storage else {
        return Ok(Json(StorageHealthResponse {
            status: "disabled",
            storage: "s3",
            bucket: None,
            prefix: None,
        }));
    };

    storage.check_bucket().await.map_err(ApiError::internal)?;

    Ok(Json(StorageHealthResponse {
        status: "ok",
        storage: "s3",
        bucket: Some(storage.bucket().to_owned()),
        prefix: Some(storage.prefix().to_owned()),
    }))
}

async fn auth_login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Redirect, ApiError> {
    let Some(auth) = &state.auth else {
        return Err(ApiError::service_unavailable(
            "auth service is not configured",
        ));
    };

    let return_to = auth::frontend_root_return_to(&headers, state.self_url.as_deref());
    let login_url = auth.login_url(&return_to).map_err(ApiError::internal)?;

    Ok(Redirect::to(&login_url))
}

async fn auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthSessionResponse>, ApiError> {
    let Some(auth) = &state.auth else {
        return Ok(Json(AuthSessionResponse::anonymous()));
    };

    let authenticated_user = auth
        .authenticate_cookie_header(&state.db, headers.get(COOKIE))
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(match authenticated_user {
        Some(authenticated_user) => AuthSessionResponse {
            authenticated: true,
            user: Some(AuthSessionUser::from(authenticated_user.user)),
        },
        None => AuthSessionResponse::anonymous(),
    }))
}

impl AuthSessionResponse {
    fn anonymous() -> Self {
        Self {
            authenticated: false,
            user: None,
        }
    }
}

impl From<User> for AuthSessionUser {
    fn from(user: User) -> Self {
        Self {
            sub: user.sub,
            email: user.email,
            name: user.name,
            picture_url: user.picture_url,
            role: user.role,
        }
    }
}

async fn api_not_found() -> ApiError {
    ApiError::not_found()
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
