use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    source: Option<anyhow::Error>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorPayload,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            source: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: message.into(),
            source: None,
        }
    }

    pub fn internal(source: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "request failed".to_owned(),
            source: Some(source),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "route not found".to_owned(),
            source: None,
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "service_unavailable",
            message: message.into(),
            source: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            if let Some(source) = &self.source {
                tracing::error!(
                    status = self.status.as_u16(),
                    code = self.code,
                    error = ?source,
                    "request failed"
                );
            } else {
                tracing::error!(
                    status = self.status.as_u16(),
                    code = self.code,
                    "request failed without source error"
                );
            }
        }

        let status = self.status;
        let body = ErrorBody {
            error: ErrorPayload {
                code: self.code,
                message: self.message,
            },
        };

        (status, Json(body)).into_response()
    }
}
