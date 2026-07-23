use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::error::DomainError;
use serde::Serialize;
use serde_json::Value;

use crate::jwks::JwksError;

#[derive(Debug)]
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
            DomainError::Conflict(reason) => {
                let mut api_err =
                    ApiError::new(StatusCode::CONFLICT, reason.code(), reason.to_string());
                api_err.details = reason
                    .details()
                    .map(|d| serde_json::to_value(d).unwrap_or_default());
                api_err
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use domain::error::ConflictReason;
    use domain::order::OrderStatus;
    use std::collections::HashMap;

    #[test]
    fn constructors_set_status_and_code() {
        let unauthorized = ApiError::unauthorized("nope");
        assert_eq!(unauthorized.status, StatusCode::UNAUTHORIZED);
        assert_eq!(unauthorized.code, "unauthorized");
        assert_eq!(unauthorized.message, "nope");
        assert!(unauthorized.details.is_none());

        assert_eq!(ApiError::forbidden("x").status, StatusCode::FORBIDDEN);
        assert_eq!(ApiError::forbidden("x").code, "forbidden");

        let internal = ApiError::internal("boom");
        assert_eq!(internal.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(internal.code, "internal");
    }

    #[test]
    fn not_found_maps_to_404() {
        let err = ApiError::from(DomainError::NotFound);
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "not_found");
        assert!(err.details.is_none());
    }

    #[test]
    fn validation_maps_to_422_with_details() {
        let mut fields = HashMap::new();
        fields.insert(
            "name".to_string(),
            domain::error::FieldError::code("required"),
        );
        let err = ApiError::from(DomainError::Validation(fields));
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "validation_failed");
        let details = err.details.expect("validation carries details");
        assert_eq!(details["name"]["code"], "required");
    }

    #[test]
    fn conflict_maps_to_409_with_reason_code_and_details() {
        let err = ApiError::from(DomainError::Conflict(
            ConflictReason::OrderStatusTransition {
                from: OrderStatus::Draft,
                to: OrderStatus::Completed,
            },
        ));
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.code, "order_status_transition");
        let details = err.details.expect("transition conflicts carry details");
        assert_eq!(details["from"], "0");
        assert_eq!(details["to"], "3");
    }

    #[test]
    fn conflict_without_details_leaves_details_none() {
        let err = ApiError::from(DomainError::Conflict(ConflictReason::CustomerHasOrders));
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.code, "customer_has_orders");
        assert!(err.details.is_none());
    }

    #[test]
    fn store_error_is_hidden_behind_a_generic_500() {
        let err = ApiError::from(DomainError::Store("connection reset".to_string()));
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "internal");
        // The raw store message must never leak to the client.
        assert!(!err.message.contains("connection reset"));
    }

    #[test]
    fn jwks_fetch_failure_is_a_generic_500() {
        let err = ApiError::from(JwksError::FetchFailed("upstream down".to_string()));
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "internal");
        assert!(!err.message.contains("upstream down"));
    }

    #[test]
    fn jwks_unknown_kid_is_a_401() {
        let err = ApiError::from(JwksError::UnknownKid("abc123".to_string()));
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.code, "unauthorized");
        assert!(err.message.contains("abc123"));
    }

    #[tokio::test]
    async fn into_response_serializes_the_error_envelope() {
        let response = ApiError::unauthorized("bad token").into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "unauthorized");
        assert_eq!(json["error"]["message"], "bad token");
        // `details` is skipped when None.
        assert!(json["error"].get("details").is_none());
    }

    #[tokio::test]
    async fn into_response_includes_details_when_present() {
        let mut err = ApiError::new(StatusCode::CONFLICT, "conflict", "boom");
        err.details = Some(serde_json::json!({ "from": "0", "to": "3" }));
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["details"]["from"], "0");
        assert_eq!(json["error"]["details"]["to"], "3");
    }
}
