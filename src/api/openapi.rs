//! OpenAPI 3.0 document served at `/openapi.json` (task 0.4.10).
//!
//! The document is **generated from a single route table** ([`ROUTES`]) so it
//! cannot silently drift from the implemented endpoints: a unit test asserts
//! every table entry appears in the served JSON, and a CI job scrapes the
//! live `/openapi.json` to validate the exposed spec.

use actix_web::{HttpResponse, web};
use utoipa::openapi::{
    ComponentsBuilder, Content, InfoBuilder, ObjectBuilder, OpenApi, PathItem, PathsBuilder,
    Required, Response, ResponseBuilder, Responses, ResponsesBuilder, Schema, SecurityRequirement,
    Tag, Type,
    extensions::Extensions,
    path::{HttpMethod, Operation, OperationBuilder, Parameter, ParameterBuilder, ParameterIn},
    request_body::{RequestBody, RequestBodyBuilder},
    schema::SchemaType,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

/// One registered HTTP route. This table is the single source of truth the
/// OpenAPI document is generated from.
struct RouteSpec {
    method: HttpMethod,
    path: &'static str,
    tag: &'static str,
    summary: &'static str,
    /// Whether the route requires `Authorization: Bearer <jwt>`.
    secure: bool,
    /// Whether the route accepts a JSON request body.
    has_body: bool,
    /// API-key scope required by this route, when it accepts API keys.
    required_scope: Option<&'static str>,
    /// Success status code, e.g. `"200"`.
    success: &'static str,
}

/// Canonical `/v1` routes plus the un-prefixed system endpoints. Legacy
/// `/auth`, `/players`, ... aliases are the same handlers and are omitted for
/// brevity (the canonical mount is authoritative).
const ROUTES: &[RouteSpec] = &[
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/auth/register",
        tag: "auth",
        summary: "Register a new player and return the public profile",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/auth/login",
        tag: "auth",
        summary: "Verify credentials and issue a signed JWT",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Fetch a player's public profile",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Update the authenticated player's own profile",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Soft-delete the authenticated player's own account",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/players/subaccount",
        tag: "players",
        summary: "Create a subaccount under the authenticated player",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/players/{id}/subaccounts",
        tag: "players",
        summary: "List the authenticated player's subaccounts",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/leaderboards/{game_id}",
        tag: "leaderboards",
        summary: "Fetch one ranked page of a leaderboard",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/leaderboards/{game_id}/entries",
        tag: "leaderboards",
        summary: "Submit a leaderboard entry for the authenticated player",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/leaderboards/{game_id}/entries/{player_id}",
        tag: "leaderboards",
        summary: "Update the authenticated player's own leaderboard entry",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/saves",
        tag: "saves",
        summary: "Create a game save",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/saves/{player_id}",
        tag: "saves",
        summary: "List the authenticated player's saves, newest first",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/saves/{player_id}/{save_id}",
        tag: "saves",
        summary: "Fetch a single game save",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/saves/{player_id}/{save_id}",
        tag: "saves",
        summary: "Delete a single game save",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/games/{game_id}/settings",
        tag: "game settings",
        summary: "Fetch a game's settings",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/games/{game_id}/settings",
        tag: "game settings",
        summary: "Set a game's settings (upsert)",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/events",
        tag: "analytics",
        summary: "Ingest a batch of analytics events (fire-and-forget)",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "202",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/feedback",
        tag: "feedback",
        summary: "Submit player feedback",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/socket-tickets",
        tag: "websocket",
        summary: "Issue a one-shot WebSocket connection ticket",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/setup/status",
        tag: "pool",
        summary: "Report whether the pool still needs first-run setup",
        secure: false,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/setup",
        tag: "pool",
        summary: "Create the organization and first owner from the browser",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/login",
        tag: "pool",
        summary: "Sign an operator in and issue a JWT",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/me",
        tag: "pool",
        summary: "Fetch the signed-in operator and organization",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/password",
        tag: "pool",
        summary: "Replace the operator's password (clears first-sign-in requirement)",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/members",
        tag: "pool",
        summary: "List the organization's operators",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/pool/members/{id}",
        tag: "pool",
        summary: "Change a member's role (owner only; last owner protected)",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/pool/members/{id}",
        tag: "pool",
        summary: "Remove a member (owner only; last owner protected)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/invitations",
        tag: "pool",
        summary: "List pending team invitations (owner only)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/invitations",
        tag: "pool",
        summary: "Invite an operator by email; the token is shown once",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/invitations/accept",
        tag: "pool",
        summary: "Accept an invitation and create the member account",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/pool/invitations/{id}",
        tag: "pool",
        summary: "Revoke a pending invitation (owner only)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/api-keys",
        tag: "pool",
        summary: "List organization API keys (owner only; prefix only)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/api-keys",
        tag: "pool",
        summary: "Create an API key; the full secret is shown once",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/api-keys/whoami",
        tag: "pool",
        summary: "Introspect an API key (requires projects:read)",
        secure: true,
        required_scope: Some("projects:read"),
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/api-keys/{id}",
        tag: "pool",
        summary: "Revoke an API key (owner only; effective immediately)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/executions",
        tag: "pool",
        summary: "Create or replay a create-agent intent with a stable execution id",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/webhooks/github/{slug}",
        tag: "pool",
        summary: "Receive a GitHub App webhook; signature verified and delivery deduplicated",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "202",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/connections",
        tag: "pool",
        summary: "List effective provider connections (environment overlay applied)",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/connections",
        tag: "pool",
        summary: "Create a provider connection from a credential reference",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/connections/{id}",
        tag: "pool",
        summary: "Fetch a connection by id or env:<slug>",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/connections/{id}",
        tag: "pool",
        summary: "Rename, enable or disable a database connection",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/connections/{id}",
        tag: "pool",
        summary: "Delete a database connection",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/execution",
        tag: "pool",
        summary: "Grant host.execute so the host may run agents",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/hosts/{id}/execution",
        tag: "pool",
        summary: "Revoke host.execute; the host becomes connected-only",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts/{id}/capabilities",
        tag: "pool",
        summary: "Read the host's last capability report and offered providers",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/capabilities",
        tag: "pool",
        summary: "Host reports providers, models, modes and thinking options",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/enrollment-tokens",
        tag: "pool",
        summary: "Mint a ten-minute single-use host enrollment token",
        secure: true,
        required_scope: Some("daemons:enroll"),
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/enroll",
        tag: "pool",
        summary: "Exchange an enrollment token for a host relationship credential",
        secure: false,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts",
        tag: "pool",
        summary: "List live hosts in the pool",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts",
        tag: "pool",
        summary: "Register a host; the slug is derived from the hostname",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "201",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts/{id}",
        tag: "pool",
        summary: "Fetch a live host by immutable id",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/hosts/{id}",
        tag: "pool",
        summary: "Rename a host slug or display name; the id is unchanged",
        secure: true,
        required_scope: None,
        has_body: true,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/hosts/{id}",
        tag: "pool",
        summary: "Revoke the pool relationship for a host",
        secure: true,
        required_scope: None,
        has_body: false,
        success: "204",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/healthz",
        tag: "system",
        summary: "Liveness probe with DB status",
        secure: false,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/metrics",
        tag: "system",
        summary: "Prometheus text-format metrics",
        secure: false,
        required_scope: None,
        has_body: false,
        success: "200",
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/openapi.json",
        tag: "system",
        summary: "This OpenAPI document",
        secure: false,
        required_scope: None,
        has_body: false,
        success: "200",
    },
];

/// Register the `/openapi.json` route.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/openapi.json").route(web::get().to(openapi_handler)));
}

/// Serve the generated OpenAPI document as JSON.
async fn openapi_handler() -> HttpResponse {
    match document().to_json() {
        Ok(json) => HttpResponse::Ok()
            .content_type("application/json")
            .body(json),
        Err(e) => {
            tracing::error!(error = %e, "openapi: serialization failed");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": "internal error" }))
        }
    }
}

/// Build the complete OpenAPI document from the route table.
fn document() -> OpenApi {
    let mut paths = PathsBuilder::new();
    for route in ROUTES {
        paths = paths.path(
            route.path,
            PathItem::new(route.method.clone(), build_operation(route)),
        );
    }

    let components = ComponentsBuilder::new()
        .security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "JWT issued by POST /v1/auth/login or POST /v1/pool/login",
                    ))
                    .build(),
            ),
        )
        .build();

    let tags: Vec<Tag> = [
        "auth",
        "players",
        "leaderboards",
        "saves",
        "game settings",
        "analytics",
        "feedback",
        "websocket",
        "pool",
        "system",
    ]
    .into_iter()
    .map(Tag::new)
    .collect();

    let info = InfoBuilder::new()
        .title("Tidepool Server API")
        .version("1.0.0")
        .description(Some(
            "Talo-compatible game backend API (v0.x line). Hub routes (/v1/setup, \
             /v1/pool, /v1/api-keys) return RFC 9457 application/problem+json \
             with code and requestId; codes: unauthorized, forbidden, \
             missing_scope, invalid_request, validation_failed, not_found, \
             conflict, gone, method_not_allowed, rate_limited, \
             daemon_not_connected, configuration_invalid, service_unavailable, \
             internal_error. x-auth and x-required-scope document each hub \
             operation.",
        ))
        .build();

    OpenApi::builder()
        .info(info)
        .paths(paths.build())
        .components(Some(components))
        .tags(Some(tags))
        .build()
}

/// Build one operation for a route.
fn build_operation(route: &RouteSpec) -> Operation {
    let mut builder = OperationBuilder::new()
        .operation_id(Some(format!(
            "{}_{}",
            method_str(route.method.clone()),
            route.path
        )))
        .summary(Some(route.summary))
        .tag(route.tag)
        .responses(build_responses(route));

    if route.secure {
        builder = builder.security(SecurityRequirement::new("bearerAuth", Vec::<String>::new()));
    }
    if route.tag == "pool" {
        let host_authenticated =
            route.method == HttpMethod::Post && route.path == "/v1/hosts/{id}/capabilities";
        let auth = if host_authenticated {
            "host"
        } else if route.required_scope.is_some() {
            "api_key"
        } else if route.secure {
            "member"
        } else {
            "public"
        };
        let mut extensions = Extensions::from_iter([("x-auth", auth)]);
        if let Some(scope) = route.required_scope {
            extensions.insert("x-required-scope".to_string(), serde_json::json!(scope));
        }
        builder = builder.extensions(Some(extensions));
    }

    let params: Vec<Parameter> = path_params(route.path)
        .iter()
        .map(|n| path_parameter(n))
        .collect();
    builder = builder.parameters(Some(params));

    if route.has_body {
        builder = builder.request_body(Some(request_body()));
    }

    builder.build()
}

/// Standard responses for a route: the success status plus the shared error
/// statuses most handlers return.
fn build_responses(route: &RouteSpec) -> Responses {
    let mut builder = ResponsesBuilder::new().response(
        route.success,
        success_response(route.success, route.has_body),
    );
    if route.secure {
        builder = builder
            .response(
                "401",
                err_response("Unauthorized — missing, invalid or expired JWT"),
            )
            .response(
                "503",
                err_response("Service unavailable — database not configured"),
            );
    }
    builder
        .response(
            "400",
            err_response("Bad request — validation or malformed input"),
        )
        .response(
            "429",
            err_response("Too many requests — rate limit exceeded"),
        )
        .build()
}

/// JSON response for a success status.
fn success_response(code: &str, with_body: bool) -> Response {
    let mut response = ResponseBuilder::new()
        .description(success_description(code))
        .build();
    if with_body && code != "204" {
        response = ResponseBuilder::new()
            .description(success_description(code))
            .content("application/json", json_content())
            .build();
    }
    response
}

/// JSON `{"error": ...}` body response.
fn err_response(description: &str) -> Response {
    ResponseBuilder::new()
        .description(description)
        .content("application/json", json_content())
        .build()
}

/// Loose JSON body content — the shape is documented per-endpoint in the
/// route handlers and `docs/TALO_API.md`.
fn json_content() -> Content {
    Content::new(Some(Schema::Object(ObjectBuilder::new().build())))
}

/// Description text for a success status code.
fn success_description(code: &str) -> &'static str {
    match code {
        "200" => "OK",
        "201" => "Created",
        "202" => "Accepted",
        "204" => "No Content",
        _ => "Success",
    }
}

/// Lower-case HTTP method name for `operationId`.
fn method_str(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
        HttpMethod::Head => "head",
        HttpMethod::Options => "options",
        HttpMethod::Trace => "trace",
    }
}

/// `{...}` path template placeholders in `path`.
fn path_params(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| seg.trim_matches(['{', '}']).to_string())
        .collect()
}

/// Required path parameter of string type.
fn path_parameter(name: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .schema(Some(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Type(Type::String))
                .build(),
        )))
        .build()
}

/// OpenAPI request body object (JSON).
fn request_body() -> RequestBody {
    RequestBodyBuilder::new()
        .content("application/json", json_content())
        .required(Some(Required::True))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_operation(item: &PathItem, method: HttpMethod) -> bool {
        match method {
            HttpMethod::Get => item.get.is_some(),
            HttpMethod::Post => item.post.is_some(),
            HttpMethod::Put => item.put.is_some(),
            HttpMethod::Patch => item.patch.is_some(),
            HttpMethod::Delete => item.delete.is_some(),
            _ => false,
        }
    }

    #[test]
    fn document_lists_every_route() {
        let doc = document();
        for route in ROUTES {
            let item = doc
                .paths
                .paths
                .get(route.path)
                .unwrap_or_else(|| panic!("missing path {}", route.path));
            assert!(
                has_operation(item, route.method.clone()),
                "{} missing {}",
                route.path,
                method_str(route.method.clone())
            );
        }
    }

    /// The game API is frozen: the rename must not move a route. Pin the exact
    /// inventory so a changed prefix, a dropped route or an added system route
    /// fails here before it reaches a client.
    #[test]
    fn game_routes_are_unmoved_under_v1() {
        const SYSTEM_PATHS: &[&str] = &["/healthz", "/metrics", "/openapi.json"];

        assert_eq!(ROUTES.len(), 56, "ROUTES must describe all 56 routes");
        assert_eq!(
            ROUTES
                .iter()
                .filter(|route| route.path.starts_with("/v1/"))
                .count(),
            53,
            "the 19 game route specs and 34 pool/host/connection routes must stay under /v1/"
        );

        let system: std::collections::BTreeSet<&str> = ROUTES
            .iter()
            .map(|route| route.path)
            .filter(|path| !path.starts_with("/v1/"))
            .collect();
        let expected: std::collections::BTreeSet<&str> = SYSTEM_PATHS.iter().copied().collect();
        assert_eq!(
            system, expected,
            "only the three system routes may sit outside /v1/"
        );
    }

    #[test]
    fn document_has_expected_meta() {
        let doc = document();
        assert_eq!(doc.info.title, "Tidepool Server API");
        assert!(doc.paths.paths.contains_key("/v1/auth/register"));
        assert!(doc.paths.paths.contains_key("/metrics"));
        assert!(doc.paths.paths.contains_key("/openapi.json"));
        let components = doc.components.expect("components present");
        assert!(components.security_schemes.contains_key("bearerAuth"));
    }

    #[test]
    fn document_is_valid_json_with_expected_paths() {
        let doc = document();
        let json = doc.to_json().expect("serializes");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["openapi"], "3.1.0");
        for route in ROUTES {
            assert!(
                value["paths"].get(route.path).is_some(),
                "missing {} in JSON",
                route.path
            );
        }
    }

    /// Every hub operation documents how it is authorized, and API-key routes
    /// document the required scope. The CI scrape asserts the same on the
    /// served document.
    #[test]
    fn hub_routes_document_access_and_required_scope() {
        let doc = document();
        for route in ROUTES.iter().filter(|route| route.tag == "pool") {
            let item = doc
                .paths
                .paths
                .get(route.path)
                .unwrap_or_else(|| panic!("missing path {}", route.path));
            let operation = match route.method {
                HttpMethod::Get => item.get.as_ref(),
                HttpMethod::Post => item.post.as_ref(),
                HttpMethod::Put => item.put.as_ref(),
                HttpMethod::Patch => item.patch.as_ref(),
                HttpMethod::Delete => item.delete.as_ref(),
                _ => None,
            }
            .unwrap_or_else(|| panic!("missing operation {}", route.path));
            let extensions = operation
                .extensions
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no access extension", route.path));
            let host_authenticated =
                route.method == HttpMethod::Post && route.path == "/v1/hosts/{id}/capabilities";
            let expected = if host_authenticated {
                "host"
            } else if route.required_scope.is_some() {
                "api_key"
            } else if route.secure {
                "member"
            } else {
                "public"
            };
            assert_eq!(
                extensions.get("x-auth").and_then(|value| value.as_str()),
                Some(expected),
                "{} access",
                route.path
            );
            match route.required_scope {
                Some(scope) => assert_eq!(
                    extensions
                        .get("x-required-scope")
                        .and_then(|value| value.as_str()),
                    Some(scope),
                    "{} scope",
                    route.path
                ),
                None => assert!(
                    !extensions.contains_key("x-required-scope"),
                    "{} must not claim a scope",
                    route.path
                ),
            }
        }
        assert_eq!(
            ROUTES
                .iter()
                .filter(|route| route.required_scope.is_some())
                .count(),
            2
        );

        // Extensions are flattened onto the operation in the served JSON, as
        // the CI scrape asserts.
        let json: serde_json::Value =
            serde_json::from_str(&document().to_json().expect("json")).expect("valid json");
        assert_eq!(
            json["paths"]["/v1/api-keys/whoami"]["get"]["x-required-scope"],
            "projects:read"
        );
        assert_eq!(json["paths"]["/v1/pool/members"]["get"]["x-auth"], "member");
        assert_eq!(json["paths"]["/v1/setup/status"]["get"]["x-auth"], "public");
    }

    /// Parse the hub modules' `web::scope(...).route(...)` registrations and
    /// require the OpenAPI table to describe exactly the same inventory. This
    /// fails when a hub route is added to code without a table entry, and when
    /// a table entry is removed while the route remains registered.
    #[test]
    fn every_registered_hub_route_is_in_the_table() {
        fn parse_scope(source: &str) -> std::collections::BTreeSet<(String, String)> {
            let mut routes = std::collections::BTreeSet::new();
            let mut scope: Option<String> = None;
            let mut pending_route: Option<String> = None;
            let mut pending_path_line = false;
            for line in source.lines() {
                let line = line.trim();
                if let Some(rest) = line.split_once("web::scope(\"")
                    && let Some((path, _)) = rest.1.split_once('"')
                {
                    scope = Some(path.to_string());
                }
                let Some(scope_path) = scope.as_ref() else {
                    continue;
                };
                if pending_route.is_none() {
                    if let Some(rest) = line.split_once(".route(\"")
                        && let Some((route_path, _)) = rest.1.split_once('"')
                    {
                        pending_route = Some(route_path.to_string());
                        pending_path_line = false;
                    } else if line.contains(".route(") {
                        pending_path_line = true;
                    } else if pending_path_line
                        && let Some((route_path, _)) = line.trim_start_matches('"').split_once('"')
                    {
                        pending_route = Some(route_path.to_string());
                        pending_path_line = false;
                    }
                }
                let Some(route_path) = pending_route.clone() else {
                    continue;
                };
                // The route and its method may be on the same line or split
                // across lines by rustfmt.
                let Some(method) = ["get", "post", "put", "patch", "delete"]
                    .into_iter()
                    .find(|method| line.contains(&format!("web::{method}()")))
                else {
                    continue;
                };
                let full = if route_path.is_empty() {
                    scope_path.clone()
                } else {
                    format!("{scope_path}{route_path}")
                };
                routes.insert((method.to_string(), full));
                pending_route = None;
            }
            routes
        }

        let mut registered = parse_scope(include_str!("pool_identity.rs"));
        registered.extend(parse_scope(include_str!("api_keys.rs")));
        registered.extend(parse_scope(include_str!("hosts.rs")));
        registered.extend(parse_scope(include_str!("connections.rs")));
        registered.extend(parse_scope(include_str!("webhooks/github.rs")));

        let documented: std::collections::BTreeSet<(String, String)> = ROUTES
            .iter()
            .filter(|route| route.tag == "pool")
            .map(|route| {
                (
                    method_str(route.method.clone()).to_string(),
                    route.path.to_string(),
                )
            })
            .collect();

        assert_eq!(
            registered, documented,
            "hub routes registered in code and documented in ROUTES must match"
        );
        assert_eq!(registered.len(), 34, "expected 34 hub operations");
    }
}
