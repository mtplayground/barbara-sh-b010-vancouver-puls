use std::{env, net::SocketAddr};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub object_storage: Option<ObjectStorageConfig>,
    pub email: Option<EmailConfig>,
    pub auth: Option<AuthConfig>,
    pub instagram: Option<InstagramConfig>,
    pub claude: Option<ClaudeConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub self_url: Option<String>,
    pub allowed_cors_origin: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub prefix: String,
}

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub url: String,
    pub app_token: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub url: String,
    pub app_token: String,
    pub jwks_url: String,
}

#[derive(Debug, Clone)]
pub struct InstagramConfig {
    pub app_id: String,
    pub app_secret: String,
    pub redirect_uri: String,
    pub graph_api_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    pub api_key: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            server: ServerConfig::from_env()?,
            database: DatabaseConfig::from_env()?,
            object_storage: ObjectStorageConfig::from_env()?,
            email: EmailConfig::from_env()?,
            auth: AuthConfig::from_env()?,
            instagram: InstagramConfig::from_env()?,
            claude: ClaudeConfig::from_env()?,
        })
    }

    pub fn log_summary(&self) {
        let object_storage = self
            .object_storage
            .as_ref()
            .map(ObjectStorageConfig::summary)
            .unwrap_or_default();
        let email = self
            .email
            .as_ref()
            .map(EmailConfig::summary)
            .unwrap_or_default();
        let auth = self
            .auth
            .as_ref()
            .map(AuthConfig::summary)
            .unwrap_or_default();
        let instagram = self
            .instagram
            .as_ref()
            .map(InstagramConfig::summary)
            .unwrap_or_default();
        let claude = self
            .claude
            .as_ref()
            .map(ClaudeConfig::summary)
            .unwrap_or_default();

        tracing::info!(
            host = %self.server.host,
            port = self.server.port,
            self_url_configured = self.server.self_url.is_some(),
            allowed_cors_origin_configured = self.server.allowed_cors_origin.is_some(),
            database_max_connections = self.database.max_connections,
            object_storage_endpoint_configured = object_storage.endpoint_configured,
            object_storage_region_configured = object_storage.region_configured,
            object_storage_bucket_configured = object_storage.bucket_configured,
            object_storage_access_key_configured = object_storage.access_key_configured,
            object_storage_secret_key_configured = object_storage.secret_key_configured,
            object_storage_prefix_configured = object_storage.prefix_configured,
            email_url_configured = email.url_configured,
            email_token_configured = email.token_configured,
            auth_url_configured = auth.url_configured,
            auth_token_configured = auth.token_configured,
            auth_jwks_configured = auth.jwks_configured,
            instagram_app_id_configured = instagram.app_id_configured,
            instagram_app_secret_configured = instagram.app_secret_configured,
            instagram_redirect_uri_configured = instagram.redirect_uri_configured,
            instagram_graph_api_version_configured = instagram.graph_api_version_configured,
            claude_api_key_configured = claude.api_key_configured,
            "configuration loaded"
        );
    }
}

#[derive(Debug, Default)]
struct ObjectStorageSummary {
    endpoint_configured: bool,
    region_configured: bool,
    bucket_configured: bool,
    access_key_configured: bool,
    secret_key_configured: bool,
    prefix_configured: bool,
}

#[derive(Debug, Default)]
struct EmailSummary {
    url_configured: bool,
    token_configured: bool,
}

#[derive(Debug, Default)]
struct AuthSummary {
    url_configured: bool,
    token_configured: bool,
    jwks_configured: bool,
}

#[derive(Debug, Default)]
struct InstagramSummary {
    app_id_configured: bool,
    app_secret_configured: bool,
    redirect_uri_configured: bool,
    graph_api_version_configured: bool,
}

#[derive(Debug, Default)]
struct ClaudeSummary {
    api_key_configured: bool,
}

impl ServerConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            host: optional_var("HOST")?.unwrap_or_else(|| "0.0.0.0".to_owned()),
            port: parse_optional_u16("PORT", 8080)?,
            self_url: optional_var("SELF_URL")?,
            allowed_cors_origin: optional_var("ALLOWED_CORS_ORIGIN")?,
        })
    }

    pub fn bind_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse::<SocketAddr>()
            .with_context(|| format!("failed to parse bind address {}:{}", self.host, self.port))
    }
}

impl DatabaseConfig {
    fn from_env() -> Result<Self> {
        let max_connections = parse_optional_u32("DATABASE_MAX_CONNECTIONS", 5)
            .context("invalid database pool configuration")?;

        if max_connections == 0 {
            bail!("DATABASE_MAX_CONNECTIONS must be greater than zero");
        }

        Ok(Self {
            url: required_var("DATABASE_URL")?,
            max_connections,
        })
    }
}

impl ObjectStorageConfig {
    fn from_env() -> Result<Option<Self>> {
        let names = [
            "OBJECT_STORAGE_ENDPOINT",
            "OBJECT_STORAGE_REGION",
            "OBJECT_STORAGE_BUCKET",
            "OBJECT_STORAGE_ACCESS_KEY_ID",
            "OBJECT_STORAGE_SECRET_ACCESS_KEY",
            "OBJECT_STORAGE_PREFIX",
        ];

        if !any_var_present(&names)? {
            return Ok(None);
        }

        Ok(Some(Self {
            endpoint: required_var("OBJECT_STORAGE_ENDPOINT")?,
            region: required_var("OBJECT_STORAGE_REGION")?,
            bucket: required_var("OBJECT_STORAGE_BUCKET")?,
            access_key_id: required_var("OBJECT_STORAGE_ACCESS_KEY_ID")?,
            secret_access_key: required_var("OBJECT_STORAGE_SECRET_ACCESS_KEY")?,
            prefix: optional_var("OBJECT_STORAGE_PREFIX")?.unwrap_or_default(),
        }))
    }

    fn summary(&self) -> ObjectStorageSummary {
        ObjectStorageSummary {
            endpoint_configured: !self.endpoint.is_empty(),
            region_configured: !self.region.is_empty(),
            bucket_configured: !self.bucket.is_empty(),
            access_key_configured: !self.access_key_id.is_empty(),
            secret_key_configured: !self.secret_access_key.is_empty(),
            prefix_configured: !self.prefix.is_empty(),
        }
    }
}

impl EmailConfig {
    fn from_env() -> Result<Option<Self>> {
        let names = ["MCTAI_EMAIL_URL", "MCTAI_EMAIL_APP_TOKEN"];

        if !any_var_present(&names)? {
            return Ok(None);
        }

        Ok(Some(Self {
            url: required_var("MCTAI_EMAIL_URL")?,
            app_token: required_var("MCTAI_EMAIL_APP_TOKEN")?,
        }))
    }

    fn summary(&self) -> EmailSummary {
        EmailSummary {
            url_configured: !self.url.is_empty(),
            token_configured: !self.app_token.is_empty(),
        }
    }
}

impl AuthConfig {
    fn from_env() -> Result<Option<Self>> {
        let names = [
            "MCTAI_AUTH_URL",
            "MCTAI_AUTH_APP_TOKEN",
            "MCTAI_AUTH_JWKS_URL",
        ];

        if !any_var_present(&names)? {
            return Ok(None);
        }

        Ok(Some(Self {
            url: required_var("MCTAI_AUTH_URL")?,
            app_token: required_var("MCTAI_AUTH_APP_TOKEN")?,
            jwks_url: required_var("MCTAI_AUTH_JWKS_URL")?,
        }))
    }

    fn summary(&self) -> AuthSummary {
        AuthSummary {
            url_configured: !self.url.is_empty(),
            token_configured: !self.app_token.is_empty(),
            jwks_configured: !self.jwks_url.is_empty(),
        }
    }
}

impl InstagramConfig {
    fn from_env() -> Result<Option<Self>> {
        let names = [
            "INSTAGRAM_APP_ID",
            "INSTAGRAM_APP_SECRET",
            "INSTAGRAM_REDIRECT_URI",
            "INSTAGRAM_GRAPH_API_VERSION",
        ];

        if !any_var_present(&names)? {
            return Ok(None);
        }

        Ok(Some(Self {
            app_id: required_var("INSTAGRAM_APP_ID")?,
            app_secret: required_var("INSTAGRAM_APP_SECRET")?,
            redirect_uri: required_var("INSTAGRAM_REDIRECT_URI")?,
            graph_api_version: optional_var("INSTAGRAM_GRAPH_API_VERSION")?,
        }))
    }

    fn summary(&self) -> InstagramSummary {
        InstagramSummary {
            app_id_configured: !self.app_id.is_empty(),
            app_secret_configured: !self.app_secret.is_empty(),
            redirect_uri_configured: !self.redirect_uri.is_empty(),
            graph_api_version_configured: self.graph_api_version.is_some(),
        }
    }
}

impl ClaudeConfig {
    fn from_env() -> Result<Option<Self>> {
        Ok(optional_var("ANTHROPIC_API_KEY")?.map(|api_key| Self { api_key }))
    }

    fn summary(&self) -> ClaudeSummary {
        ClaudeSummary {
            api_key_configured: !self.api_key.is_empty(),
        }
    }
}

fn required_var(name: &str) -> Result<String> {
    optional_var(name)?.with_context(|| format!("{name} must be set"))
}

fn optional_var(name: &str) -> Result<Option<String>> {
    match env::var(name) {
        Ok(value) if value.is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => bail!("failed to read {name}: {error}"),
    }
}

fn any_var_present(names: &[&str]) -> Result<bool> {
    for name in names {
        if optional_var(name)?.is_some() {
            return Ok(true);
        }
    }

    Ok(false)
}

fn parse_optional_u16(name: &str, default: u16) -> Result<u16> {
    match optional_var(name)? {
        Some(value) => value
            .parse::<u16>()
            .with_context(|| format!("{name} must be a valid u16, got `{value}`")),
        None => Ok(default),
    }
}

fn parse_optional_u32(name: &str, default: u32) -> Result<u32> {
    match optional_var(name)? {
        Some(value) => value
            .parse::<u32>()
            .with_context(|| format!("{name} must be a valid u32, got `{value}`")),
        None => Ok(default),
    }
}
