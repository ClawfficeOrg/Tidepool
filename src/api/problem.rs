//! RFC 9457 problem details for the hub API (task 1.1.4).
//!
//! Every failure under `/v1/setup`, `/v1/pool` and `/v1/api-keys` is rendered
//! as `application/problem+json`:
//!
//! ```json
//! {
//!   "type": "https://tidepool.dev/problems/missing_scope",
//!   "title": "Forbidden",
//!   "status": 403,
//!   "detail": "API key lacks the required scope",
//!   "code": "missing_scope",
//!   "requestId": "6b1f..."
//! }
//! ```
//!
//! The game API (`/v1/auth`, `/v1/players`, ...) keeps its existing
//! `{"error": "..."}` bodies; [`crate::middleware::problem::HubProblem`] only
//! rewrites hub paths.
//!
//! # Code taxonomy
//!
//! | code | status | meaning |
//! |---|---|---|
//! | `unauthorized` | 401 | missing/invalid credentials |
//! | `forbidden` | 403 | authenticated but the role is not allowed |
//! | `missing_scope` | 403 | valid API key without the required scope |
//! | `invalid_request` | 400 | malformed or unusable input |
//! | `validation_failed` | 422 | field validation failed |
//! | `not_found` | 404 | no such member/invitation/key/route |
//! | `conflict` | 409 | already used / already initialized / last owner |
//! | `gone` | 410 | invitation expired or revoked |
//! | `method_not_allowed` | 405 | wrong method for a hub route |
//! | `rate_limited` | 429 | credential budget exhausted |
//! | `daemon_not_connected` | 409 | reserved for dispatch (phase 1.5) |
//! | `configuration_invalid` | 422 | reserved for install/validate (phase 1.3) |
//! | `service_unavailable` | 503 | database/redis unavailable |
//! | `internal_error` | 500 | unexpected server failure |
//!
//! Unknown parameters are not carried; `requestId` matches the
//! `x-request-id` response header and the tracing span.

use crate::middleware::request_id::RequestId;
use actix_web::{
    HttpRequest, HttpResponse, ResponseError,
    http::{StatusCode, header},
};
use serde::Serialize;

/// Media type required by RFC 9457.
pub const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

/// Base URI for problem type identifiers.
pub const PROBLEM_TYPE_BASE: &str = "https://tidepool.dev/problems";

/// Stable machine-readable failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemCode {
    /// Missing or invalid credentials.
    Unauthorized,
    /// Authenticated but the role is not allowed.
    Forbidden,
    /// Valid API key without the scope an endpoint needs.
    MissingScope,
    /// Malformed or unusable input.
    InvalidRequest,
    /// Field validation failed.
    ValidationFailed,
    /// No such resource.
    NotFound,
    /// State conflict (already used, already initialized, last owner).
    Conflict,
    /// Expired or revoked one-time resource.
    Gone,
    /// Wrong HTTP method.
    MethodNotAllowed,
    /// Credential budget exhausted.
    RateLimited,
    /// Reserved: no connected daemon can serve a dispatch.
    DaemonNotConnected,
    /// Reserved: configuration failed validation or installation.
    ConfigurationInvalid,
    /// Database or Redis unavailable.
    ServiceUnavailable,
    /// Unexpected server failure.
    InternalError,
}

impl ProblemCode {
    /// Wire spelling used in `code` and the type URI.
    pub fn as_str(self) -> &'static str {
        match self {
            ProblemCode::Unauthorized => "unauthorized",
            ProblemCode::Forbidden => "forbidden",
            ProblemCode::MissingScope => "missing_scope",
            ProblemCode::InvalidRequest => "invalid_request",
            ProblemCode::ValidationFailed => "validation_failed",
            ProblemCode::NotFound => "not_found",
            ProblemCode::Conflict => "conflict",
            ProblemCode::Gone => "gone",
            ProblemCode::MethodNotAllowed => "method_not_allowed",
            ProblemCode::RateLimited => "rate_limited",
            ProblemCode::DaemonNotConnected => "daemon_not_connected",
            ProblemCode::ConfigurationInvalid => "configuration_invalid",
            ProblemCode::ServiceUnavailable => "service_unavailable",
            ProblemCode::InternalError => "internal_error",
        }
    }

    /// Default code for an HTTP status when no handler-specific code is set.
    pub fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::UNAUTHORIZED => ProblemCode::Unauthorized,
            StatusCode::FORBIDDEN => ProblemCode::Forbidden,
            StatusCode::NOT_FOUND => ProblemCode::NotFound,
            StatusCode::CONFLICT => ProblemCode::Conflict,
            StatusCode::GONE => ProblemCode::Gone,
            StatusCode::METHOD_NOT_ALLOWED => ProblemCode::MethodNotAllowed,
            StatusCode::TOO_MANY_REQUESTS => ProblemCode::RateLimited,
            StatusCode::UNPROCESSABLE_ENTITY => ProblemCode::ValidationFailed,
            StatusCode::SERVICE_UNAVAILABLE => ProblemCode::ServiceUnavailable,
            StatusCode::INTERNAL_SERVER_ERROR => ProblemCode::InternalError,
            status if status.is_client_error() => ProblemCode::InvalidRequest,
            _ => ProblemCode::InternalError,
        }
    }
}

/// One RFC 9457 problem document plus hub extensions.
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// Stable type URI.
    #[serde(rename = "type")]
    pub type_uri: String,
    /// Short human-readable summary (the HTTP reason phrase).
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Specific human-readable explanation.
    pub detail: String,
    /// Stable machine-readable code (see module docs).
    pub code: String,
    /// Correlation id, also returned as `x-request-id`.
    #[serde(rename = "requestId")]
    pub request_id: String,
}

impl Problem {
    /// Build a problem with an explicit code.
    pub fn new(
        status: StatusCode,
        code: ProblemCode,
        detail: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            type_uri: format!("{PROBLEM_TYPE_BASE}/{}", code.as_str()),
            title: status.canonical_reason().unwrap_or("Error").to_string(),
            status: status.as_u16(),
            detail: detail.into(),
            code: code.as_str().to_string(),
            request_id: request_id.into(),
        }
    }

    /// Build a problem using the status-derived default code.
    pub fn for_status(status: StatusCode, request_id: impl Into<String>) -> Self {
        Self::new(
            status,
            ProblemCode::from_status(status),
            status.canonical_reason().unwrap_or("Error"),
            request_id,
        )
    }

    /// Build from a request, reading the `RequestId` extension.
    pub fn from_request(
        req: &HttpRequest,
        status: StatusCode,
        code: ProblemCode,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(status, code, detail, RequestId::from_request(req))
    }

    /// Render as an Actix response with the RFC 9457 media type.
    pub fn into_response(self) -> HttpResponse {
        let body = serde_json::to_vec(&self).unwrap_or_else(|_| {
            br#"{"type":"about:blank","title":"Internal Server Error","status":500,"detail":"serialization failed","code":"internal_error","requestId":""}"#.to_vec()
        });
        HttpResponse::build(
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        )
        .insert_header((header::CONTENT_TYPE, PROBLEM_CONTENT_TYPE))
        .body(body)
    }
}

/// An [`actix_web::Error`] that renders a problem document.
///
/// Used by extractors/guards (auth, API key scope) where returning a bare
/// status would otherwise lose the code and request id.
#[derive(Debug)]
pub struct ProblemError {
    /// Problem document carried through Actix's error pipeline.
    pub problem: Problem,
}

impl ProblemError {
    /// Wrap a problem.
    pub fn new(problem: Problem) -> Self {
        Self { problem }
    }

    /// Build from a request and explicit code.
    pub fn from_request(
        req: &HttpRequest,
        status: StatusCode,
        code: ProblemCode,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(Problem::from_request(req, status, code, detail))
    }
}

impl std::fmt::Display for ProblemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.problem.code, self.problem.detail)
    }
}

impl std::error::Error for ProblemError {}

impl ResponseError for ProblemError {
    fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn error_response(&self) -> HttpResponse {
        self.problem.clone().into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_serializes_the_rfc_shape() {
        let problem = Problem::new(
            StatusCode::FORBIDDEN,
            ProblemCode::MissingScope,
            "API key lacks the required scope",
            "req-1",
        );
        let json = serde_json::to_value(&problem).expect("serialize");
        assert_eq!(json["type"], "https://tidepool.dev/problems/missing_scope");
        assert_eq!(json["title"], "Forbidden");
        assert_eq!(json["status"], 403);
        assert_eq!(json["code"], "missing_scope");
        assert_eq!(json["requestId"], "req-1");
        assert_eq!(json["detail"], "API key lacks the required scope");
    }

    #[test]
    fn status_maps_to_the_documented_default_code() {
        let cases = [
            (StatusCode::UNAUTHORIZED, ProblemCode::Unauthorized),
            (StatusCode::FORBIDDEN, ProblemCode::Forbidden),
            (StatusCode::NOT_FOUND, ProblemCode::NotFound),
            (StatusCode::CONFLICT, ProblemCode::Conflict),
            (StatusCode::GONE, ProblemCode::Gone),
            (
                StatusCode::METHOD_NOT_ALLOWED,
                ProblemCode::MethodNotAllowed,
            ),
            (StatusCode::TOO_MANY_REQUESTS, ProblemCode::RateLimited),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                ProblemCode::ValidationFailed,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                ProblemCode::ServiceUnavailable,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
            ),
            (StatusCode::BAD_REQUEST, ProblemCode::InvalidRequest),
        ];
        for (status, expected) in cases {
            assert_eq!(ProblemCode::from_status(status), expected);
        }
    }

    #[test]
    fn reserved_codes_are_stable() {
        assert_eq!(
            ProblemCode::DaemonNotConnected.as_str(),
            "daemon_not_connected"
        );
        assert_eq!(
            ProblemCode::ConfigurationInvalid.as_str(),
            "configuration_invalid"
        );
    }

    #[test]
    fn response_uses_problem_json_content_type() {
        let response = Problem::new(
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            "bad input",
            "req-2",
        )
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some(PROBLEM_CONTENT_TYPE)
        );
    }
}
