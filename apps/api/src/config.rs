use std::{env, net::SocketAddr};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    host: String,
    port: u16,
    database_url: String,
    database_max_connections: u32,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
        let port = match env::var("PORT") {
            Ok(value) => value
                .parse::<u16>()
                .with_context(|| format!("PORT must be a valid u16, got `{value}`"))?,
            Err(env::VarError::NotPresent) => 8080,
            Err(error) => bail!("failed to read PORT: {error}"),
        };

        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let database_max_connections = parse_optional_u32("DATABASE_MAX_CONNECTIONS", 5)
            .context("invalid database pool configuration")?;

        if database_max_connections == 0 {
            bail!("DATABASE_MAX_CONNECTIONS must be greater than zero");
        }

        Ok(Self {
            host,
            port,
            database_url,
            database_max_connections,
        })
    }

    pub fn bind_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse::<SocketAddr>()
            .with_context(|| format!("failed to parse bind address {}:{}", self.host, self.port))
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn database_max_connections(&self) -> u32 {
        self.database_max_connections
    }
}

fn parse_optional_u32(name: &str, default: u32) -> Result<u32> {
    match env::var(name) {
        Ok(value) => value
            .parse::<u32>()
            .with_context(|| format!("{name} must be a valid u32, got `{value}`")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => bail!("failed to read {name}: {error}"),
    }
}
