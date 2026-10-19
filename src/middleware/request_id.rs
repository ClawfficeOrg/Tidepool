//! Request-id middleware (task 1.1.4).
//!
//! Every request gets a correlation id before it reaches a handler:
//!
//! - an incoming `x-request-id` is reused when it is 1–128 characters of
//!   `[A-Za-z0-9._-]`;
//! - otherwise a UUID v4 is generated.
//!
//! The id is stored in request extensions as [`RequestId`] and echoed on the
//! response as `x-request-id`. Hub problem documents carry the same value.

use actix_web::{
    Error, HttpMessage, HttpRequest, HttpResponse,
    body::MessageBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

/// Correlation id for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

impl RequestId {
    /// Read the id inserted by [`RequestIdMiddleware`].
    ///
    /// Falls back to the incoming header, then to a fresh UUID, so unit tests
    /// that mount a handler without the middleware still get a value.
    pub fn from_request(req: &HttpRequest) -> String {
        if let Some(id) = req.extensions().get::<RequestId>() {
            return id.0.clone();
        }
        incoming_header(req).unwrap_or_else(new_id)
    }
}

/// Transform factory for the request-id middleware.
#[derive(Default)]
pub struct RequestIdMiddleware;

impl<S, B> Transform<S, ServiceRequest> for RequestIdMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestIdService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestIdService { service }))
    }
}

/// Per-worker request-id service.
pub struct RequestIdService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for RequestIdService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let id = incoming_header(req.request()).unwrap_or_else(new_id);
        req.extensions_mut().insert(RequestId(id.clone()));

        let fut = self.service.call(req);
        Box::pin(async move {
            let mut response = fut.await?;
            if let Ok(value) = id.parse() {
                response.headers_mut().insert(
                    actix_web::http::header::HeaderName::from_static("x-request-id"),
                    value,
                );
            }
            Ok(response)
        })
    }
}

fn incoming_header(req: &HttpRequest) -> Option<String> {
    let value = req
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())?
        .trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return None;
    }
    Some(value.to_string())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Build an empty response with the id echoed; used by tests.
#[allow(dead_code)]
pub async fn echo(req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().body(RequestId::from_request(&req))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test, web};

    #[actix_web::test]
    async fn generates_and_echoes_an_id() {
        let app = test::init_service(
            App::new()
                .wrap(RequestIdMiddleware)
                .route("/", web::get().to(echo)),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let header = resp
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("request id header")
            .to_string();
        assert!(!header.is_empty());
        let body = test::read_body(resp).await;
        assert_eq!(String::from_utf8_lossy(&body), header);
    }

    #[actix_web::test]
    async fn reuses_a_valid_incoming_id() {
        let app = test::init_service(
            App::new()
                .wrap(RequestIdMiddleware)
                .route("/", web::get().to(echo)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("x-request-id", "trace-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.headers().get("x-request-id").unwrap(), "trace-123");
    }

    #[actix_web::test]
    async fn rejects_an_unsafe_incoming_id() {
        let app = test::init_service(
            App::new()
                .wrap(RequestIdMiddleware)
                .route("/", web::get().to(echo)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("x-request-id", "bad id with spaces"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let header = resp
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("generated id");
        assert_ne!(header, "bad id with spaces");
        assert_eq!(header.len(), 36);
    }
}
