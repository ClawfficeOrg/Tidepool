//! Connection registry HTTP surface (task 1.3.1).
//!
//! - `GET    /v1/connections`
//! - `POST   /v1/connections`
//! - `GET    /v1/connections/{id}` (database id or `env:<slug>`)
//! - `PATCH  /v1/connections/{id}` (slug, display name, active/disabled)
//! - `DELETE /v1/connections/{id}`
//!
//! Reads never return credentials or the credential reference. Environment
//! connections win on slug, are labelled `managed_by: environment`, and cannot
//! be mutated here.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::connection::{ConnectionStatus, ConnectionView, ProviderKind};
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::AuditActor;
use crate::services::connections::{
    self as connection_svc, ConnectionError, NewConnection, NewResource, UpdateConnection,
};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use validator::Validate;

/// Register connection routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/connections")
            .route("", web::get().to(list_connections))
            .route("", web::post().to(create_connection))
            .route("/{id}", web::get().to(get_connection))
            .route("/{id}", web::patch().to(update_connection))
            .route("/{id}", web::delete().to(delete_connection)),
    );
}

/// List response.
#[derive(Debug, Serialize)]
pub struct ConnectionsResponse {
    /// Effective connections (environment overlay applied).
    pub connections: Vec<ConnectionView>,
}

/// Resource input.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequest {
    /// `repository`, `workspace`, `guild`, `channel`, ...
    #[validate(length(min = 1, max = 32))]
    pub resource_kind: String,
    /// Provider-side id.
    #[validate(length(min = 1, max = 255))]
    pub external_id: String,
    /// Optional human label.
    #[validate(length(min = 1, max = 128))]
    pub display_name: Option<String>,
}

/// Create payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateConnectionRequest {
    /// Provider kind.
    pub provider: ProviderKind,
    /// Friendly slug.
    #[validate(length(min = 1, max = 255))]
    pub slug: String,
    /// Display name.
    #[validate(length(min = 1, max = 128))]
    pub display_name: String,
    /// Environment variable/secret-store key, never the secret itself.
    #[validate(length(min = 1, max = 255))]
    pub credential_ref: String,
    /// Resources to deliver for.
    #[serde(default)]
    pub resources: Vec<ResourceRequest>,
}

/// Update payload; all fields optional.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateConnectionRequest {
    /// New slug.
    #[validate(length(min = 1, max = 255))]
    pub slug: Option<String>,
    /// New display name.
    #[validate(length(min = 1, max = 128))]
    pub display_name: Option<String>,
    /// Active or disabled.
    pub status: Option<ConnectionStatus>,
}

fn actor(auth: &OperatorAuth) -> AuditActor {
    AuditActor::member(&auth.operator.public_id, &auth.operator.email)
}

#[tracing::instrument(skip(pool, auth))]
async fn list_connections(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match connection_svc::list_connections(pool.get_ref(), auth.organization_id()).await {
        Ok(connections) => HttpResponse::Ok().json(ConnectionsResponse { connections }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn create_connection(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    body: web::Json<CreateConnectionRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_mutating() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    let input = NewConnection {
        provider: body.provider,
        slug: body.slug.clone(),
        display_name: body.display_name.clone(),
        credential_ref: body.credential_ref.clone(),
        resources: body
            .resources
            .iter()
            .map(|resource| NewResource {
                resource_kind: resource.resource_kind.clone(),
                external_id: resource.external_id.clone(),
                display_name: resource.display_name.clone(),
            })
            .collect(),
    };
    match connection_svc::create_connection(
        pool.get_ref(),
        auth.organization_id(),
        input,
        actor(&auth),
    )
    .await
    {
        Ok(connection) => HttpResponse::Created().json(connection),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_connection(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match connection_svc::get_connection(pool.get_ref(), auth.organization_id(), &path.into_inner())
        .await
    {
        Ok(connection) => HttpResponse::Ok().json(connection),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn update_connection(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    body: web::Json<UpdateConnectionRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_mutating() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    let input = UpdateConnection {
        slug: body.slug.clone(),
        display_name: body.display_name.clone(),
        status: body.status,
    };
    match connection_svc::update_connection(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        input,
        actor(&auth),
    )
    .await
    {
        Ok(connection) => HttpResponse::Ok().json(connection),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn delete_connection(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_mutating() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match connection_svc::delete_connection(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        actor(&auth),
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => map_error(e, &req),
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

fn validation_error(req: &HttpRequest, detail: &str) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::UNPROCESSABLE_ENTITY,
        ProblemCode::ValidationFailed,
        format!("validation: {detail}"),
    )
    .into_response()
}

fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

fn map_error(error: ConnectionError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        ConnectionError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        ConnectionError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "connection not found".to_string(),
        ),
        ConnectionError::Duplicate(slug) => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            format!("connection slug is already in use: {slug}"),
        ),
        ConnectionError::ManagedByEnvironment => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "connection is managed by the environment".to_string(),
        ),
        ConnectionError::MissingCredential(reference) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            format!("credential reference '{reference}' is not set"),
        ),
        ConnectionError::CredentialInvalid(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            format!("invalid credentials: {message}"),
        ),
        ConnectionError::Database(error) => {
            tracing::error!(error = %error, "connection operation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode as TestStatus, test};

    #[actix_web::test]
    async fn list_requires_a_member_token() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::get().uri("/v1/connections").to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn create_requires_a_member_token() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/connections")
            .set_json(serde_json::json!({
                "provider": "github",
                "slug": "acme",
                "display_name": "Acme",
                "credential_ref": "GITHUB_CREDS"
            }))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }
}
