use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    HeaderValue, Method,
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::ServerConfig;

pub fn cors_layer(config: &ServerConfig) -> CorsLayer {
    let allowed_origins = configured_origins(config);

    CorsLayer::new()
        .allow_credentials(true)
        .allow_methods([
            Method::DELETE,
            Method::GET,
            Method::OPTIONS,
            Method::PATCH,
            Method::POST,
            Method::PUT,
        ])
        .allow_headers([ACCEPT, AUTHORIZATION, CONTENT_TYPE])
        .allow_origin(AllowOrigin::predicate(move |origin, request_parts| {
            let forwarded_host = request_parts
                .headers
                .get("x-forwarded-host")
                .and_then(|value| value.to_str().ok());
            let forwarded_proto = request_parts
                .headers
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok());

            origin_is_allowed(origin, &allowed_origins, forwarded_host, forwarded_proto)
        }))
}

fn configured_origins(config: &ServerConfig) -> Vec<String> {
    [
        config.allowed_cors_origin.as_deref(),
        config.self_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(normalize_origin)
    .collect()
}

fn origin_is_allowed(
    origin: &HeaderValue,
    allowed_origins: &[String],
    forwarded_host: Option<&str>,
    forwarded_proto: Option<&str>,
) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some(origin) = normalize_origin(origin) else {
        return false;
    };

    if allowed_origins.iter().any(|allowed| allowed == &origin) {
        return true;
    }

    if is_local_dev_origin(&origin) {
        return true;
    }

    forwarded_host
        .and_then(normalize_host)
        .is_some_and(|host| origin_host(&origin).is_some_and(|origin_host| origin_host == host))
        || forwarded_origin(forwarded_host, forwarded_proto)
            .as_ref()
            .is_some_and(|forwarded_origin| forwarded_origin == &origin)
}

fn forwarded_origin(forwarded_host: Option<&str>, forwarded_proto: Option<&str>) -> Option<String> {
    let host = normalize_host(forwarded_host?)?;
    let proto = forwarded_proto
        .unwrap_or("https")
        .trim()
        .to_ascii_lowercase();

    if proto != "http" && proto != "https" {
        return None;
    }

    Some(format!("{proto}://{host}"))
}

fn is_local_dev_origin(origin: &str) -> bool {
    matches!(
        origin_host(origin),
        Some(host)
            if host == "localhost"
                || host.starts_with("localhost:")
                || host == "127.0.0.1"
                || host.starts_with("127.0.0.1:")
                || host == "[::1]"
                || host.starts_with("[::1]:")
    )
}

fn normalize_origin(origin: &str) -> Option<String> {
    let origin = origin.trim().trim_end_matches('/').to_ascii_lowercase();

    if origin.starts_with("http://") || origin.starts_with("https://") {
        Some(origin)
    } else {
        None
    }
}

fn normalize_host(host: &str) -> Option<String> {
    let host = host
        .split(',')
        .next()
        .map(str::trim)?
        .trim_end_matches('/')
        .to_ascii_lowercase();

    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn origin_host(origin: &str) -> Option<&str> {
    origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::origin_is_allowed;

    #[test]
    fn allows_configured_origin() {
        let origin = HeaderValue::from_static("https://example.mctai.app");
        let allowed = vec!["https://example.mctai.app".to_owned()];

        assert!(origin_is_allowed(&origin, &allowed, None, None));
    }

    #[test]
    fn allows_forwarded_public_host() {
        let origin = HeaderValue::from_static("https://public.mctai.app");

        assert!(origin_is_allowed(
            &origin,
            &[],
            Some("public.mctai.app"),
            Some("https"),
        ));
    }

    #[test]
    fn allows_localhost_for_dev() {
        let origin = HeaderValue::from_static("http://localhost:5173");

        assert!(origin_is_allowed(&origin, &[], None, None));
    }

    #[test]
    fn rejects_unconfigured_remote_origin() {
        let origin = HeaderValue::from_static("https://other.example.com");

        assert!(!origin_is_allowed(&origin, &[], None, None));
    }
}
