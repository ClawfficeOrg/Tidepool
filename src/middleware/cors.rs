//! Configurable CORS for browser callers (`TIDEPOOL_CORS_ORIGINS`).
//!
//! The control plane can be served from its own hostname and call the pool from
//! a browser. It authenticates with a Bearer token, not cookies, so credentials
//! mode is never enabled.
//!
//! - With no origins configured the middleware is a pass-through.
//! - A request whose `Origin` is in the allowlist gets that exact origin echoed
//!   in `Access-Control-Allow-Origin`, `Vary: Origin`, and
//!   `Access-Control-Expose-Headers` for the request-id header.
//! - A preflight (`OPTIONS` with `Access-Control-Request-Method`) from an
//!   allowed origin is answered `204` right here. The middleware is registered
//!   outermost, so the preflight never reaches rate limiting or the auth
//!   extractors, which a browser's credential-less preflight could not satisfy.
//! - Any other origin is untouched: no CORS headers, no behaviour change.
//!   Origins are matched exactly and never reflected.

use actix_web::{
    HttpRequest, HttpResponse,
    body::{BoxBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::{
        Method, StatusCode,
        header::{self, HeaderValue},
    },
    middleware::Next,
    web,
};

/// The request-id header the server returns and accepts.
const REQUEST_ID: &str = "x-request-id";
/// Methods advertised to preflights.
const ALLOW_METHODS: &str = "GET, POST, PATCH, PUT, DELETE, OPTIONS";
/// Headers advertised to preflights.
const ALLOW_HEADERS: &str = "Authorization, Content-Type, x-request-id";
/// How long a browser may cache a preflight, in seconds.
const MAX_AGE: &str = "600";

/// The validated allowlist, registered as app data.
#[derive(Debug, Clone, Default)]
pub struct CorsOrigins(pub Vec<String>);

impl CorsOrigins {
    /// True when `origin` exactly matches a configured entry.
    pub fn allows(&self, origin: &str) -> bool {
        self.0.iter().any(|allowed| allowed == origin)
    }
}

/// The request's `Origin` header, when it is valid UTF-8.
fn origin_of(req: &HttpRequest) -> Option<&str> {
    req.headers().get(header::ORIGIN)?.to_str().ok()
}

/// The allowed origin of this request, if any.
fn allowed_origin(req: &HttpRequest) -> Option<String> {
    let origins = req.app_data::<web::Data<CorsOrigins>>()?;
    let origin = origin_of(req)?;
    origins.allows(origin).then(|| origin.to_string())
}

/// Whether a browser WebSocket upgrade may proceed for this request.
///
/// With no allowlist configured nothing is checked. Otherwise a request with an
/// `Origin` header must carry an allowlisted origin, the pool's own public
/// origin, or an origin whose authority equals the request `Host` (the pool
/// serving its own bundle). A request with no `Origin`, as non-browser clients
/// send, is not a browser cross-origin call and is not checked.
pub fn websocket_origin_permitted(
    req: &HttpRequest,
    cors: &[String],
    public_origin: Option<&str>,
) -> bool {
    if cors.is_empty() {
        return true;
    }
    let Some(origin) = origin_of(req) else {
        return req.headers().get(header::ORIGIN).is_none();
    };
    if cors.iter().any(|allowed| allowed == origin) || public_origin == Some(origin) {
        return true;
    }
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    match (host, origin.split_once("://")) {
        (Some(host), Some((_, authority))) => authority == host,
        _ => false,
    }
}

fn expose(headers: &mut header::HeaderMap, origin: &str) {
    if let Ok(value) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static(REQUEST_ID),
    );
}

/// CORS middleware; wrap with `actix_web::middleware::from_fn(cors)`.
pub async fn cors(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<BoxBody>, actix_web::Error> {
    let Some(origin) = allowed_origin(req.request()) else {
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    };

    let is_preflight = req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    if is_preflight {
        let mut res = HttpResponse::build(StatusCode::NO_CONTENT).finish();
        let headers = res.headers_mut();
        expose(headers, &origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static(ALLOW_METHODS),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(ALLOW_HEADERS),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(MAX_AGE),
        );
        return Ok(req.into_response(res));
    }

    let mut res = next.call(req).await?.map_into_boxed_body();
    expose(res.headers_mut(), &origin);
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::{problem::HubProblem, request_id::RequestIdMiddleware};
    use actix_web::{App, middleware::from_fn, test};

    const ALLOWED: &str = "https://control.example.com";

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().body("ok")
    }

    async fn unauthorized() -> HttpResponse {
        HttpResponse::Unauthorized().finish()
    }

    /// Mirrors the production wrap order: CORS outermost.
    macro_rules! app {
        ($origins:expr) => {
            test::init_service(
                App::new()
                    .wrap(HubProblem)
                    .wrap(RequestIdMiddleware)
                    .wrap(from_fn(cors))
                    .app_data(web::Data::new(CorsOrigins($origins)))
                    .route("/v1/thing", web::get().to(ok))
                    .route("/v1/secure", web::get().to(unauthorized)),
            )
            .await
        };
    }

    fn get(origin: Option<&str>) -> test::TestRequest {
        let req = test::TestRequest::get().uri("/v1/thing");
        match origin {
            Some(o) => req.insert_header((header::ORIGIN, o)),
            None => req,
        }
    }

    fn has_cors_headers(res: &ServiceResponse<BoxBody>) -> bool {
        res.headers()
            .keys()
            .any(|name| name.as_str().starts_with("access-control-"))
    }

    #[actix_web::test]
    async fn an_allowed_origin_gets_the_exact_origin_vary_and_expose() {
        let app = app!(vec![
            ALLOWED.to_string(),
            "http://localhost:5173".to_string()
        ]);
        let res = test::call_service(&app, get(Some(ALLOWED)).to_request()).await;
        assert_eq!(res.status(), StatusCode::OK);
        let h = res.headers();
        assert_eq!(h.get("access-control-allow-origin").unwrap(), ALLOWED);
        assert_eq!(h.get("vary").unwrap(), "Origin");
        assert_eq!(
            h.get("access-control-expose-headers").unwrap(),
            "x-request-id"
        );
        assert!(h.contains_key("x-request-id"));
        assert!(!h.contains_key("access-control-allow-credentials"));
    }

    #[actix_web::test]
    async fn error_responses_from_an_allowed_origin_carry_the_headers() {
        let app = app!(vec![ALLOWED.to_string()]);
        let req = test::TestRequest::get()
            .uri("/v1/secure")
            .insert_header((header::ORIGIN, ALLOWED))
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            ALLOWED
        );
    }

    #[actix_web::test]
    async fn a_disallowed_origin_gets_no_cors_headers() {
        let app = app!(vec![ALLOWED.to_string()]);
        for origin in [
            "https://evil.example.com",
            "https://control.example.com:8443",
            "http://control.example.com",
            "https://control.example.com/",
        ] {
            let res = test::call_service(&app, get(Some(origin)).to_request()).await;
            assert_eq!(res.status(), StatusCode::OK);
            assert!(!has_cors_headers(&res), "{origin} must get no CORS headers");
            assert!(!res.headers().contains_key("vary"));
        }
        let res = test::call_service(&app, get(None).to_request()).await;
        assert!(!has_cors_headers(&res));
    }

    #[actix_web::test]
    async fn nothing_is_added_when_unset() {
        let app = app!(Vec::new());
        let res = test::call_service(&app, get(Some(ALLOWED)).to_request()).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(!has_cors_headers(&res));
        let req = test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri("/v1/thing")
            .insert_header((header::ORIGIN, ALLOWED))
            .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_ne!(res.status(), StatusCode::NO_CONTENT);
        assert!(!has_cors_headers(&res));
    }

    fn preflight(origin: &str, uri: &str) -> test::TestRequest {
        test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri(uri)
            .insert_header((header::ORIGIN, origin))
            .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "POST"))
            .insert_header((
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "authorization,content-type",
            ))
    }

    #[actix_web::test]
    async fn a_preflight_is_answered_204_before_any_route_runs() {
        let app = app!(vec![ALLOWED.to_string()]);
        // `/v1/secure` answers 401 and has no OPTIONS route; `/nowhere` has no
        // route at all. Both must be answered by the CORS layer alone.
        for uri in ["/v1/secure", "/v1/nowhere"] {
            let res = test::call_service(&app, preflight(ALLOWED, uri).to_request()).await;
            assert_eq!(res.status(), StatusCode::NO_CONTENT, "{uri}");
            let h = res.headers();
            assert_eq!(h.get("access-control-allow-origin").unwrap(), ALLOWED);
            assert_eq!(h.get("vary").unwrap(), "Origin");
            assert_eq!(
                h.get("access-control-allow-methods").unwrap(),
                "GET, POST, PATCH, PUT, DELETE, OPTIONS"
            );
            assert_eq!(
                h.get("access-control-allow-headers").unwrap(),
                "Authorization, Content-Type, x-request-id"
            );
            assert_eq!(h.get("access-control-max-age").unwrap(), "600");
            assert_eq!(
                h.get("access-control-expose-headers").unwrap(),
                "x-request-id"
            );
        }
    }

    #[actix_web::test]
    async fn a_preflight_from_a_disallowed_origin_is_not_answered() {
        let app = app!(vec![ALLOWED.to_string()]);
        let res = test::call_service(
            &app,
            preflight("https://evil.example.com", "/v1/secure").to_request(),
        )
        .await;
        assert_ne!(res.status(), StatusCode::NO_CONTENT);
        assert!(!has_cors_headers(&res));
    }

    fn ws_req(origin: Option<&str>, host: &str) -> HttpRequest {
        let req = test::TestRequest::get()
            .uri("/v1/chatrooms/r/stream")
            .insert_header((header::HOST, host));
        match origin {
            Some(o) => req.insert_header((header::ORIGIN, o)),
            None => req,
        }
        .to_http_request()
    }

    #[actix_web::test]
    async fn websocket_origin_checks_the_same_allowlist() {
        let cors = vec![ALLOWED.to_string()];
        let pool = Some("https://pool.example.com");
        let host = "pool.example.com";
        // Unset: nothing is checked.
        assert!(websocket_origin_permitted(
            &ws_req(Some("https://evil.example.com"), host),
            &[],
            pool
        ));
        assert!(websocket_origin_permitted(
            &ws_req(Some(ALLOWED), host),
            &cors,
            pool
        ));
        assert!(websocket_origin_permitted(&ws_req(None, host), &cors, pool));
        assert!(websocket_origin_permitted(
            &ws_req(Some("https://pool.example.com"), host),
            &cors,
            pool
        ));
        assert!(websocket_origin_permitted(
            &ws_req(Some("http://127.0.0.1:8080"), "127.0.0.1:8080"),
            &cors,
            None
        ));
        assert!(!websocket_origin_permitted(
            &ws_req(Some("https://evil.example.com"), host),
            &cors,
            pool
        ));
    }
}
