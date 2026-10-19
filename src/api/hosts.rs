//! Host registry HTTP surface (task 1.2.1).
//!
//! - `GET    /v1/hosts`       — list live hosts
//! - `POST   /v1/hosts`       — register a host (slug derived from hostname)
//! - `GET    /v1/hosts/{id}`  — fetch one live host
//! - `PATCH  /v1/hosts/{id}`  — rename slug/display name; id is immutable
//! - `DELETE /v1/hosts/{id}`  — revoke the pool relationship
//!
//! Revoked hosts are absent from every endpoint. Mutating routes reject a
//! viewer with the hub problem contract.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::host::HostView;
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::AuditActor;
use crate::services::hosts::{self as host_svc, HostServiceError, NewHost, RenameHost};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use validator::Validate;

/// Register the host routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/hosts")
            .route("", web::get().to(list_hosts))
            .route("", web::post().to(register_host))
            .route("/{id}", web::get().to(get_host))
            .route("/{id}", web::patch().to(rename_host))
            .route("/{id}", web::delete().to(revoke_host)),
    );
}

/// List response.
#[derive(Debug, Serialize)]
pub struct HostsResponse {
    /// Live hosts, ordered by slug.
    pub hosts: Vec<HostView>,
}

/// Registration payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateHostRequest {
    /// Hostname reported by the machine.
    #[validate(length(min = 1, max = 255))]
    pub hostname: String,
    /// Optional display name; defaults to the hostname.
    #[validate(length(min = 1, max = 128))]
    pub display_name: Option<String>,
}

/// Rename payload; at least one field is required.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RenameHostRequest {
    /// New friendly slug.
    #[validate(length(min = 1, max = 255))]
    pub slug: Option<String>,
    /// New display name.
    #[validate(length(min = 1, max = 128))]
    pub display_name: Option<String>,
}

fn actor(auth: &OperatorAuth) -> AuditActor {
    AuditActor::member(&auth.operator.public_id, &auth.operator.email)
}

#[tracing::instrument(skip(pool, auth))]
async fn list_hosts(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match host_svc::list_hosts(pool.get_ref(), auth.organization_id()).await {
        Ok(hosts) => HttpResponse::Ok().json(HostsResponse {
            hosts: hosts.iter().map(HostView::from).collect(),
        }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn register_host(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    body: web::Json<CreateHostRequest>,
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

    let input = NewHost {
        hostname: body.hostname.clone(),
        display_name: body.display_name.clone(),
    };
    match host_svc::register_host(pool.get_ref(), auth.organization_id(), input, actor(&auth)).await
    {
        Ok(host) => HttpResponse::Created().json(HostView::from(&host)),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_host(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match host_svc::find_host(pool.get_ref(), auth.organization_id(), &path.into_inner()).await {
        Ok(Some(host)) => HttpResponse::Ok().json(HostView::from(&host)),
        Ok(None) => not_found(&req),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn rename_host(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    body: web::Json<RenameHostRequest>,
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

    let input = RenameHost {
        slug: body.slug.clone(),
        display_name: body.display_name.clone(),
    };
    match host_svc::rename_host(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        input,
        actor(&auth),
    )
    .await
    {
        Ok(host) => HttpResponse::Ok().json(HostView::from(&host)),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn revoke_host(
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
    match host_svc::revoke_host(
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

fn not_found(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::NOT_FOUND,
        ProblemCode::NotFound,
        "host not found",
    )
    .into_response()
}

fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

fn map_error(error: HostServiceError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        HostServiceError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        HostServiceError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "host not found".to_string(),
        ),
        HostServiceError::SlugConflict(slug) => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            format!("slug is already in use: {slug}"),
        ),
        HostServiceError::UnknownHostSlug { field, slug } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            format!("{field}: unknown host slug '{slug}'"),
        ),
        other => {
            tracing::error!(error = %other, "host operation failed");
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
        let req = test::TestRequest::get().uri("/v1/hosts").to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn register_requires_a_member_token() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/hosts")
            .set_json(serde_json::json!({"hostname": "studio-mac"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }
}
