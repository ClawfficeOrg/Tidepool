//! GitHub App connection transport (task 1.3.2).
//!
//! Receives webhook deliveries, verifies `X-Hub-Signature-256` against the
//! connection's `webhook_secret`, deduplicates `X-GitHub-Delivery`, resolves the
//! repository to the connection that owns it, and maps the GitHub event to a
//! typed [`GithubEvent`].
//!
//! Documented events:
//!
//! - `issues` / `opened` → issue created
//! - `issues` / `labeled` → issue label added
//! - `pull_request` / `opened` → pull request created
//! - `pull_request` / `labeled` → pull request label added
//! - `issue_comment` / `created` → issue or pull-request comment created
//! - `pull_request_review_comment` / `created` → pull-request comment created
//! - `pull_request_review` / `submitted` → pull-request review submitted
//! - `ping`, `installation`, `installation_repositories` for App lifecycle
//!
//! GitHub's legacy `issue_comment` on a pull request is normalised to the
//! pull-request comment variant; the native review-comment event is kept as
//! `pull_request.review_comment_created`.

use super::{ConnectionStatus, EffectiveConnection, credential_material, find_effective_by_slug};
use actix_web::http::header::HeaderMap;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use sqlx::MySqlPool;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// Domain errors for the GitHub webhook route.
#[derive(Debug, Error)]
pub enum GithubWebhookError {
    /// No active GitHub connection with that slug.
    #[error("connection not found")]
    NotFound,
    /// Signature missing or invalid.
    #[error("invalid webhook signature")]
    BadSignature,
    /// Delivery id/event/payload is malformed.
    #[error("invalid webhook: {0}")]
    Invalid(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Typed internal GitHub event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubEvent {
    /// An issue was opened.
    IssueCreated {
        /// Repository full name.
        repository: String,
        /// Issue number.
        number: u64,
        /// Issue title.
        title: String,
        /// Invoking user.
        actor: String,
    },
    /// A pull request was opened.
    PullRequestCreated {
        /// Repository full name.
        repository: String,
        /// Pull request number.
        number: u64,
        /// Pull request title.
        title: String,
        /// Invoking user.
        actor: String,
    },
    /// A comment was created on an issue.
    IssueCommentCreated {
        /// Repository full name.
        repository: String,
        /// Issue number.
        number: u64,
        /// Comment id.
        comment_id: u64,
        /// Comment body.
        body: String,
        /// Commenting user.
        actor: String,
    },
    /// A comment was created on a pull request (including legacy
    /// `issue_comment` on a pull request).
    PullRequestCommentCreated {
        /// Repository full name.
        repository: String,
        /// Pull request number.
        number: u64,
        /// Comment id.
        comment_id: u64,
        /// Comment body.
        body: String,
        /// Commenting user.
        actor: String,
    },
    /// A review comment was created on a pull request.
    PullRequestReviewCommentCreated {
        /// Repository full name.
        repository: String,
        /// Pull request number.
        number: u64,
        /// Comment id.
        comment_id: u64,
        /// Comment body.
        body: String,
        /// Commenting user.
        actor: String,
    },
    /// A pull request review was submitted.
    PullRequestReviewSubmitted {
        /// Repository full name.
        repository: String,
        /// Pull request number.
        number: u64,
        /// Review state (`approved`, `changes_requested`, `commented`).
        state: String,
        /// Review author.
        actor: String,
    },
    /// A label was added to an issue.
    IssueLabelAdded {
        /// Repository full name.
        repository: String,
        /// Issue number.
        number: u64,
        /// Label name.
        label: String,
        /// Invoking user.
        actor: String,
    },
    /// A label was added to a pull request.
    PullRequestLabelAdded {
        /// Repository full name.
        repository: String,
        /// Pull request number.
        number: u64,
        /// Label name.
        label: String,
        /// Invoking user.
        actor: String,
    },
    /// GitHub App ping.
    Ping {
        /// Repository, when the ping is repository-scoped.
        repository: Option<String>,
    },
    /// GitHub App installation lifecycle.
    Installation {
        /// Action (`created`, `deleted`, ...).
        action: String,
        /// Installation id.
        installation_id: u64,
    },
    /// Installation repository selection changed.
    InstallationRepositories {
        /// Action (`added`, `removed`).
        action: String,
        /// Installation id.
        installation_id: u64,
    },
    /// A GitHub event the pool does not match on yet.
    Unsupported {
        /// GitHub event name.
        event: String,
        /// GitHub action.
        action: Option<String>,
    },
}

impl GithubEvent {
    /// Stable internal event name.
    pub fn kind(&self) -> &'static str {
        match self {
            GithubEvent::IssueCreated { .. } => "issue.created",
            GithubEvent::PullRequestCreated { .. } => "pull_request.created",
            GithubEvent::IssueCommentCreated { .. } => "issue.comment_created",
            GithubEvent::PullRequestCommentCreated { .. } => "pull_request.comment_created",
            GithubEvent::PullRequestReviewCommentCreated { .. } => {
                "pull_request.review_comment_created"
            }
            GithubEvent::PullRequestReviewSubmitted { .. } => "pull_request.review_submitted",
            GithubEvent::IssueLabelAdded { .. } => "issue.label_added",
            GithubEvent::PullRequestLabelAdded { .. } => "pull_request.label_added",
            GithubEvent::Ping { .. } => "ping",
            GithubEvent::Installation { .. } => "installation",
            GithubEvent::InstallationRepositories { .. } => "installation_repositories",
            GithubEvent::Unsupported { .. } => "unsupported",
        }
    }

    /// GitHub action, when the event carries one.
    pub fn action(&self) -> Option<String> {
        match self {
            GithubEvent::IssueCreated { .. }
            | GithubEvent::PullRequestCreated { .. }
            | GithubEvent::IssueCommentCreated { .. }
            | GithubEvent::PullRequestCommentCreated { .. }
            | GithubEvent::PullRequestReviewCommentCreated { .. } => Some("created".into()),
            GithubEvent::PullRequestReviewSubmitted { .. } => Some("submitted".into()),
            GithubEvent::IssueLabelAdded { .. } | GithubEvent::PullRequestLabelAdded { .. } => {
                Some("labeled".into())
            }
            GithubEvent::Installation { action, .. }
            | GithubEvent::InstallationRepositories { action, .. } => Some(action.clone()),
            GithubEvent::Unsupported { action, .. } => action.clone(),
            GithubEvent::Ping { .. } => None,
        }
    }

    /// Repository full name, when the event is repository-scoped.
    pub fn repository(&self) -> Option<&str> {
        match self {
            GithubEvent::IssueCreated { repository, .. }
            | GithubEvent::PullRequestCreated { repository, .. }
            | GithubEvent::IssueCommentCreated { repository, .. }
            | GithubEvent::PullRequestCommentCreated { repository, .. }
            | GithubEvent::PullRequestReviewCommentCreated { repository, .. }
            | GithubEvent::PullRequestReviewSubmitted { repository, .. }
            | GithubEvent::IssueLabelAdded { repository, .. }
            | GithubEvent::PullRequestLabelAdded { repository, .. } => Some(repository),
            GithubEvent::Ping { repository } => repository.as_deref(),
            _ => None,
        }
    }
}

/// Result of accepting a webhook delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookOutcome {
    /// GitHub delivery id.
    pub delivery_id: String,
    /// Internal event name.
    pub event: String,
    /// GitHub action.
    pub action: Option<String>,
    /// True when this delivery id was already recorded.
    pub duplicate: bool,
    /// False when the delivery was recorded but not matched (unknown repo or
    /// unsupported event).
    pub accepted: bool,
}

/// Verify `X-Hub-Signature-256: sha256=<hex>` over the raw body.
pub fn verify_signature(secret: &str, signature: &str, body: &[u8]) -> bool {
    let Some(encoded) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(encoded) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// Compute the signature header for tests/reference.
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Handle one GitHub webhook delivery.
pub async fn handle_webhook(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<WebhookOutcome, GithubWebhookError> {
    let connection = find_effective_by_slug(pool, organization_id, slug)
        .await
        .map_err(|e| GithubWebhookError::Invalid(e.to_string()))?
        .ok_or(GithubWebhookError::NotFound)?;
    if connection.provider != super::ProviderKind::Github
        || connection.status != ConnectionStatus::Active
    {
        return Err(GithubWebhookError::NotFound);
    }

    let material = credential_material(&connection.credential_ref)
        .map_err(|e| GithubWebhookError::Invalid(e.to_string()))?;
    let secret = material
        .values
        .get("webhook_secret")
        .ok_or_else(|| GithubWebhookError::Invalid("webhook_secret is not configured".into()))?;

    let signature =
        header(headers, "x-hub-signature-256").ok_or(GithubWebhookError::BadSignature)?;
    if !verify_signature(secret, signature, body) {
        return Err(GithubWebhookError::BadSignature);
    }

    let delivery_id = header(headers, "x-github-delivery")
        .ok_or_else(|| GithubWebhookError::Invalid("x-github-delivery is required".into()))?;
    let event_name = header(headers, "x-github-event")
        .ok_or_else(|| GithubWebhookError::Invalid("x-github-event is required".into()))?;
    if delivery_id.len() > 128 || event_name.len() > 64 {
        return Err(GithubWebhookError::Invalid(
            "delivery id or event name is too long".into(),
        ));
    }

    let payload: Value = serde_json::from_slice(body)
        .map_err(|e| GithubWebhookError::Invalid(format!("payload is not JSON: {e}")))?;
    let event = map_event(event_name, &payload);

    let accepted = match event.repository() {
        Some(repository) => repository_owned(&connection, repository),
        None => true,
    };

    let result = sqlx::query(
        "INSERT IGNORE INTO webhook_deliveries \
         (organization_id, connection_public_id, provider, delivery_id, event, action, result) \
         VALUES (?, ?, 'github', ?, ?, ?, ?)",
    )
    .bind(organization_id)
    .bind(&connection.public_id)
    .bind(delivery_id)
    .bind(event_name)
    .bind(event.action())
    .bind(if accepted { "accepted" } else { "ignored" })
    .execute(pool)
    .await
    .map_err(GithubWebhookError::Database)?;

    Ok(WebhookOutcome {
        delivery_id: delivery_id.to_string(),
        event: event.kind().to_string(),
        action: event.action(),
        duplicate: result.rows_affected() == 0,
        accepted,
    })
}

fn repository_owned(connection: &EffectiveConnection, repository: &str) -> bool {
    let repository = repository.trim().to_ascii_lowercase();
    connection.resources.iter().any(|resource| {
        resource.resource_kind == "repository"
            && resource.external_id.to_ascii_lowercase() == repository
    })
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Map a GitHub event name + payload to the typed internal event.
pub fn map_event(event: &str, payload: &Value) -> GithubEvent {
    let repository = string_at(payload, &["repository", "full_name"]).unwrap_or_default();
    let action = string_at(payload, &["action"]);

    match (event, action.as_deref()) {
        ("issues", Some("opened")) => GithubEvent::IssueCreated {
            repository,
            number: u64_at(payload, &["issue", "number"]).unwrap_or_default(),
            title: string_at(payload, &["issue", "title"]).unwrap_or_default(),
            actor: string_at(payload, &["sender", "login"]).unwrap_or_default(),
        },
        ("issues", Some("labeled")) => GithubEvent::IssueLabelAdded {
            repository,
            number: u64_at(payload, &["issue", "number"]).unwrap_or_default(),
            label: string_at(payload, &["label", "name"]).unwrap_or_default(),
            actor: string_at(payload, &["sender", "login"]).unwrap_or_default(),
        },
        ("pull_request", Some("opened")) => GithubEvent::PullRequestCreated {
            repository,
            number: u64_at(payload, &["pull_request", "number"]).unwrap_or_default(),
            title: string_at(payload, &["pull_request", "title"]).unwrap_or_default(),
            actor: string_at(payload, &["sender", "login"]).unwrap_or_default(),
        },
        ("pull_request", Some("labeled")) => GithubEvent::PullRequestLabelAdded {
            repository,
            number: u64_at(payload, &["pull_request", "number"]).unwrap_or_default(),
            label: string_at(payload, &["label", "name"]).unwrap_or_default(),
            actor: string_at(payload, &["sender", "login"]).unwrap_or_default(),
        },
        ("issue_comment", Some("created")) => {
            let is_pull_request = payload
                .get("issue")
                .and_then(|issue| issue.get("pull_request"))
                .is_some();
            let number = u64_at(payload, &["issue", "number"]).unwrap_or_default();
            let comment_id = u64_at(payload, &["comment", "id"]).unwrap_or_default();
            let body = string_at(payload, &["comment", "body"]).unwrap_or_default();
            let actor = string_at(payload, &["comment", "user", "login"])
                .or_else(|| string_at(payload, &["sender", "login"]))
                .unwrap_or_default();
            if is_pull_request {
                GithubEvent::PullRequestCommentCreated {
                    repository,
                    number,
                    comment_id,
                    body,
                    actor,
                }
            } else {
                GithubEvent::IssueCommentCreated {
                    repository,
                    number,
                    comment_id,
                    body,
                    actor,
                }
            }
        }
        ("pull_request_review_comment", Some("created")) => {
            GithubEvent::PullRequestReviewCommentCreated {
                repository,
                number: u64_at(payload, &["pull_request", "number"]).unwrap_or_default(),
                comment_id: u64_at(payload, &["comment", "id"]).unwrap_or_default(),
                body: string_at(payload, &["comment", "body"]).unwrap_or_default(),
                actor: string_at(payload, &["comment", "user", "login"])
                    .or_else(|| string_at(payload, &["sender", "login"]))
                    .unwrap_or_default(),
            }
        }
        ("pull_request_review", Some("submitted")) => GithubEvent::PullRequestReviewSubmitted {
            repository,
            number: u64_at(payload, &["pull_request", "number"]).unwrap_or_default(),
            state: string_at(payload, &["review", "state"]).unwrap_or_default(),
            actor: string_at(payload, &["review", "user", "login"])
                .or_else(|| string_at(payload, &["sender", "login"]))
                .unwrap_or_default(),
        },
        ("ping", _) => GithubEvent::Ping {
            repository: (!repository.is_empty()).then_some(repository),
        },
        ("installation", _) => GithubEvent::Installation {
            action: action.clone().unwrap_or_default(),
            installation_id: u64_at(payload, &["installation", "id"]).unwrap_or_default(),
        },
        ("installation_repositories", _) => GithubEvent::InstallationRepositories {
            action: action.clone().unwrap_or_default(),
            installation_id: u64_at(payload, &["installation", "id"]).unwrap_or_default(),
        },
        _ => GithubEvent::Unsupported {
            event: event.to_string(),
            action,
        },
    }
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

fn u64_at(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SECRET: &str = "test-webhook-secret";

    fn signed(body: &str) -> String {
        sign(SECRET, body.as_bytes())
    }

    #[test]
    fn signature_verification_rejects_tampering() {
        let body = br#"{"action":"opened"}"#;
        let signature = sign(SECRET, body);
        assert!(verify_signature(SECRET, &signature, body));
        assert!(!verify_signature("other", &signature, body));
        assert!(!verify_signature(
            SECRET,
            &signature,
            br#"{"action":"closed"}"#
        ));
        assert!(!verify_signature(SECRET, "sha256=zz", body));
        assert!(!verify_signature(SECRET, "sha1=abc", body));
    }

    #[test]
    fn issue_created_maps_to_typed_event() {
        let payload = json!({
            "action": "opened",
            "repository": {"full_name": "acme/app"},
            "issue": {"number": 7, "title": "Bug"},
            "sender": {"login": "alice"}
        });
        let event = map_event("issues", &payload);
        assert_eq!(event.kind(), "issue.created");
        assert_eq!(
            event,
            GithubEvent::IssueCreated {
                repository: "acme/app".into(),
                number: 7,
                title: "Bug".into(),
                actor: "alice".into()
            }
        );
    }

    #[test]
    fn pull_request_opened_and_labels_map() {
        let opened = json!({
            "action": "opened",
            "repository": {"full_name": "acme/app"},
            "pull_request": {"number": 9, "title": "Feature"},
            "sender": {"login": "bob"}
        });
        assert_eq!(
            map_event("pull_request", &opened).kind(),
            "pull_request.created"
        );

        let labeled = json!({
            "action": "labeled",
            "repository": {"full_name": "acme/app"},
            "pull_request": {"number": 9},
            "label": {"name": "ready"},
            "sender": {"login": "bob"}
        });
        assert_eq!(
            map_event("pull_request", &labeled).kind(),
            "pull_request.label_added"
        );
    }

    #[test]
    fn issue_comment_on_pull_request_normalises_to_pr_comment() {
        let payload = json!({
            "action": "created",
            "repository": {"full_name": "acme/app"},
            "issue": {"number": 9, "pull_request": {"url": "x"}},
            "comment": {"id": 55, "body": "hi", "user": {"login": "bob"}},
            "sender": {"login": "bob"}
        });
        assert_eq!(
            map_event("issue_comment", &payload).kind(),
            "pull_request.comment_created"
        );
    }

    #[test]
    fn issue_comment_on_issue_stays_an_issue_comment() {
        let payload = json!({
            "action": "created",
            "repository": {"full_name": "acme/app"},
            "issue": {"number": 7},
            "comment": {"id": 56, "body": "hi", "user": {"login": "alice"}},
            "sender": {"login": "alice"}
        });
        assert_eq!(
            map_event("issue_comment", &payload).kind(),
            "issue.comment_created"
        );
    }

    #[test]
    fn review_comment_and_review_map() {
        let comment = json!({
            "action": "created",
            "repository": {"full_name": "acme/app"},
            "pull_request": {"number": 9},
            "comment": {"id": 57, "body": "nit", "user": {"login": "carol"}},
            "sender": {"login": "carol"}
        });
        assert_eq!(
            map_event("pull_request_review_comment", &comment).kind(),
            "pull_request.review_comment_created"
        );

        let review = json!({
            "action": "submitted",
            "repository": {"full_name": "acme/app"},
            "pull_request": {"number": 9},
            "review": {"state": "approved", "user": {"login": "carol"}},
            "sender": {"login": "carol"}
        });
        assert_eq!(
            map_event("pull_request_review", &review).kind(),
            "pull_request.review_submitted"
        );
    }

    #[test]
    fn ping_and_installation_map() {
        let ping = json!({"repository": {"full_name": "acme/app"}});
        assert_eq!(map_event("ping", &ping).kind(), "ping");

        let installation = json!({"action": "created", "installation": {"id": 12}});
        let event = map_event("installation", &installation);
        assert_eq!(event.kind(), "installation");
        assert_eq!(event.action().as_deref(), Some("created"));
    }

    #[test]
    fn unknown_events_are_typed_unsupported() {
        let event = map_event("star", &json!({"action": "created"}));
        assert_eq!(event.kind(), "unsupported");
        assert!(!signed("{}").is_empty());
    }
}
