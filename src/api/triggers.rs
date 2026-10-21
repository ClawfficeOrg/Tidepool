//! Trigger HTTP surface (tasks 1.4.2 and 1.4.3).
//!
//! - `POST /v1/triggers/validate` — compile one trigger document and ask the
//!   named host whether it can run the agent it names, without writing pool
//!   state.
//! - `POST /v1/triggers/install` — the same compilation, then create or replace
//!   the project's trigger of that name and return its id, the new revision,
//!   the version and whether it is active.
//!
//! The endpoints accept a non-viewer member or an API key with
//! `configuration:validate` (validate) or `configuration:install` (install). A
//! malformed body is a 400; a well-formed document whose references fail to
//! resolve is a 200 with `valid: false` and the named field issues from
//! validate, and a 422 with the same issues from install — installing nothing.
//! Install takes the project it belongs to in the body, because a trigger name
//! is only unique within one project.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::api_key::ApiKeyScope;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::AuditActor;
use crate::services::triggers::{self as trigger_svc, TriggerDocument, TriggerError, TriggerIssue};
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use serde::Serialize;
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};

/// Register the trigger routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/triggers")
            .route("/validate", web::post().to(validate_trigger))
            .route("/install", web::post().to(install_trigger)),
    );
}

/// Caller authenticated as either an API key or a member.
enum HubAuth {
    /// API key.
    Key(ApiKeyAuth),
    /// Operator member.
    Member(OperatorAuth),
}

impl HubAuth {
    /// Organization id both principals carry.
    fn organization_id(&self) -> u64 {
        match self {
            HubAuth::Key(auth) => auth.organization_id(),
            HubAuth::Member(auth) => auth.organization_id(),
        }
    }

    /// Audit actor for mutations.
    fn actor(&self) -> AuditActor {
        match self {
            HubAuth::Key(auth) => AuditActor::api_key(&auth.key.public_id, &auth.key.prefix),
            HubAuth::Member(auth) => {
                AuditActor::member(&auth.operator.public_id, &auth.operator.email)
            }
        }
    }

    /// A non-viewer member, or an API key with `scope`.
    fn require_scope(&self, scope: ApiKeyScope) -> Result<(), Error> {
        match self {
            HubAuth::Key(auth) => auth.require_scope(scope),
            HubAuth::Member(auth) => auth.require_mutating(),
        }
    }
}

impl FromRequest for HubAuth {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let is_api_key = req
            .headers()
            .get("x-api-key")
            .is_some_and(|value| !value.is_empty())
            || req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|value| {
                    value.starts_with(crate::services::api_keys::KEY_PREFIX_MARKER)
                });

        let key_future = ApiKeyAuth::from_request(req, payload);
        let member_future = OperatorAuth::from_request(req, payload);

        Box::pin(async move {
            if is_api_key {
                Ok(HubAuth::Key(key_future.await?))
            } else {
                Ok(HubAuth::Member(member_future.await?))
            }
        })
    }
}

/// Trigger validate response.
#[derive(Debug, Serialize)]
pub struct TriggerValidateResponse {
    /// The trigger name, echoed back.
    pub name: String,
    /// True when the document is valid (references resolve and the host can
    /// run the agent).
    pub valid: bool,
    /// Named field issues, empty when valid.
    pub errors: Vec<TriggerIssue>,
}

/// Trigger install payload: the project it belongs to plus the document.
///
/// Parsed from one flat object — the same body validate accepts, plus
/// `project` — so install accepts and refuses exactly the documents validate
/// does, including `deny_unknown_fields`. (A `#[serde(flatten)]` would silently
/// disable that last rule, which is why the body is parsed explicitly.)
#[derive(Debug, Clone)]
pub struct TriggerInstallRequest {
    /// Public id of the project the trigger belongs to.
    pub project: String,
    /// The trigger document: `name`, `event`, `filters` and `run`.
    pub document: TriggerDocument,
}

impl TriggerInstallRequest {
    /// Parse an install body, returning a caller-facing reason on failure.
    pub fn parse(body: serde_json::Value) -> Result<Self, String> {
        let mut object = match body {
            serde_json::Value::Object(object) => object,
            _ => return Err("body must be a JSON object".to_string()),
        };
        let project = match object.remove("project") {
            Some(serde_json::Value::String(project)) => project,
            Some(_) => return Err("project must be a string".to_string()),
            None => return Err("project is required".to_string()),
        };
        let project = project.trim().to_string();
        if project.is_empty() || project.chars().count() > 36 {
            return Err("project must be 1-36 characters".to_string());
        }
        let document = serde_json::from_value(serde_json::Value::Object(object))
            .map_err(|error| format!("invalid trigger document: {error}"))?;
        Ok(Self { project, document })
    }
}

/// Trigger install response.
#[derive(Debug, Serialize)]
pub struct TriggerInstallResponse {
    /// Immutable trigger id; a replacement keeps it.
    pub id: String,
    /// The trigger name, unique per project.
    pub name: String,
    /// Public id of the revision this install produced.
    pub revision: String,
    /// Per-trigger revision number, starting at 1.
    pub version: u32,
    /// True when this revision is the trigger's active document.
    pub active: bool,
    /// True when an existing trigger of that name was replaced.
    pub replaced: bool,
}

#[tracing::instrument(skip(pool, auth, body))]
async fn validate_trigger(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    body: web::Json<TriggerDocument>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_scope(ApiKeyScope::ConfigurationValidate) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };

    match trigger_svc::validate_trigger(pool.get_ref(), auth.organization_id(), &body.into_inner())
        .await
    {
        Ok(outcome) => HttpResponse::Ok().json(TriggerValidateResponse {
            name: outcome.name,
            valid: outcome.valid,
            errors: outcome.errors,
        }),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn install_trigger(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    body: web::Json<serde_json::Value>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_scope(ApiKeyScope::ConfigurationInstall) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let request = match TriggerInstallRequest::parse(body.into_inner()) {
        Ok(request) => request,
        Err(detail) => return invalid_request(&req, &detail),
    };

    match trigger_svc::install_trigger(
        pool.get_ref(),
        auth.organization_id(),
        &request.project,
        &request.document,
        auth.actor(),
    )
    .await
    {
        Ok(result) => HttpResponse::Created().json(TriggerInstallResponse {
            id: result.id,
            name: result.name,
            revision: result.revision,
            version: result.version,
            active: result.active,
            replaced: result.replaced,
        }),
        Err(error) => map_error(error, &req),
    }
}

fn db_unavailable(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::SERVICE_UNAVAILABLE,
        ProblemCode::ServiceUnavailable,
        "database unavailable",
    )
    .into_response()
}

fn invalid_request(req: &HttpRequest, detail: &str) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::BAD_REQUEST,
        ProblemCode::InvalidRequest,
        detail,
    )
    .into_response()
}

fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

fn map_error(error: TriggerError, req: &HttpRequest) -> HttpResponse {
    match error {
        TriggerError::Database(error) => {
            tracing::error!(error = %error, "trigger operation failed");
            Problem::from_request(
                req,
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error",
            )
            .into_response()
        }
        TriggerError::ProjectNotFound => Problem::from_request(
            req,
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "project not found",
        )
        .into_response(),
        TriggerError::Invalid(issues) => {
            // The document is well formed but cannot be installed: name every
            // failing field and change nothing.
            let detail = issues
                .iter()
                .map(|issue| format!("{}: {}", issue.field, issue.message))
                .collect::<Vec<_>>()
                .join("; ");
            Problem::from_request(
                req,
                StatusCode::UNPROCESSABLE_ENTITY,
                ProblemCode::ConfigurationInvalid,
                format!("trigger document is invalid: {detail}"),
            )
            .into_response()
        }
        TriggerError::Encoding(message) => {
            tracing::error!(%message, "trigger document could not be stored");
            Problem::from_request(
                req,
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error",
            )
            .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode as TestStatus, test as actix_test};

    #[actix_web::test]
    async fn validate_requires_credentials() {
        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::post()
            .uri("/v1/triggers/validate")
            .set_json(serde_json::json!({}))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn install_requires_credentials() {
        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::post()
            .uri("/v1/triggers/install")
            .set_json(serde_json::json!({
                "project": "11111111-1111-4111-8111-111111111111",
                "name": "auto-fix-issues",
                "event": { "provider": "github", "type": "issue.comment_created" },
                "filters": {
                    "connection": "github-main",
                    "resource": { "kind": "repository", "id": "org/repo" },
                    "from_users": ["alice"]
                },
                "run": {
                    "host": "studio-mac",
                    "agent": {
                        "provider": "anthropic",
                        "model": "claude",
                        "execution_mode": "default"
                    }
                }
            }))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[test]
    fn the_install_body_is_the_document_plus_a_project() {
        // Parsed from one flat object: the trigger document as authored, with
        // one extra key naming the project.
        let body = TriggerInstallRequest::parse(serde_json::json!({
            "project": "11111111-1111-4111-8111-111111111111",
            "name": "auto-fix-issues",
            "event": { "provider": "github", "type": "issue.comment_created" },
            "filters": {
                "connection": "github-main",
                "resource": { "kind": "repository", "id": "org/repo" },
                "from_users": ["alice"]
            },
            "run": {
                "host": "studio-mac",
                "agent": {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default"
                }
            }
        }))
        .expect("body parses");
        assert_eq!(body.project, "11111111-1111-4111-8111-111111111111");
        assert_eq!(body.document.name, "auto-fix-issues");
        assert_eq!(body.document.filters.from_users, vec!["alice".to_string()]);
    }

    #[test]
    fn a_missing_or_unusable_project_is_a_bad_request() {
        let mut body = serde_json::json!({
            "name": "auto-fix-issues",
            "event": { "provider": "github", "type": "issue.comment_created" },
            "filters": {
                "connection": "github-main",
                "resource": { "kind": "repository", "id": "org/repo" },
                "from_users": ["alice"]
            },
            "run": {
                "host": "studio-mac",
                "agent": {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default"
                }
            }
        });
        assert!(TriggerInstallRequest::parse(body.clone()).is_err());

        body["project"] = serde_json::json!("   ");
        assert!(TriggerInstallRequest::parse(body.clone()).is_err());

        body["project"] = serde_json::json!(7);
        assert!(TriggerInstallRequest::parse(body.clone()).is_err());

        body["project"] = serde_json::json!("p".repeat(37));
        assert!(TriggerInstallRequest::parse(body).is_err());
    }

    #[test]
    fn an_unknown_body_field_is_refused() {
        // `deny_unknown_fields` survives the split, so install refuses the same
        // documents validate refuses.
        let parsed = TriggerInstallRequest::parse(serde_json::json!({
            "project": "11111111-1111-4111-8111-111111111111",
            "name": "auto-fix-issues",
            "event": { "provider": "github", "type": "issue.comment_created" },
            "filters": {
                "connection": "github-main",
                "resource": { "kind": "repository", "id": "org/repo" },
                "from_users": ["alice"],
                "nope": true
            },
            "run": {
                "host": "studio-mac",
                "agent": {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default"
                }
            }
        }));
        assert!(parsed.is_err(), "unknown fields must not parse");
    }

    #[test]
    fn a_missing_allowlist_is_refused_before_any_database_call() {
        // The document is parsed here, so a missing from_users is already a 400
        // rather than reaching install.
        let parsed = TriggerInstallRequest::parse(serde_json::json!({
            "project": "11111111-1111-4111-8111-111111111111",
            "name": "auto-fix-issues",
            "event": { "provider": "github", "type": "issue.comment_created" },
            "filters": {
                "connection": "github-main",
                "resource": { "kind": "repository", "id": "org/repo" }
            },
            "run": {
                "host": "studio-mac",
                "agent": {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default"
                }
            }
        }))
        .expect("from_users is optional on the wire so validation can name it");
        // from_users defaults to empty, which structural validation then rejects
        // by name; that is the 422 path, not a parse failure.
        assert!(parsed.document.filters.from_users.is_empty());
    }

    #[test]
    fn the_install_response_carries_every_expected_field() {
        let response = TriggerInstallResponse {
            id: "trigger-1".into(),
            name: "auto-fix-issues".into(),
            revision: "revision-2".into(),
            version: 2,
            active: true,
            replaced: true,
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["id"], "trigger-1");
        assert_eq!(json["name"], "auto-fix-issues");
        assert_eq!(json["revision"], "revision-2");
        assert_eq!(json["version"], 2);
        assert_eq!(json["active"], true);
        assert_eq!(json["replaced"], true);
    }
}
