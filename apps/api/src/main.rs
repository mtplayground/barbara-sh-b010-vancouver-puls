use anyhow::{Context, Result};
use api::{
    auth::{self, AuthService, AuthenticatedUser},
    backup_library::{
        BackupContentItem, BackupContentKind, NewBackupContentItem, UpdateBackupContentItem,
    },
    claude::ClaudeClient,
    config::{AppConfig, InstagramConfig},
    cors::cors_layer,
    db,
    drafting::{DraftingService, ManualDraftTopic},
    drafts::{DraftStatus, NewPostDraft, PostDraft, UpdatePostDraft},
    email::{email_error_to_anyhow, EmailDelivery, EmailSendError, EmailService},
    error::ApiError,
    ingestion,
    instagram::{InstagramAccountType, InstagramConnection, NewInstagramConnection},
    invites::{self, AcceptInviteError, UserInvite},
    publisher_scheduler,
    publishing::{InstagramPublishTarget, InstagramPublisher, PublishLog, PublishLogStatus},
    schedule::{self, NewScheduleAssignment, ScheduleSlot},
    sources::{
        ContentSource, ContentSourceKind, IngestedItem, NewContentSource, UpdateContentSource,
    },
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
use chrono::{NaiveDate, NaiveTime, Utc};
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
    instagram: Option<InstagramConfig>,
    drafting: Option<DraftingService>,
    publisher: InstagramPublisher,
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
struct InstagramStatusResponse {
    configured: bool,
    token_available: bool,
    env_account_available: bool,
    connected: bool,
    account: Option<InstagramConnectionResponse>,
}

#[derive(Debug, Serialize)]
struct InstagramConnectionResponse {
    instagram_account_id: String,
    username: Option<String>,
    account_type: InstagramAccountType,
    graph_api_version: String,
    app_id: String,
    token_source: String,
    token_configured: bool,
    connected_by_sub: Option<String>,
    disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    connected_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct ConnectInstagramRequest {
    instagram_account_id: Option<String>,
    username: Option<String>,
    account_type: Option<InstagramAccountType>,
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
struct InboxItemsQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct InboxItemsResponse {
    items: Vec<IngestedItemResponse>,
}

#[derive(Debug, Deserialize)]
struct DraftsQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct DraftsResponse {
    drafts: Vec<DraftResponse>,
}

#[derive(Debug, Serialize)]
struct DraftResponse {
    id: i64,
    source_item_id: Option<i64>,
    caption_en: String,
    caption_zh: String,
    status: DraftStatus,
    rendered_post_asset_ref: Option<String>,
    rendered_reel_asset_ref: Option<String>,
    created_by_sub: Option<String>,
    updated_by_sub: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateDraftRequest {
    source_item_id: Option<i64>,
    manual_topic: Option<String>,
    manual_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateDraftRequest {
    source_item_id: Option<Option<i64>>,
    caption_en: Option<String>,
    caption_zh: Option<String>,
    status: Option<DraftStatus>,
    rendered_post_asset_ref: Option<Option<String>>,
    rendered_reel_asset_ref: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct RegenerateDraftRequest {
    manual_topic: Option<String>,
    manual_notes: Option<String>,
}

#[derive(Debug, Serialize)]
struct RenderDraftResponse {
    draft: DraftResponse,
    post_asset_ref: String,
    reel_asset_ref: String,
}

#[derive(Debug, Deserialize)]
struct PublishDraftRequest {
    target: Option<InstagramPublishTarget>,
}

#[derive(Debug, Serialize)]
struct PublishDraftResponse {
    draft: DraftResponse,
    log: PublishLogResponse,
}

#[derive(Debug, Serialize)]
struct PublishLogsResponse {
    logs: Vec<PublishLogResponse>,
}

#[derive(Debug, Serialize)]
struct PublishLogResponse {
    id: i64,
    draft_id: i64,
    target: InstagramPublishTarget,
    status: PublishLogStatus,
    instagram_account_id: String,
    asset_ref: String,
    graph_container_id: Option<String>,
    graph_media_id: Option<String>,
    error_message: Option<String>,
    requested_by_sub: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CalendarQuery {
    start: Option<NaiveDate>,
    days: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CalendarResponse {
    start_date: NaiveDate,
    end_date: NaiveDate,
    daily_cadence: &'static str,
    empty_upcoming_slots: i64,
    slots: Vec<CalendarSlotResponse>,
}

#[derive(Debug, Serialize)]
struct CalendarSlotResponse {
    id: Option<i64>,
    slot_date: NaiveDate,
    slot_time: NaiveTime,
    draft: Option<DraftResponse>,
    is_empty: bool,
    is_upcoming: bool,
}

#[derive(Debug, Deserialize)]
struct AssignCalendarSlotRequest {
    slot_date: NaiveDate,
    slot_time: Option<NaiveTime>,
    draft_id: i64,
}

#[derive(Debug, Serialize)]
struct IngestedItemResponse {
    id: i64,
    source_id: i64,
    title: String,
    summary: Option<String>,
    link: String,
    media_ref: Option<String>,
    dedup_key: String,
    source_published_at: Option<chrono::DateTime<chrono::Utc>>,
    discovered_at: chrono::DateTime<chrono::Utc>,
    ingested_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct BackupContentQuery {
    limit: Option<i64>,
    active: Option<bool>,
}

#[derive(Debug, Serialize)]
struct BackupContentItemsResponse {
    items: Vec<BackupContentItemResponse>,
}

#[derive(Debug, Serialize)]
struct BackupContentItemResponse {
    id: i64,
    kind: BackupContentKind,
    title: String,
    body: String,
    source_url: Option<String>,
    media_ref: Option<String>,
    active: bool,
    created_by_sub: Option<String>,
    updated_by_sub: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateBackupContentItemRequest {
    kind: BackupContentKind,
    title: String,
    body: String,
    source_url: Option<String>,
    media_ref: Option<String>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateBackupContentItemRequest {
    kind: Option<BackupContentKind>,
    title: Option<String>,
    body: Option<String>,
    source_url: Option<Option<String>>,
    media_ref: Option<Option<String>>,
    active: Option<bool>,
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
    let drafting = config
        .claude
        .clone()
        .map(ClaudeClient::new)
        .map(DraftingService::new);
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

    let publisher = InstagramPublisher::new();
    let _ingestion_job = ingestion::spawn_ingestion_job(pool.clone(), storage.clone())
        .context("failed to start scheduled ingestion job")?;
    let _publisher_job = publisher_scheduler::spawn_scheduled_publisher_job(
        pool.clone(),
        storage.clone(),
        email.clone(),
        publisher.clone(),
    )
    .context("failed to start scheduled publisher job")?;

    let bind_addr = config.server.bind_addr()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on {bind_addr}"))?;

    info!("api listening on {bind_addr}");

    axum::serve(
        listener,
        app(pool, storage, auth, email, drafting, publisher, config),
    )
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
    drafting: Option<DraftingService>,
    publisher: InstagramPublisher,
    config: AppConfig,
) -> Router {
    let state = AppState {
        db: pool,
        storage,
        auth,
        email,
        instagram: config.instagram.clone(),
        drafting,
        publisher,
        self_url: config.server.self_url.clone(),
    };

    Router::new()
        .route("/healthz", get(health))
        .route("/api/health", get(health))
        .route("/api/health/db", get(database_health))
        .route("/api/health/storage", get(storage_health))
        .route("/api/auth/login", get(auth_login))
        .route("/api/auth/session", get(auth_session))
        .route("/api/settings/instagram", get(instagram_status))
        .route("/api/settings/instagram/connect", post(connect_instagram))
        .route(
            "/api/settings/instagram/disconnect",
            post(disconnect_instagram),
        )
        .route("/api/admin/invites", post(create_invite))
        .route("/api/admin/users", get(list_admin_users))
        .route("/api/admin/users/:sub/role", patch(update_admin_user_role))
        .route("/api/admin/sources", get(list_sources).post(create_source))
        .route(
            "/api/admin/sources/:source_id",
            patch(update_source).delete(delete_source),
        )
        .route("/api/inbox/items", get(list_inbox_items))
        .route(
            "/api/backup-library",
            get(list_backup_content_items).post(create_backup_content_item),
        )
        .route(
            "/api/backup-library/:item_id",
            patch(update_backup_content_item).delete(delete_backup_content_item),
        )
        .route("/api/drafts", get(list_drafts).post(create_draft))
        .route("/api/drafts/:draft_id", get(get_draft).patch(update_draft))
        .route("/api/drafts/:draft_id/approve", post(approve_draft))
        .route("/api/drafts/:draft_id/reject", post(reject_draft))
        .route("/api/drafts/:draft_id/regenerate", post(regenerate_draft))
        .route("/api/drafts/:draft_id/render", post(render_draft_assets))
        .route("/api/drafts/:draft_id/publish", post(publish_draft))
        .route("/api/drafts/:draft_id/publish-log", get(list_publish_logs))
        .route("/api/calendar", get(list_calendar))
        .route("/api/calendar/slots", post(assign_calendar_slot))
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

async fn instagram_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InstagramStatusResponse>, ApiError> {
    require_admin(&state, &headers).await?;

    instagram_status_response(&state).await.map(Json)
}

async fn connect_instagram(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ConnectInstagramRequest>,
) -> Result<Json<InstagramStatusResponse>, ApiError> {
    let admin = require_admin(&state, &headers).await?;
    let config = instagram_config(&state)?;
    let access_token = config
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::service_unavailable("Instagram access token is not configured"))?;
    let instagram_account_id = payload
        .instagram_account_id
        .as_deref()
        .or(config.business_account_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("Instagram account id is required"))?;
    let username = payload.username.or_else(|| config.username.clone());
    let connection = NewInstagramConnection {
        instagram_account_id: instagram_account_id.to_owned(),
        username,
        account_type: payload
            .account_type
            .unwrap_or(InstagramAccountType::Business),
        graph_api_version: config
            .graph_api_version
            .clone()
            .unwrap_or_else(|| "v20.0".to_owned()),
        app_id: config.app_id.clone(),
        access_token: access_token.to_owned(),
        token_source: "environment".to_owned(),
        connected_by_sub: Some(admin.user.sub),
    };

    api::instagram::connect_instagram_account(&state.db, &connection)
        .await
        .map_err(instagram_connection_error)?;

    instagram_status_response(&state).await.map(Json)
}

async fn disconnect_instagram(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InstagramStatusResponse>, ApiError> {
    require_admin(&state, &headers).await?;

    api::instagram::disconnect_instagram_account(&state.db)
        .await
        .map_err(ApiError::internal)?;

    instagram_status_response(&state).await.map(Json)
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

async fn list_inbox_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<InboxItemsQuery>,
) -> Result<Json<InboxItemsResponse>, ApiError> {
    require_user(&state, &headers).await?;

    let items = api::sources::list_recent_ingested_items(&state.db, query.limit.unwrap_or(50))
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(IngestedItemResponse::from)
        .collect();

    Ok(Json(InboxItemsResponse { items }))
}

async fn list_backup_content_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BackupContentQuery>,
) -> Result<Json<BackupContentItemsResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    let items = api::backup_library::list_backup_content_items(
        &state.db,
        query.limit.unwrap_or(50),
        query.active,
    )
    .await
    .map_err(ApiError::internal)?
    .into_iter()
    .map(BackupContentItemResponse::from)
    .collect();

    Ok(Json(BackupContentItemsResponse { items }))
}

async fn create_backup_content_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateBackupContentItemRequest>,
) -> Result<Json<BackupContentItemResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_draft_content() {
        return Err(ApiError::forbidden(
            "content draft permissions are required",
        ));
    }

    let item = NewBackupContentItem {
        kind: payload.kind,
        title: payload.title,
        body: payload.body,
        source_url: payload.source_url,
        media_ref: payload.media_ref,
        active: payload.active,
        created_by_sub: Some(authenticated.user.sub),
    };
    let created = api::backup_library::create_backup_content_item(&state.db, &item)
        .await
        .map_err(backup_content_write_error)?;

    Ok(Json(BackupContentItemResponse::from(created)))
}

async fn update_backup_content_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<i64>,
    Json(payload): Json<UpdateBackupContentItemRequest>,
) -> Result<Json<BackupContentItemResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_draft_content() {
        return Err(ApiError::forbidden(
            "content draft permissions are required",
        ));
    }

    ensure_positive_backup_content_item_id(item_id)?;

    if payload.kind.is_none()
        && payload.title.is_none()
        && payload.body.is_none()
        && payload.source_url.is_none()
        && payload.media_ref.is_none()
        && payload.active.is_none()
    {
        return Err(ApiError::bad_request(
            "backup content update has no changes",
        ));
    }

    let update = UpdateBackupContentItem {
        kind: payload.kind,
        title: payload.title,
        body: payload.body,
        source_url: payload.source_url,
        media_ref: payload.media_ref,
        active: payload.active,
        updated_by_sub: Some(authenticated.user.sub),
    };
    let updated = api::backup_library::update_backup_content_item(&state.db, item_id, &update)
        .await
        .map_err(backup_content_write_error)?
        .ok_or_else(|| ApiError::not_found_message("backup content item was not found"))?;

    Ok(Json(BackupContentItemResponse::from(updated)))
}

async fn delete_backup_content_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<i64>,
) -> Result<Json<BackupContentItemResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_draft_content() {
        return Err(ApiError::forbidden(
            "content draft permissions are required",
        ));
    }

    ensure_positive_backup_content_item_id(item_id)?;

    let deleted = api::backup_library::delete_backup_content_item(&state.db, item_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("backup content item was not found"))?;

    Ok(Json(BackupContentItemResponse::from(deleted)))
}

async fn list_drafts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DraftsQuery>,
) -> Result<Json<DraftsResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    let drafts = api::drafts::list_post_drafts(&state.db, query.limit.unwrap_or(50))
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(DraftResponse::from)
        .collect();

    Ok(Json(DraftsResponse { drafts }))
}

async fn get_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
) -> Result<Json<DraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    ensure_positive_draft_id(draft_id)?;

    let draft = api::drafts::find_post_draft(&state.db, draft_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    Ok(Json(DraftResponse::from(draft)))
}

async fn create_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateDraftRequest>,
) -> Result<Json<DraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_draft_content() {
        return Err(ApiError::forbidden(
            "content draft permissions are required",
        ));
    }

    let captions = match draft_generation_input(&state, &payload).await? {
        DraftGenerationInput::IngestedItem(item) => drafting_service(&state)?
            .generate_from_ingested_item(&item)
            .await
            .map_err(drafting_error)?,
        DraftGenerationInput::ManualTopic(topic) => drafting_service(&state)?
            .generate_from_manual_topic(&topic)
            .await
            .map_err(drafting_error)?,
    };

    let draft = NewPostDraft {
        source_item_id: payload.source_item_id,
        caption_en: captions.caption_en,
        caption_zh: captions.caption_zh,
        status: Some(DraftStatus::Draft),
        rendered_post_asset_ref: None,
        rendered_reel_asset_ref: None,
        created_by_sub: Some(authenticated.user.sub),
    };
    let created = api::drafts::create_post_draft(&state.db, &draft)
        .await
        .map_err(draft_write_error)?;

    Ok(Json(DraftResponse::from(created)))
}

async fn update_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
    Json(payload): Json<UpdateDraftRequest>,
) -> Result<Json<DraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    ensure_positive_draft_id(draft_id)?;

    let existing = api::drafts::find_post_draft(&state.db, draft_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    if !existing.status.is_editable() && update_contains_content_changes(&payload) {
        return Err(ApiError::bad_request(
            "published, rejected, archived, or scheduled drafts can only receive workflow status changes",
        ));
    }

    if payload.source_item_id.is_none()
        && payload.caption_en.is_none()
        && payload.caption_zh.is_none()
        && payload.status.is_none()
        && payload.rendered_post_asset_ref.is_none()
        && payload.rendered_reel_asset_ref.is_none()
    {
        return Err(ApiError::bad_request("draft update has no changes"));
    }

    if let Some(Some(source_item_id)) = payload.source_item_id {
        find_required_ingested_item(&state, source_item_id).await?;
    }

    let update = UpdatePostDraft {
        source_item_id: payload.source_item_id,
        caption_en: payload.caption_en,
        caption_zh: payload.caption_zh,
        status: payload.status,
        rendered_post_asset_ref: payload.rendered_post_asset_ref,
        rendered_reel_asset_ref: payload.rendered_reel_asset_ref,
        updated_by_sub: Some(authenticated.user.sub),
    };
    let updated = api::drafts::update_post_draft(&state.db, draft_id, &update)
        .await
        .map_err(draft_write_error)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    Ok(Json(DraftResponse::from(updated)))
}

async fn approve_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
) -> Result<Json<DraftResponse>, ApiError> {
    update_draft_workflow_status(&state, &headers, draft_id, DraftStatus::Approved).await
}

async fn reject_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
) -> Result<Json<DraftResponse>, ApiError> {
    update_draft_workflow_status(&state, &headers, draft_id, DraftStatus::Rejected).await
}

async fn update_draft_workflow_status(
    state: &AppState,
    headers: &HeaderMap,
    draft_id: i64,
    status: DraftStatus,
) -> Result<Json<DraftResponse>, ApiError> {
    let authenticated = require_user(state, headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    ensure_positive_draft_id(draft_id)?;

    let update = UpdatePostDraft {
        source_item_id: None,
        caption_en: None,
        caption_zh: None,
        status: Some(status),
        rendered_post_asset_ref: None,
        rendered_reel_asset_ref: None,
        updated_by_sub: Some(authenticated.user.sub),
    };
    let updated = api::drafts::update_post_draft(&state.db, draft_id, &update)
        .await
        .map_err(draft_write_error)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    Ok(Json(DraftResponse::from(updated)))
}

async fn regenerate_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
    Json(payload): Json<RegenerateDraftRequest>,
) -> Result<Json<DraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_draft_content() {
        return Err(ApiError::forbidden(
            "content draft permissions are required",
        ));
    }

    ensure_positive_draft_id(draft_id)?;

    let existing = api::drafts::find_post_draft(&state.db, draft_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    if !existing.status.is_editable() {
        return Err(ApiError::bad_request(
            "published or archived drafts cannot be regenerated",
        ));
    }

    let captions = if let Some(topic) = manual_topic_from_regenerate_payload(&payload) {
        drafting_service(&state)?
            .generate_from_manual_topic(&topic)
            .await
            .map_err(drafting_error)?
    } else if let Some(source_item_id) = existing.source_item_id {
        let item = find_required_ingested_item(&state, source_item_id).await?;
        drafting_service(&state)?
            .generate_from_ingested_item(&item)
            .await
            .map_err(drafting_error)?
    } else {
        let fallback_topic = ManualDraftTopic {
            topic: existing.caption_en.clone(),
            notes: Some(existing.caption_zh.clone()),
        };
        drafting_service(&state)?
            .generate_from_manual_topic(&fallback_topic)
            .await
            .map_err(drafting_error)?
    };

    let update = UpdatePostDraft {
        source_item_id: None,
        caption_en: Some(captions.caption_en),
        caption_zh: Some(captions.caption_zh),
        status: Some(DraftStatus::Draft),
        rendered_post_asset_ref: None,
        rendered_reel_asset_ref: None,
        updated_by_sub: Some(authenticated.user.sub),
    };
    let updated = api::drafts::update_post_draft(&state.db, draft_id, &update)
        .await
        .map_err(draft_write_error)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    Ok(Json(DraftResponse::from(updated)))
}

async fn render_draft_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
) -> Result<Json<RenderDraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    ensure_positive_draft_id(draft_id)?;

    let storage = storage_service(&state)?;
    let existing = api::drafts::find_post_draft(&state.db, draft_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    if !existing.status.is_editable() {
        return Err(ApiError::bad_request(
            "published or archived drafts cannot be rendered",
        ));
    }

    let rendered = api::rendering::render_and_store_draft_assets(storage, &existing)
        .await
        .map_err(rendering_error)?;
    let update = UpdatePostDraft {
        source_item_id: None,
        caption_en: None,
        caption_zh: None,
        status: None,
        rendered_post_asset_ref: Some(Some(rendered.post_asset_ref.clone())),
        rendered_reel_asset_ref: Some(Some(rendered.reel_asset_ref.clone())),
        updated_by_sub: Some(authenticated.user.sub),
    };
    let updated = api::drafts::update_post_draft(&state.db, draft_id, &update)
        .await
        .map_err(draft_write_error)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;

    Ok(Json(RenderDraftResponse {
        draft: DraftResponse::from(updated),
        post_asset_ref: rendered.post_asset_ref,
        reel_asset_ref: rendered.reel_asset_ref,
    }))
}

async fn publish_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
    Json(payload): Json<PublishDraftRequest>,
) -> Result<Json<PublishDraftResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_schedule_content() {
        return Err(ApiError::forbidden(
            "content publish permissions are required",
        ));
    }

    ensure_positive_draft_id(draft_id)?;

    let storage = storage_service(&state)?;
    let draft = api::drafts::find_post_draft(&state.db, draft_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("draft was not found"))?;
    let target = payload.target.unwrap_or(InstagramPublishTarget::Post);
    let connection = active_instagram_connection(&state).await?;
    let published = api::publishing::publish_draft_to_instagram(
        &state.db,
        storage,
        &state.publisher,
        &draft,
        &connection,
        target,
        Some(&authenticated.user.sub),
    )
    .await
    .map_err(publishing_error)?;

    Ok(Json(PublishDraftResponse {
        draft: DraftResponse::from(published.draft),
        log: PublishLogResponse::from(published.log),
    }))
}

async fn list_publish_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<i64>,
) -> Result<Json<PublishLogsResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_edit_content() {
        return Err(ApiError::forbidden("content edit permissions are required"));
    }

    ensure_positive_draft_id(draft_id)?;

    let logs = api::publishing::list_publish_logs_for_draft(&state.db, draft_id)
        .await
        .map_err(publishing_error)?
        .into_iter()
        .map(PublishLogResponse::from)
        .collect();

    Ok(Json(PublishLogsResponse { logs }))
}

async fn list_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CalendarQuery>,
) -> Result<Json<CalendarResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_schedule_content() {
        return Err(ApiError::forbidden(
            "content schedule permissions are required",
        ));
    }

    let start_date = query.start.unwrap_or_else(|| Utc::now().date_naive());
    let end_date = schedule::calendar_end_date(
        start_date,
        query.days.unwrap_or(schedule::DEFAULT_CALENDAR_DAYS),
    )
    .map_err(calendar_error)?;
    let persisted_slots = schedule::list_schedule_slots(&state.db, start_date, end_date)
        .await
        .map_err(calendar_error)?;
    let calendar_dates = schedule::calendar_dates(start_date, end_date).map_err(calendar_error)?;
    let today = Utc::now().date_naive();
    let mut slots = Vec::with_capacity(calendar_dates.len());

    for slot_date in calendar_dates {
        let persisted_slot = persisted_slots
            .iter()
            .find(|slot| slot.slot_date == slot_date);
        let draft = match persisted_slot.and_then(|slot| slot.draft_id) {
            Some(draft_id) => api::drafts::find_post_draft(&state.db, draft_id)
                .await
                .map_err(ApiError::internal)?
                .map(DraftResponse::from),
            None => None,
        };

        slots.push(CalendarSlotResponse::from_parts(
            persisted_slot,
            slot_date,
            draft,
            today,
        ));
    }

    let empty_upcoming_slots = slots
        .iter()
        .filter(|slot| slot.is_empty && slot.is_upcoming)
        .count() as i64;

    Ok(Json(CalendarResponse {
        start_date,
        end_date,
        daily_cadence: "one_post_per_day",
        empty_upcoming_slots,
        slots,
    }))
}

async fn assign_calendar_slot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AssignCalendarSlotRequest>,
) -> Result<Json<CalendarSlotResponse>, ApiError> {
    let authenticated = require_user(&state, &headers).await?;

    if !authenticated.user.role.can_schedule_content() {
        return Err(ApiError::forbidden(
            "content schedule permissions are required",
        ));
    }

    let slot = schedule::assign_approved_draft_to_slot(
        &state.db,
        &NewScheduleAssignment {
            slot_date: payload.slot_date,
            slot_time: payload.slot_time,
            draft_id: payload.draft_id,
            user_sub: Some(authenticated.user.sub),
        },
    )
    .await
    .map_err(calendar_error)?;
    let draft = match slot.draft_id {
        Some(draft_id) => api::drafts::find_post_draft(&state.db, draft_id)
            .await
            .map_err(ApiError::internal)?
            .map(DraftResponse::from),
        None => None,
    };
    let today = Utc::now().date_naive();

    Ok(Json(CalendarSlotResponse::from_parts(
        Some(&slot),
        slot.slot_date,
        draft,
        today,
    )))
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

fn drafting_service(state: &AppState) -> Result<&DraftingService, ApiError> {
    state
        .drafting
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("drafting service is not configured"))
}

fn storage_service(state: &AppState) -> Result<&ObjectStorage, ApiError> {
    state
        .storage
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("object storage is not configured"))
}

fn instagram_config(state: &AppState) -> Result<&InstagramConfig, ApiError> {
    state
        .instagram
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Instagram settings are not configured"))
}

async fn instagram_status_response(state: &AppState) -> Result<InstagramStatusResponse, ApiError> {
    let connection = api::instagram::find_instagram_connection(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let active_connection = connection.filter(|connection| connection.disconnected_at.is_none());
    let configured = state.instagram.is_some();
    let token_available = state
        .instagram
        .as_ref()
        .and_then(|config| config.access_token.as_deref())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let env_account_available = state
        .instagram
        .as_ref()
        .and_then(|config| config.business_account_id.as_deref())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    Ok(InstagramStatusResponse {
        configured,
        token_available,
        env_account_available,
        connected: active_connection.is_some(),
        account: active_connection.map(InstagramConnectionResponse::from),
    })
}

async fn active_instagram_connection(state: &AppState) -> Result<InstagramConnection, ApiError> {
    let connection = api::instagram::find_instagram_connection(&state.db)
        .await
        .map_err(ApiError::internal)?
        .filter(|connection| connection.disconnected_at.is_none())
        .ok_or_else(|| ApiError::service_unavailable("Instagram account is not connected"))?;

    if connection.access_token.trim().is_empty() {
        return Err(ApiError::service_unavailable(
            "Instagram access token is not configured",
        ));
    }

    Ok(connection)
}

async fn draft_generation_input(
    state: &AppState,
    payload: &CreateDraftRequest,
) -> Result<DraftGenerationInput, ApiError> {
    if state.drafting.is_none() {
        return Err(ApiError::service_unavailable(
            "drafting service is not configured",
        ));
    }

    let manual_topic = payload
        .manual_topic
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (payload.source_item_id, manual_topic) {
        (Some(_), Some(_)) => Err(ApiError::bad_request(
            "provide either source_item_id or manual_topic, not both",
        )),
        (Some(source_item_id), None) => find_required_ingested_item(state, source_item_id)
            .await
            .map(DraftGenerationInput::IngestedItem),
        (None, Some(topic)) => Ok(DraftGenerationInput::ManualTopic(ManualDraftTopic {
            topic: topic.to_owned(),
            notes: payload.manual_notes.clone(),
        })),
        (None, None) => Err(ApiError::bad_request(
            "source_item_id or manual_topic is required",
        )),
    }
}

async fn find_required_ingested_item(
    state: &AppState,
    item_id: i64,
) -> Result<IngestedItem, ApiError> {
    if item_id < 1 {
        return Err(ApiError::bad_request("source item id must be positive"));
    }

    api::sources::find_ingested_item(&state.db, item_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found_message("source item was not found"))
}

fn manual_topic_from_regenerate_payload(
    payload: &RegenerateDraftRequest,
) -> Option<ManualDraftTopic> {
    payload
        .manual_topic
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|topic| ManualDraftTopic {
            topic: topic.to_owned(),
            notes: payload.manual_notes.clone(),
        })
}

enum DraftGenerationInput {
    IngestedItem(IngestedItem),
    ManualTopic(ManualDraftTopic),
}

fn ensure_positive_draft_id(draft_id: i64) -> Result<(), ApiError> {
    if draft_id < 1 {
        return Err(ApiError::bad_request("draft id must be positive"));
    }

    Ok(())
}

fn update_contains_content_changes(payload: &UpdateDraftRequest) -> bool {
    payload.source_item_id.is_some()
        || payload.caption_en.is_some()
        || payload.caption_zh.is_some()
        || payload.rendered_post_asset_ref.is_some()
        || payload.rendered_reel_asset_ref.is_some()
}

fn ensure_positive_source_id(source_id: i64) -> Result<(), ApiError> {
    if source_id < 1 {
        return Err(ApiError::bad_request("source id must be positive"));
    }

    Ok(())
}

fn ensure_positive_backup_content_item_id(item_id: i64) -> Result<(), ApiError> {
    if item_id < 1 {
        return Err(ApiError::bad_request(
            "backup content item id must be positive",
        ));
    }

    Ok(())
}

fn draft_write_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("source item id must be positive")
        || message.contains("English caption is required")
        || message.contains("Chinese caption is required")
        || message.contains("violates check constraint")
        || message.contains("violates foreign key constraint")
    {
        return ApiError::bad_request(message);
    }

    ApiError::internal(error)
}

fn drafting_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("manual draft topic is required") {
        return ApiError::bad_request(message);
    }

    if message.contains("rate limited") {
        return ApiError::service_unavailable("drafting service is rate limited");
    }

    ApiError::internal(error)
}

fn rendering_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("draft id must be positive") || message.contains("object key") {
        return ApiError::bad_request(message);
    }

    ApiError::internal(error)
}

fn publishing_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("draft id must be positive")
        || message.contains("Instagram account id is required")
        || message.contains("Instagram access token is required")
        || message.contains("Instagram Graph API version is required")
        || message.contains("Instagram media URL")
        || message.contains("Instagram caption is required")
        || message.contains("published asset ref is required")
        || message.contains("successful publish log requires")
        || message.contains("failed publish log requires")
        || message.contains("object key")
        || message.contains("violates check constraint")
        || message.contains("violates foreign key constraint")
    {
        return ApiError::bad_request(message);
    }

    if message.contains("Instagram Graph API")
        || message.contains("failed to create Instagram media container")
        || message.contains("failed to publish Instagram media container")
    {
        return ApiError::service_unavailable(message);
    }

    ApiError::internal(error)
}

fn calendar_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("calendar end date")
        || message.contains("date range")
        || message.contains("draft id must be positive")
        || message.contains("draft was not found")
        || message.contains("only approved drafts")
        || message.contains("already assigned")
        || message.contains("slot already")
        || message.contains("cannot be in the past")
        || message.contains("violates check constraint")
        || message.contains("violates unique constraint")
        || message.contains("violates foreign key constraint")
    {
        return ApiError::bad_request(message);
    }

    ApiError::internal(error)
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

fn backup_content_write_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("backup content title is required")
        || message.contains("backup content body is required")
        || message.contains("backup content source url")
        || message.contains("violates check constraint")
        || message.contains("violates foreign key constraint")
    {
        return ApiError::bad_request(message);
    }

    ApiError::internal(error)
}

fn instagram_connection_error(error: anyhow::Error) -> ApiError {
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");

    if message.contains("Instagram account id is required")
        || message.contains("Instagram Graph API version is required")
        || message.contains("Instagram app id is required")
        || message.contains("Instagram access token is required")
        || message.contains("Instagram token source is required")
        || message.contains("violates check constraint")
        || message.contains("violates foreign key constraint")
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

impl From<IngestedItem> for IngestedItemResponse {
    fn from(item: IngestedItem) -> Self {
        Self {
            id: item.id,
            source_id: item.source_id,
            title: item.title,
            summary: item.summary,
            link: item.link,
            media_ref: item.media_ref,
            dedup_key: item.dedup_key,
            source_published_at: item.source_published_at,
            discovered_at: item.discovered_at,
            ingested_at: item.ingested_at,
            updated_at: item.updated_at,
        }
    }
}

impl From<InstagramConnection> for InstagramConnectionResponse {
    fn from(connection: InstagramConnection) -> Self {
        Self {
            instagram_account_id: connection.instagram_account_id,
            username: connection.username,
            account_type: connection.account_type,
            graph_api_version: connection.graph_api_version,
            app_id: connection.app_id,
            token_source: connection.token_source,
            token_configured: !connection.access_token.trim().is_empty(),
            connected_by_sub: connection.connected_by_sub,
            disconnected_at: connection.disconnected_at,
            connected_at: connection.connected_at,
            updated_at: connection.updated_at,
        }
    }
}

impl From<BackupContentItem> for BackupContentItemResponse {
    fn from(item: BackupContentItem) -> Self {
        Self {
            id: item.id,
            kind: item.kind,
            title: item.title,
            body: item.body,
            source_url: item.source_url,
            media_ref: item.media_ref,
            active: item.active,
            created_by_sub: item.created_by_sub,
            updated_by_sub: item.updated_by_sub,
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

impl From<PostDraft> for DraftResponse {
    fn from(draft: PostDraft) -> Self {
        Self {
            id: draft.id,
            source_item_id: draft.source_item_id,
            caption_en: draft.caption_en,
            caption_zh: draft.caption_zh,
            status: draft.status,
            rendered_post_asset_ref: draft.rendered_post_asset_ref,
            rendered_reel_asset_ref: draft.rendered_reel_asset_ref,
            created_by_sub: draft.created_by_sub,
            updated_by_sub: draft.updated_by_sub,
            created_at: draft.created_at,
            updated_at: draft.updated_at,
        }
    }
}

impl From<PublishLog> for PublishLogResponse {
    fn from(log: PublishLog) -> Self {
        Self {
            id: log.id,
            draft_id: log.draft_id,
            target: log.target,
            status: log.status,
            instagram_account_id: log.instagram_account_id,
            asset_ref: log.asset_ref,
            graph_container_id: log.graph_container_id,
            graph_media_id: log.graph_media_id,
            error_message: log.error_message,
            requested_by_sub: log.requested_by_sub,
            created_at: log.created_at,
        }
    }
}

impl CalendarSlotResponse {
    fn from_parts(
        slot: Option<&ScheduleSlot>,
        slot_date: NaiveDate,
        draft: Option<DraftResponse>,
        today: NaiveDate,
    ) -> Self {
        let slot_time = slot
            .map(|slot| slot.slot_time)
            .unwrap_or_else(schedule::default_slot_time);

        Self {
            id: slot.map(|slot| slot.id),
            slot_date,
            slot_time,
            is_empty: draft.is_none(),
            is_upcoming: slot_date >= today,
            draft,
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
