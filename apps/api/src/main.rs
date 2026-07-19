use anyhow::{Context, Result};
use api::{
    auth::{self, AuthService, AuthenticatedUser},
    config::AppConfig,
    cors::cors_layer,
    db,
    email::{email_error_to_anyhow, EmailDelivery, EmailSendError, EmailService},
    error::ApiError,
    ingestion,
    invites::{self, AcceptInviteError, UserInvite},
    sources::{ContentSource, ContentSourceKind, NewContentSource, UpdateContentSource},
    storage::ObjectStorage,
    users::{User, UserRole},
};
use axum::{
    extract::{Path, Query, State},
    http::{header::COOKIE, HeaderMap},
    response::Redirect,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
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
    email: Option<EmailService>,
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

#[derive(Debug, Serialize)]
struct UsersResponse {
    users: Vec<UserResponse>,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    sub: String,
    email: String,
    name: Option<String>,
    picture_url: Option<String>,
    role: UserRole,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_seen_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct UpdateUserRoleRequest {
    role: UserRole,
}

#[derive(Debug, Serialize)]
struct SourcesResponse {
    sources: Vec<SourceResponse>,
}

#[derive(Debug, Serialize)]
struct SourceResponse {
    id: i64,
    name: String,
    kind: ContentSourceKind,
    url: Option<String>,
    external_id: Option<String>,
    created_by_sub: Option<String>,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateSourceRequest {
    name: String,
    kind: ContentSourceKind,
    url: Option<String>,
    external_id: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateSourceRequest {
    name: Option<String>,
    kind: Option<ContentSourceKind>,
    url: Option<Option<String>>,
    external_id: Option<Option<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateInviteRequest {
    email: String,
}

#[derive(Debug, Serialize)]
struct CreateInviteResponse {
    invite: InviteResponse,
    invite_url: String,
    email_delivery: InviteEmailDelivery,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum InviteEmailDelivery {
    Sent { message_id: String },
    RateLimited,
    Skipped,
}

#[derive(Debug, Deserialize)]
struct AcceptInviteRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct AcceptInviteQuery {
    token: String,
}

#[derive(Debug, Serialize)]
struct AcceptInviteResponse {
    invite: InviteResponse,
    user: AuthSessionUser,
}

#[derive(Debug, Serialize)]
struct InviteResponse {
    email: String,
    role: UserRole,
    invited_by_sub: String,
    accepted_by_sub: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    accepted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    config.log_summary();
    let auth = config.auth.clone().map(AuthService::new);
    let email = config.email.clone().map(EmailService::new);
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

    let _ingestion_job = ingestion::spawn_ingestion_job(pool.clone(), storage.clone())
        .context("failed to start scheduled ingestion job")?;

    let bind_addr = config.server.bind_addr()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on {bind_addr}"))?;

    info!("api listening on {bind_addr}");

    axum::serve(listener, app(pool, storage, auth, email, config))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("api server failed")?;

    Ok(())
}

fn app(
    pool: PgPool,
    storage: Option<ObjectStorage>,
    auth: Option<AuthService>,
    email: Option<EmailService>,
    config: AppConfig,
) -> Router {
    let state = AppState {
        db: pool,
        storage,
        auth,
        email,
        self_url: config.server.self_url.clone(),
    };

    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .route("/api/health/db", get(database_health))
        .route("/api/health/storage", get(storage_health))
        .route("/api/auth/login", get(auth_login))
        .route("/api/auth/session", get(auth_session))
        .route("/api/admin/invites", post(create_invite))
        .route("/api/admin/users", get(list_admin_users))
        .route("/api/admin/users/:sub/role", patch(update_admin_user_role))
        .route("/api/admin/sources", get(list_sources).post(create_source))
        .route(
            "/api/admin/sources/:source_id",
            patch(update_source).delete(delete_source),
        )
        .route(
            "/api/invites/accept",
            get(accept_invite_redirect).post(accept_invite),
        )
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

async fn list_admin_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UsersResponse>, ApiError> {
    require_admin(&state, &headers).await?;

    let users = api::users::list_users(&state.db)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(UserResponse::from)
        .collect();

    Ok(Json(UsersResponse { users }))
}

async fn update_admin_user_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sub): Path<String>,
    Json(payload): Json<UpdateUserRoleRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let admin = require_admin(&state, &headers).await?;
    let sub = sub.trim();

    if sub.is_empty() {
        return Err(ApiError::bad_request("user id is required"));
    }

    if admin.user.sub == sub && payload.role != UserRole::Admin {
        return Err(ApiError::bad_request(
            "admins cannot remove their own admin role",
        ));
    }

    let user = api::users::update_user_role(&state.db, sub, payload.role)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("user was not found"))?;

    Ok(Json(UserResponse::from(user)))
}

async fn list_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SourcesResponse>, ApiError> {
    require_admin(&state, &headers).await?;

    let sources = api::sources::list_content_sources(&state.db)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(SourceResponse::from)
        .collect();

    Ok(Json(SourcesResponse { sources }))
}

async fn create_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateSourceRequest>,
) -> Result<Json<SourceResponse>, ApiError> {
    let admin = require_admin(&state, &headers).await?;
    let source = NewContentSource {
        name: payload.name,
        kind: payload.kind,
        url: payload.url,
        external_id: payload.external_id,
        created_by_sub: Some(admin.user.sub),
    };

    let created = api::sources::create_content_source(&state.db, &source)
        .await
        .map_err(source_write_error)?;

    if payload.enabled == Some(false) {
        let disabled = api::sources::set_content_source_enabled(&state.db, created.id, false)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::internal(anyhow::anyhow!("created source was not found")))?;
        return Ok(Json(SourceResponse::from(disabled)));
    }

    Ok(Json(SourceResponse::from(created)))
}

async fn update_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(source_id): Path<i64>,
    Json(payload): Json<UpdateSourceRequest>,
) -> Result<Json<SourceResponse>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_positive_source_id(source_id)?;

    let update = UpdateContentSource {
        name: payload.name,
        kind: payload.kind,
        url: payload.url,
        external_id: payload.external_id,
        enabled: payload.enabled,
    };
    let source = api::sources::update_content_source(&state.db, source_id, &update)
        .await
        .map_err(source_write_error)?
        .ok_or_else(|| ApiError::not_found_message("source was not found"))?;

    Ok(Json(SourceResponse::from(source)))
}

async fn delete_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(source_id): Path<i64>,
) -> Result<Json<SourceResponse>, ApiError> {
    require_admin(&state, &headers).await?;
    ensure_positive_source_id(source_id)?;

    let source = api::sources::delete_content_source(&state.db, source_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("source was not found"))?;

    Ok(Json(SourceResponse::from(source)))
}

async fn create_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateInviteRequest>,
) -> Result<Json<CreateInviteResponse>, ApiError> {
    let admin = require_admin(&state, &headers).await?;
    let email = api::email::validate_email_address(&payload.email)
        .map_err(|_| ApiError::bad_request("email address is invalid"))?;

    let created = invites::create_editor_invite(&state.db, &email, &admin.user.sub)
        .await
        .map_err(ApiError::internal)?;
    let invite_url =
        frontend_accept_invite_url(&headers, state.self_url.as_deref(), &created.token);
    let email_delivery = send_invite_email(&state, &email, &invite_url).await?;

    Ok(Json(CreateInviteResponse {
        invite: InviteResponse::from(created.invite),
        invite_url,
        email_delivery,
    }))
}

async fn accept_invite_redirect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AcceptInviteQuery>,
) -> Result<Redirect, ApiError> {
    let Some(auth) = &state.auth else {
        return Err(ApiError::service_unavailable(
            "auth service is not configured",
        ));
    };

    let return_to = frontend_accept_invite_url(&headers, state.self_url.as_deref(), &query.token);
    let login_url = auth.login_url(&return_to).map_err(ApiError::internal)?;

    Ok(Redirect::to(&login_url))
}

async fn accept_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AcceptInviteRequest>,
) -> Result<Json<AcceptInviteResponse>, ApiError> {
    if payload.token.trim().is_empty() {
        return Err(ApiError::bad_request("invite token is required"));
    }

    let authenticated = require_user(&state, &headers).await?;
    let invite = invites::accept_invite(
        &state.db,
        payload.token.trim(),
        &authenticated.user.sub,
        &authenticated.user.email,
    )
    .await
    .map_err(ApiError::internal)?
    .map_err(|error| match error {
        AcceptInviteError::InvalidOrExpired => ApiError::bad_request(
            "invite is invalid, expired, already accepted, or for another email",
        ),
    })?;

    let user = api::users::find_user_by_sub(&state.db, &authenticated.user.sub)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("accepted user was not found")))?;

    Ok(Json(AcceptInviteResponse {
        invite: InviteResponse::from(invite),
        user: AuthSessionUser::from(user),
    }))
}

async fn require_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, ApiError> {
    let Some(auth) = &state.auth else {
        return Err(ApiError::service_unavailable(
            "auth service is not configured",
        ));
    };

    auth.authenticate_cookie_header(&state.db, headers.get(COOKIE))
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("valid session is required"))
}

async fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, ApiError> {
    let authenticated = require_user(state, headers).await?;

    if !authenticated.user.role.can_manage_users() {
        return Err(ApiError::forbidden("admin permissions are required"));
    }

    Ok(authenticated)
}

fn ensure_positive_source_id(source_id: i64) -> Result<(), ApiError> {
    if source_id < 1 {
        return Err(ApiError::bad_request("source id must be positive"));
    }

    Ok(())
}

fn source_write_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("source name is required")
        || message.contains("source url")
        || message.contains("source url or external id")
        || message.contains("duplicate key value violates unique constraint")
        || message.contains("violates check constraint")
    {
        return ApiError::bad_request(message);
    }

    ApiError::internal(error)
}

async fn send_invite_email(
    state: &AppState,
    email: &str,
    invite_url: &str,
) -> Result<InviteEmailDelivery, ApiError> {
    let Some(email_service) = &state.email else {
        return Ok(InviteEmailDelivery::Skipped);
    };

    match email_service.send_invite(email, invite_url).await {
        Ok(EmailDelivery { message_id }) => Ok(InviteEmailDelivery::Sent { message_id }),
        Err(EmailSendError::RateLimited) => Ok(InviteEmailDelivery::RateLimited),
        Err(error) => Err(ApiError::internal(email_error_to_anyhow(error))),
    }
}

fn frontend_accept_invite_url(
    headers: &HeaderMap,
    configured_self_url: Option<&str>,
    token: &str,
) -> String {
    let root = auth::frontend_root_return_to(headers, configured_self_url);
    let relative = format!("/accept-invite?token={}", url_encode(token));

    if root == "/" {
        return relative;
    }

    let Ok(mut url) = url::Url::parse(&root) else {
        return relative;
    };

    url.set_path("/accept-invite");
    url.set_query(Some(&format!("token={}", url_encode(token))));
    url.into()
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

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            sub: user.sub,
            email: user.email,
            name: user.name,
            picture_url: user.picture_url,
            role: user.role,
            created_at: user.created_at,
            updated_at: user.updated_at,
            last_seen_at: user.last_seen_at,
        }
    }
}

impl From<ContentSource> for SourceResponse {
    fn from(source: ContentSource) -> Self {
        Self {
            id: source.id,
            name: source.name,
            kind: source.kind,
            url: source.url,
            external_id: source.external_id,
            created_by_sub: source.created_by_sub,
            enabled: source.enabled,
            created_at: source.created_at,
            updated_at: source.updated_at,
        }
    }
}

impl From<UserInvite> for InviteResponse {
    fn from(invite: UserInvite) -> Self {
        Self {
            email: invite.email,
            role: invite.role,
            invited_by_sub: invite.invited_by_sub,
            accepted_by_sub: invite.accepted_by_sub,
            created_at: invite.created_at,
            expires_at: invite.expires_at,
            accepted_at: invite.accepted_at,
        }
    }
}

fn url_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
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
