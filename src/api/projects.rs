//! Project and configuration HTTP surface (task 1.4.1).
//!
//! - `GET    /v1/projects`       — list projects
//! - `POST   /v1/projects`       — create a project
//! - `GET    /v1/projects/{id}`  — fetch one project
//! - `PATCH  /v1/projects/{id}`  — rename slug/name; id is immutable
//! - `DELETE /v1/projects/{id}`  — delete a project (revisions cascade)
//! - `POST   /v1/projects/{id}/configuration/validate` — dry-run validation
//! - `POST   /v1/projects/{id}/configuration/install`  — install a revision
//! - `GET    /v1/projects/{id}/configuration`          — the active revision
//! - `GET    /v1/projects/{id}/revisions`              — revision history
//! - `GET    /v1/projects/{id}/revisions/{revision_id}` — one revision
//!
//! Reads accept a member token (any role) or an API key with `projects:read`.
//! Project mutations require a non-viewer member. Configuration validate and
//! install accept a non-viewer member or an API key with the matching scope.
//! Every query is scoped to the organization the credential belongs to.

use crate::api::problem::{Problem, ProblemCode, ProblemError};
use crate::entities::api_key::ApiKeyScope;
use crate::entities::configuration_revision::{
    ConfigFile, FieldIssue, RevisionMetaView, RevisionView,
};
use crate::entities::project::ProjectView;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::AuditActor;
use crate::services::configuration::{self as configuration_svc, ConfigurationError};
use crate::services::projects::{
    self as project_svc, NewProject, ProjectServiceError, RenameProject,
};
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};
use validator::Validate;

/// Register the project and configuration routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/projects")
            .route("", web::get().to(list_projects))
            .route("", web::post().to(create_project))
            .route("/{id}", web::get().to(get_project))
            .route("/{id}", web::patch().to(rename_project))
            .route("/{id}", web::delete().to(delete_project))
            .route(
                "/{id}/configuration/validate",
                web::post().to(validate_configuration),
            )
            .route(
                "/{id}/configuration/install",
                web::post().to(install_configuration),
            )
            .route("/{id}/configuration", web::get().to(get_active_revision))
            .route("/{id}/revisions", web::get().to(list_revisions))
            .route("/{id}/revisions/{revision_id}", web::get().to(get_revision)),
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

    /// Correlation id for problem documents.
    fn request_id(&self) -> &str {
        match self {
            HubAuth::Key(auth) => &auth.request_id,
            HubAuth::Member(auth) => &auth.request_id,
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

    /// Reads: a member of any role, or an API key with `scope`.
    fn require_read(&self, scope: ApiKeyScope) -> Result<(), Error> {
        match self {
            HubAuth::Key(auth) => auth.require_scope(scope),
            HubAuth::Member(_) => Ok(()),
        }
    }

    /// Project mutation: a non-viewer member. API keys carry no project-write
    /// scope, so they are refused.
    fn require_project_write(&self) -> Result<(), Error> {
        match self {
            HubAuth::Key(_) => Err(forbidden(
                self.request_id(),
                "project mutation requires a member account",
            )),
            HubAuth::Member(auth) => auth.require_mutating(),
        }
    }

    /// Validate/install: a non-viewer member or an API key with `scope`.
    fn require_scope_or_member(&self, scope: ApiKeyScope) -> Result<(), Error> {
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

/// A 403 with the hub problem shape.
fn forbidden(request_id: &str, detail: &str) -> Error {
    ProblemError::new(Problem::new(
        StatusCode::FORBIDDEN,
        ProblemCode::Forbidden,
        detail,
        request_id,
    ))
    .into()
}

/// Project list response.
#[derive(Debug, Serialize)]
pub struct ProjectsResponse {
    /// Projects in the organization, ordered by slug.
    pub projects: Vec<ProjectView>,
}

/// Create-project payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
    /// Human-readable name.
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    /// Optional friendly slug; derived from `name` when absent.
    #[validate(length(min = 1, max = 255))]
    pub slug: Option<String>,
}

/// Rename payload; at least one field is required.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RenameProjectRequest {
    /// New friendly slug.
    #[validate(length(min = 1, max = 255))]
    pub slug: Option<String>,
    /// New name.
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
}

/// One authored configuration file.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfigFileRequest {
    /// Authored file path.
    #[validate(length(min = 1, max = 512))]
    pub path: String,
    /// Authored file content.
    #[validate(length(max = 1_000_000))]
    pub content: String,
}

/// Install/validate payload: the authored file set.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRequest {
    /// Authored files; exactly one must be `tidepool.json`.
    #[validate(length(min = 1, max = 100))]
    pub files: Vec<ConfigFileRequest>,
}

/// Install response.
#[derive(Debug, Serialize)]
pub struct InstallResponse {
    /// The stored, immutable revision.
    pub revision: RevisionView,
    /// True when the revision became active.
    pub activated: bool,
}

/// Validate response.
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// True when every reference resolved.
    pub valid: bool,
    /// Version the revision would get.
    pub version: u32,
    /// Named field issues, empty when valid.
    pub errors: Vec<FieldIssue>,
}

/// Revision-history response.
#[derive(Debug, Serialize)]
pub struct RevisionsResponse {
    /// Revisions, newest first, metadata only.
    pub revisions: Vec<RevisionMetaView>,
}

fn files_from_request(files: Vec<ConfigFileRequest>) -> Vec<ConfigFile> {
    files
        .into_iter()
        .map(|file| ConfigFile {
            path: file.path,
            content: file.content,
        })
        .collect()
}

#[tracing::instrument(skip(pool, auth))]
async fn list_projects(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match project_svc::list_projects(pool.get_ref(), auth.organization_id()).await {
        Ok(projects) => HttpResponse::Ok().json(ProjectsResponse {
            projects: projects.iter().map(ProjectView::from).collect(),
        }),
        Err(error) => map_project_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn create_project(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    body: web::Json<CreateProjectRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_project_write() {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(error) = body.validate() {
        return validation_error(&req, &error.to_string());
    }

    let input = NewProject {
        slug: body.slug.clone(),
        name: body.name.clone(),
    };
    match project_svc::create_project(pool.get_ref(), auth.organization_id(), input, auth.actor())
        .await
    {
        Ok(project) => HttpResponse::Created().json(ProjectView::from(&project)),
        Err(error) => map_project_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_project(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match project_svc::find_project(pool.get_ref(), auth.organization_id(), &path.into_inner())
        .await
    {
        Ok(Some(project)) => HttpResponse::Ok().json(ProjectView::from(&project)),
        Ok(None) => not_found(&req, "project not found"),
        Err(error) => map_project_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn rename_project(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<RenameProjectRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_project_write() {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(error) = body.validate() {
        return validation_error(&req, &error.to_string());
    }

    let input = RenameProject {
        slug: body.slug.clone(),
        name: body.name.clone(),
    };
    match project_svc::rename_project(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        input,
        auth.actor(),
    )
    .await
    {
        Ok(project) => HttpResponse::Ok().json(ProjectView::from(&project)),
        Err(error) => map_project_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn delete_project(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_project_write() {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match project_svc::delete_project(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        auth.actor(),
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => map_project_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn validate_configuration(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<ConfigurationRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_scope_or_member(ApiKeyScope::ConfigurationValidate) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(error) = body.validate() {
        return validation_error(&req, &error.to_string());
    }

    let files = files_from_request(body.files.clone());
    match configuration_svc::validate(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        &files,
    )
    .await
    {
        Ok(result) => HttpResponse::Ok().json(ValidateResponse {
            valid: result.valid,
            version: result.version,
            errors: result.errors,
        }),
        Err(error) => map_configuration_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn install_configuration(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    body: web::Json<ConfigurationRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_scope_or_member(ApiKeyScope::ConfigurationInstall) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(error) = body.validate() {
        return validation_error(&req, &error.to_string());
    }

    let files = files_from_request(body.files.clone());
    match configuration_svc::install(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
        &files,
        auth.actor(),
    )
    .await
    {
        Ok(result) => HttpResponse::Created().json(InstallResponse {
            revision: result.revision,
            activated: result.activated,
        }),
        Err(error) => map_configuration_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_active_revision(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match configuration_svc::get_active_revision(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
    )
    .await
    {
        Ok(Some(revision)) => HttpResponse::Ok().json(revision),
        Ok(None) => not_found(&req, "no active configuration"),
        Err(error) => map_configuration_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn list_revisions(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match configuration_svc::list_revisions(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
    )
    .await
    {
        Ok(revisions) => HttpResponse::Ok().json(RevisionsResponse { revisions }),
        Err(error) => map_configuration_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_revision(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<(String, String)>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read(ApiKeyScope::ProjectsRead) {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let (project_id, revision_id) = path.into_inner();
    match configuration_svc::get_revision(
        pool.get_ref(),
        auth.organization_id(),
        &project_id,
        &revision_id,
    )
    .await
    {
        Ok(Some(revision)) => HttpResponse::Ok().json(revision),
        Ok(None) => not_found(&req, "revision not found"),
        Err(error) => map_configuration_error(error, &req),
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

fn not_found(req: &HttpRequest, detail: &str) -> HttpResponse {
    Problem::from_request(req, StatusCode::NOT_FOUND, ProblemCode::NotFound, detail).into_response()
}

fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

fn map_project_error(error: ProjectServiceError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        ProjectServiceError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        ProjectServiceError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "project not found".to_string(),
        ),
        ProjectServiceError::SlugConflict(slug) => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            format!("project slug is already in use: {slug}"),
        ),
        ProjectServiceError::Database(error) => {
            tracing::error!(error = %error, "project operation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}

fn map_configuration_error(error: ConfigurationError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        ConfigurationError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            format!("invalid configuration: {message}"),
        ),
        ConfigurationError::ProjectNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "project not found".to_string(),
        ),
        ConfigurationError::RevisionNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "revision not found".to_string(),
        ),
        ConfigurationError::Database(error) => {
            tracing::error!(error = %error, "configuration operation failed");
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
    async fn list_requires_credentials() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::get().uri("/v1/projects").to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn create_requires_credentials() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/projects")
            .set_json(serde_json::json!({"name": "Nightly"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn install_requires_credentials() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/projects/11111111-1111-4111-8111-111111111111/configuration/install")
            .set_json(serde_json::json!({"files": []}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }
}
