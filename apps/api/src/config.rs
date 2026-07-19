use std::{env, net::SocketAddr};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    host: String,
    port: u16,
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

        Ok(Self { host, port })
    }

    pub fn bind_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse::<SocketAddr>()
            .with_context(|| format!("failed to parse bind address {}:{}", self.host, self.port))
    }
}
