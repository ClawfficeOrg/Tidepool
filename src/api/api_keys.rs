//! API-key HTTP surface (task 1.1.3).
//!
//! Members (owners) manage keys at `/v1/api-keys`. Automation authenticates
//! with a key at `/v1/api-keys/whoami`, which requires `projects:read` and
//! demonstrates the 403 scope contract.
//!
//! The full key is returned by `POST /v1/api-keys` exactly once. List and
//! whoami return only the prefix.

use crate::entities::api_key::{ApiKeyScope, ApiKeyView};
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::{self as api_key_svc, ApiKeyServiceError, AuditActor, AuditEntry};
use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use validator::Validate;

/// Register the API-key routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/api-keys")
            .route("", web::get().to(list_keys))
            .route("", web::post().to(create_key))
            .route("/whoami", web::get().to(whoami))
            .route("/{id}", web::delete().to(revoke_key)),
    );
}

/// Key list response.
#[derive(Debug, Serialize)]
pub struct ApiKeysResponse {
    /// Keys in the organization, newest first; no secrets.
    pub api_keys: Vec<ApiKeyView>,
}

/// Create-key payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKeyRequest {
    /// Human-readable name.
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    /// Capabilities to grant; at least one.
    pub scopes: Vec<ApiKeyScope>,
}

/// Create-key response: the secret is shown exactly once.
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    /// Key metadata (prefix only).
    pub api_key: ApiKeyView,
    /// The full `<prefix>.<secret>`; not stored in plaintext.
    pub secret: String,
}

#[tracing::instrument(skip(pool, auth))]
async fn list_keys(pool: Option<web::Data<MySqlPool>>, auth: OperatorAuth) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable();
    };
    match api_key_svc::list_api_keys(pool.get_ref(), auth.organization_id()).await {
        Ok(keys) => HttpResponse::Ok().json(ApiKeysResponse {
            api_keys: keys.iter().map(ApiKeyView::from).collect(),
        }),
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn create_key(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    body: web::Json<CreateApiKeyRequest>,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable();
    };
    if let Err(e) = body.validate() {
        return HttpResponse::BadRequest().json(error_body(&format!("validation: {e}")));
    }

    match api_key_svc::create_api_key(
        pool.get_ref(),
        auth.organization_id(),
        Some(auth.operator.id),
        &body.name,
        &body.scopes,
    )
    .await
    {
        Ok((key, secret)) => {
            let view = ApiKeyView::from(&key);
            api_key_svc::audit(
                pool.get_ref(),
                AuditEntry {
                    organization_id: auth.organization_id(),
                    actor: AuditActor::member(&auth.operator.public_id, &auth.operator.email),
                    action: "api_key.create",
                    target_kind: Some("api_key"),
                    target_public_id: Some(&key.public_id),
                    detail: serde_json::json!({
                        "name": key.name,
                        "prefix": key.prefix,
                        "scopes": body.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    }),
                },
            )
            .await;
            HttpResponse::Created().json(CreateApiKeyResponse {
                api_key: view,
                secret,
            })
        }
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn revoke_key(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable();
    };
    let public_id = path.into_inner();

    match api_key_svc::revoke_api_key(pool.get_ref(), auth.organization_id(), &public_id).await {
        Ok(key) => {
            api_key_svc::audit(
                pool.get_ref(),
                AuditEntry {
                    organization_id: auth.organization_id(),
                    actor: AuditActor::member(&auth.operator.public_id, &auth.operator.email),
                    action: "api_key.revoke",
                    target_kind: Some("api_key"),
                    target_public_id: Some(&key.public_id),
                    detail: serde_json::json!({ "prefix": key.prefix }),
                },
            )
            .await;
            HttpResponse::NoContent().finish()
        }
        Err(e) => map_error(e),
    }
}

/// Automation introspection. A key must carry `projects:read`.
#[tracing::instrument(skip(key))]
async fn whoami(key: ApiKeyAuth) -> HttpResponse {
    if let Err(e) = key.require_scope(ApiKeyScope::ProjectsRead) {
        return error_response(e);
    }
    HttpResponse::Ok().json(ApiKeyView::from(&key.key))
}

fn db_unavailable() -> HttpResponse {
    HttpResponse::ServiceUnavailable().json(error_body("database unavailable"))
}

fn error_body(msg: &str) -> serde_json::Value {
    serde_json::json!({ "error": msg })
}

fn error_response(error: actix_web::Error) -> HttpResponse {
    let response = error.error_response();
    HttpResponse::build(response.status()).json(error_body(
        response.status().canonical_reason().unwrap_or("forbidden"),
    ))
}

fn map_error(error: ApiKeyServiceError) -> HttpResponse {
    match error {
        ApiKeyServiceError::Invalid(message) => {
            HttpResponse::BadRequest().json(error_body(&message))
        }
        ApiKeyServiceError::NotFound => {
            HttpResponse::NotFound().json(error_body("API key not found"))
        }
        ApiKeyServiceError::InvalidCredentials => {
            HttpResponse::Unauthorized().json(error_body("invalid API key"))
        }
        other => {
            tracing::error!(error = ?other, "API key operation failed");
            HttpResponse::InternalServerError().json(error_body("internal error"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test};

    #[actix_web::test]
    async fn whoami_requires_a_key() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::get()
            .uri("/v1/api-keys/whoami")
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn member_jwt_is_not_an_api_key() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::get()
            .uri("/v1/api-keys/whoami")
            .insert_header(("Authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.e30.sig"))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn list_requires_a_member_token() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::get().uri("/v1/api-keys").to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
