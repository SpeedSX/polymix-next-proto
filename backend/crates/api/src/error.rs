use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::error::DomainError;
use serde::Serialize;
use serde_json::Value;

use crate::jwks::JwksError;

pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorPayload<'a>,
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: &'a Option<Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorPayload {
                code: self.code,
                message: &self.message,
                details: &self.details,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "not_found", "not found"),
            DomainError::Validation(details) => {
                let mut api_err = ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "validation_failed",
                    "validation failed",
                );
                api_err.details = Some(serde_json::to_value(details).unwrap_or_default());
                api_err
            }
            DomainError::Conflict(message) => {
                ApiError::new(StatusCode::CONFLICT, "conflict", message)
            }
            DomainError::Store(message) => {
                tracing::error!(error = %message, "store error");
                ApiError::internal("internal server error")
            }
        }
    }
}

impl From<JwksError> for ApiError {
    fn from(err: JwksError) -> Self {
        match err {
            // The JWKS endpoint being down/slow/malformed is an upstream
            // dependency failure, not the caller's fault — 401 here would
            // mislead debugging into suspecting the token instead.
            JwksError::FetchFailed(message) => {
                tracing::error!(error = %message, "jwks fetch failed");
                ApiError::internal("internal server error")
            }
            JwksError::UnknownKid(kid) => {
                ApiError::unauthorized(format!("unknown signing key: {kid}"))
            }
        }
    }
}
