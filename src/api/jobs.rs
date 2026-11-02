//! TideTable binding and host job HTTP surface (phase 1.7).
//!
//! - `GET  /v1/projects/{id}/tidetable` — the project's binding, if any
//! - `PUT  /v1/projects/{id}/tidetable` — bind to a slug the hub already lists
//! - `POST /v1/projects/{id}/jobs`      — dispatch one typed `tkrun` job
//! - `GET  /v1/jobs/{id}`               — one job, with its output so far
//! - `POST /v1/jobs/{id}/stop`          — ask the host to stop or kill it
//! - `PUT|DELETE /v1/projects/{id}/hosts/{host}/grants/jobs.tkrun` — grant or
//!   revoke a host's permission to run tkrun for a project
//!
//! Dispatch to an offline host is `409 daemon_not_connected` and stores no live
//! job. A second live job for the same project is `409 job_already_running`.
//! Errors use the RFC 9457 contract of the rest of the API.

use crate::api::manual_runs::HubAuth;
use crate::api::problem::{Problem, ProblemCode};
use crate::entities::api_key::ApiKeyScope;
use crate::services::tidetable_jobs::{
    self as jobs_svc, BindingRequest, DispatchRequest, GrantRequest, JobError, StopRequest,
};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use sqlx::MySqlPool;

/// Register the binding and job routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/v1/projects/{id}/tidetable")
            .route(web::get().to(get_binding))
            .route(web::put().to(put_binding)),
    )
    .service(
        web::resource("/v1/projects/{id}/hosts/{host}/grants/{capability}")
            .route(web::put().to(put_grant))
            .route(web::delete().to(delete_grant)),
    )
    .service(web::resource("/v1/projects/{id}/jobs").route(web::post().to(dispatch_job)))
    .service(web::resource("/v1/jobs/{id}").route(web::get().to(get_job)))
    .service(web::resource("/v1/jobs/{id}/stop").route(web::post().to(stop_job)));
}

#[tracing::instrument(skip(pool, auth))]
async fn get_binding(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match jobs_svc::find_binding(pool.get_ref(), auth.organization_id(), &path.into_inner()).await {
        Ok(Some(binding)) => HttpResponse::Ok().json(binding),
        Ok(None) => map_error(JobError::NotBound, &req),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn put_binding(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<BindingRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_write(ApiKeyScope::ConfigurationInstall) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match jobs_svc::bind_project(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        body.into_inner(),
    )
    .await
    {
        Ok(binding) => HttpResponse::Ok().json(binding),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn put_grant(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<(String, String, String)>,
    body: web::Json<GrantRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_write(ApiKeyScope::ConfigurationInstall) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let (project, host, capability) = path.into_inner();
    match jobs_svc::put_grant(
        pool.get_ref(),
        auth.organization_id(),
        &project,
        &host,
        &capability,
        body.into_inner(),
    )
    .await
    {
        Ok(grant) => HttpResponse::Ok().json(grant),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn delete_grant(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<(String, String, String)>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_write(ApiKeyScope::ConfigurationInstall) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let (project, host, capability) = path.into_inner();
    match jobs_svc::delete_grant(
        pool.get_ref(),
        auth.organization_id(),
        &project,
        &host,
        &capability,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn dispatch_job(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<DispatchRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_write(ApiKeyScope::RunsDispatch) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match jobs_svc::dispatch(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        body.into_inner(),
    )
    .await
    {
        Ok(job) => HttpResponse::Created().json(job),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_job(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match jobs_svc::get_job(pool.get_ref(), auth.organization_id(), &path.into_inner()).await {
        Ok(job) => HttpResponse::Ok().json(job),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn stop_job(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<StopRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_write(ApiKeyScope::RunsDispatch) {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match jobs_svc::stop(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        body.into_inner(),
    )
    .await
    {
        Ok(job) => HttpResponse::Accepted().json(job),
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

fn map_error(error: JobError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match &error {
        JobError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "not found".to_string(),
        ),
        JobError::NotBound => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "project has no TideTable binding".to_string(),
        ),
        JobError::Invalid(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            message.clone(),
        ),
        JobError::HubRefused(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            message.clone(),
        ),
        JobError::HubUnavailable(message) => (
            StatusCode::SERVICE_UNAVAILABLE,
            ProblemCode::ServiceUnavailable,
            message.clone(),
        ),
        JobError::AlreadyRunning => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "job_already_running: the project already has a live job".to_string(),
        ),
        JobError::GrantMissing(grant) => (
            StatusCode::FORBIDDEN,
            ProblemCode::Forbidden,
            format!("grant '{grant}' is missing for this host and project"),
        ),
        JobError::FeatureMissing(feature) => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            format!("host has not negotiated '{feature}'"),
        ),
        JobError::DaemonNotConnected { slug } => (
            StatusCode::CONFLICT,
            ProblemCode::DaemonNotConnected,
            format!("daemon_not_connected: host '{slug}' is offline"),
        ),
        JobError::NotLive => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "job is not live".to_string(),
        ),
        JobError::Database(error) => {
            tracing::error!(%error, "tidetable job request failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}
