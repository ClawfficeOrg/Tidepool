//! Hub problem-details response middleware (task 1.1.4).
//!
//! Handler-level problems are already rendered by [`crate::api::problem`].
//! This middleware is the safety net for everything else under the hub
//! prefixes — extractor failures (401/403), unknown paths (404), wrong methods
//! (405), rate limiting (429) and unexpected 5xx errors:
//!
//! - if the response is already `application/problem+json`, it is left alone;
//! - otherwise a status-derived problem document is substituted.
//!
//! The game API is untouched: its existing `{"error": "..."}` bodies must keep
//! their shape.

use crate::api::problem::{PROBLEM_CONTENT_TYPE, Problem, ProblemCode};
use crate::middleware::request_id::RequestId;
use actix_web::{
    Error,
    body::{BoxBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header,
    web,
};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

/// Hub path prefixes that use the problem-details contract.
const HUB_PREFIXES: &[&str] = &[
    "/v1/setup",
    "/v1/pool",
    "/v1/api-keys",
    "/v1/hosts",
    "/v1/connections",
];

/// True for a hub route (segment-boundary match, so `/v1/poolside` is not hub).
pub fn is_hub_path(path: &str) -> bool {
    HUB_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}

/// Transform factory for the hub problem middleware.
#[derive(Default)]
pub struct HubProblem;

impl<S, B> Transform<S, ServiceRequest> for HubProblem
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = HubProblemService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(HubProblemService { service }))
    }
}

/// Per-worker hub problem service.
pub struct HubProblemService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for HubProblemService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let request_id = RequestId::from_request(req.request());
        let hub = is_hub_path(req.path());
        let fut = self.service.call(req);

        Box::pin(async move {
            let response = fut.await?;
            if !hub || !response.status().is_client_error() && !response.status().is_server_error()
            {
                return Ok(response.map_body(|_, body| body.boxed()));
            }

            let already_problem = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with(PROBLEM_CONTENT_TYPE));
            if already_problem {
                return Ok(response.map_body(|_, body| body.boxed()));
            }

            let status = response.status();
            let problem = Problem::new(
                status,
                ProblemCode::from_status(status),
                status
                    .canonical_reason()
                    .unwrap_or("Request failed")
                    .to_string(),
                request_id,
            );
            let body = serde_json::to_vec(&problem).unwrap_or_default();
            Ok(response.map_body(move |head, _body| {
                head.headers_mut().insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static(PROBLEM_CONTENT_TYPE),
                );
                head.headers_mut().remove(header::CONTENT_LENGTH);
                BoxBody::new(web::Bytes::from(body))
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::problem::PROBLEM_TYPE_BASE;
    use crate::middleware::request_id::RequestIdMiddleware;
    use actix_web::{App, HttpResponse, http::StatusCode, test, web};
    use serde_json::Value;

    #[actix_web::test]
    async fn converts_a_hub_401_to_problem_details() {
        let app = test::init_service(App::new().wrap(HubProblem).wrap(RequestIdMiddleware).route(
            "/v1/pool/me",
            web::get().to(|| async { HttpResponse::Unauthorized().body("plain text") }),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/v1/pool/me")
            .insert_header(("x-request-id", "req-401"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            PROBLEM_CONTENT_TYPE
        );
        assert_eq!(resp.headers().get("x-request-id").unwrap(), "req-401");
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "unauthorized");
        assert_eq!(body["requestId"], "req-401");
        assert_eq!(body["type"], format!("{PROBLEM_TYPE_BASE}/unauthorized"));
    }

    #[actix_web::test]
    async fn leaves_the_game_api_error_body_alone() {
        let app = test::init_service(App::new().wrap(HubProblem).wrap(RequestIdMiddleware).route(
            "/v1/auth/login",
            web::post().to(|| async {
                HttpResponse::Unauthorized().json(serde_json::json!({"error": "nope"}))
            }),
        ))
        .await;

        let req = test::TestRequest::post().uri("/v1/auth/login").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "nope");
        assert!(body.get("code").is_none());
    }

    #[actix_web::test]
    async fn preserves_an_existing_problem_response() {
        let app = test::init_service(App::new().wrap(HubProblem).wrap(RequestIdMiddleware).route(
            "/v1/api-keys/whoami",
            web::get().to(|| async {
                crate::api::problem::Problem::new(
                    StatusCode::FORBIDDEN,
                    ProblemCode::MissingScope,
                    "specific detail",
                    "req-specific",
                )
                .into_response()
            }),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/v1/api-keys/whoami")
            .to_request();
        let resp = test::call_service(&app, req).await;
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "missing_scope");
        assert_eq!(body["detail"], "specific detail");
    }

    #[actix_web::test]
    async fn hub_path_matching_respects_segments() {
        assert!(is_hub_path("/v1/pool"));
        assert!(is_hub_path("/v1/pool/login"));
        assert!(is_hub_path("/v1/setup/status"));
        assert!(is_hub_path("/v1/api-keys"));
        assert!(is_hub_path("/v1/hosts/abc"));
        assert!(is_hub_path("/v1/connections/env:acme"));
        assert!(!is_hub_path("/v1/poolside"));
        assert!(!is_hub_path("/v1/auth/login"));
        assert!(!is_hub_path("/v1/players/abc"));
    }
}
