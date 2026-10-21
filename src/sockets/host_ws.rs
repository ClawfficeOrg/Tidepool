//! Host WebSocket presence endpoint (task 1.2.3).
//!
//! `GET /ws/host` authenticates with the relationship credential established by
//! enrollment:
//!
//! - `X-Host-Id: <host public id>`
//! - `X-Host-Credential: <host-generated credential>`
//!
//! A successful upgrade marks the host connected. Text frame
//! `{"type":"heartbeat"}` refreshes the heartbeat and receives
//! `{"type":"heartbeat.ack"}`. Dropping the socket marks the host offline when
//! it was the last connection. A socket that stops sending heartbeats is closed
//! after [`HEARTBEAT_TIMEOUT_SECS`].
//!
//! Presence is never accepted from a client-supplied status field: only the
//! connection lifecycle and heartbeat timestamps in
//! [`crate::services::host_presence`] decide.

use crate::services::enrollment::{self, EnrollmentError};
use crate::services::host_presence::{self, HEARTBEAT_TIMEOUT_SECS};
use crate::services::host_protocol::{self, SendFrame};
use actix::{ActorContext, AsyncContext};
use actix_web::{Error, HttpRequest, HttpResponse, error::ErrorUnauthorized, web};
use actix_web_actors::ws;
use serde_json::json;
use sqlx::MySqlPool;
use std::time::{Duration, Instant};
use tracing::instrument;

/// Seconds between server-side heartbeat checks.
const HEARTBEAT_TICK_SECS: u64 = 30;

/// One host WebSocket connection.
pub struct HostWs {
    host_public_id: String,
    pool: MySqlPool,
    last_client_heartbeat: Instant,
}

impl actix::Actor for HostWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        host_protocol::register_socket(&self.host_public_id, ctx.address());
        let pool = self.pool.clone();
        let public_id = self.host_public_id.clone();
        actix::spawn(async move {
            match crate::services::runs::recover_for_host(&pool, &public_id).await {
                Ok(report) if report.scanned > 0 => tracing::info!(
                    host = %public_id,
                    scanned = report.scanned,
                    replayed = report.replayed,
                    "recovered runs after host reconnect"
                ),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "host run recovery failed"),
            }
        });
        ctx.run_interval(Duration::from_secs(HEARTBEAT_TICK_SECS), |actor, ctx| {
            if actor.last_client_heartbeat.elapsed()
                > Duration::from_secs(HEARTBEAT_TIMEOUT_SECS as u64)
            {
                tracing::info!(
                    host = %actor.host_public_id,
                    "host heartbeat timed out; closing connection"
                );
                ctx.close(Some(ws::CloseCode::Away.into()));
                ctx.stop();
            }
        });
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> actix::Running {
        let public_id = self.host_public_id.clone();
        host_protocol::unregister_socket(&public_id);
        let pool = self.pool.clone();
        actix::spawn(async move {
            if let Err(error) = host_presence::host_disconnected(&pool, &public_id).await {
                tracing::warn!(error = %error, "failed to mark host offline");
            }
        });
        actix::Running::Stop
    }
}

impl actix::Handler<SendFrame> for HostWs {
    type Result = ();

    fn handle(&mut self, msg: SendFrame, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

impl actix::StreamHandler<Result<ws::Message, ws::ProtocolError>> for HostWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                let frame = serde_json::from_str::<serde_json::Value>(&text).ok();
                let frame_type = frame
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if frame_type == "heartbeat" || text.trim() == "heartbeat" {
                    self.last_client_heartbeat = Instant::now();
                    let public_id = self.host_public_id.clone();
                    let pool = self.pool.clone();
                    // Refresh the registry immediately; the DB projection is
                    // only touched when the row needs to move back online.
                    host_presence::registry().heartbeat(&public_id, chrono::Utc::now());
                    actix::spawn(async move {
                        if let Err(error) = host_presence::heartbeat(&pool, &public_id).await {
                            tracing::warn!(error = %error, "host heartbeat failed");
                        }
                    });
                    ctx.text(json!({"type": "heartbeat.ack"}).to_string());
                    return;
                }

                let execution_id = frame
                    .as_ref()
                    .and_then(|value| value.get("executionId"))
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                let pool = self.pool.clone();
                match frame_type {
                    "execution_started" => {
                        actix::spawn(async move {
                            if let Err(error) =
                                host_protocol::record_started(&pool, &execution_id).await
                            {
                                tracing::warn!(error = %error, "failed to record execution start");
                            }
                        });
                    }
                    "execution_output" => {
                        let delta = frame
                            .as_ref()
                            .and_then(|value| value.get("text"))
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string();
                        actix::spawn(async move {
                            if let Err(error) =
                                host_protocol::record_output(&pool, &execution_id, &delta).await
                            {
                                tracing::warn!(error = %error, "failed to record execution output");
                            }
                        });
                    }
                    "execution_completed" => {
                        let status = frame
                            .as_ref()
                            .and_then(|value| value.get("status"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("failed")
                            .to_string();
                        let output = frame
                            .as_ref()
                            .and_then(|value| value.get("output"))
                            .and_then(|value| value.as_str())
                            .map(str::to_string);
                        actix::spawn(async move {
                            if let Err(error) = host_protocol::record_completion(
                                &pool,
                                &execution_id,
                                &status,
                                output.as_deref(),
                            )
                            .await
                            {
                                tracing::warn!(error = %error, "failed to record execution completion");
                            }
                            if let Err(error) = crate::services::runs::observe_completion(
                                &pool,
                                &execution_id,
                                &status,
                                output.as_deref(),
                            )
                            .await
                            {
                                tracing::warn!(error = %error, "failed to advance run on completion");
                            }
                        });
                    }
                    _ => {
                        ctx.text(
                            json!({"type": "error", "code": "invalid_request", "detail": "unknown frame"})
                                .to_string(),
                        );
                    }
                }
            }
            Ok(ws::Message::Ping(bytes)) => ctx.pong(&bytes),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, "host socket protocol error");
                ctx.stop();
            }
        }
    }
}

/// Upgrade an authenticated host connection.
#[instrument(skip(req, stream, pool))]
pub async fn ws_host(
    req: HttpRequest,
    stream: web::Payload,
    pool: Option<web::Data<MySqlPool>>,
) -> Result<HttpResponse, Error> {
    let host_id = req
        .headers()
        .get("x-host-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ErrorUnauthorized("X-Host-Id is required"))?;
    let credential = req
        .headers()
        .get("x-host-credential")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ErrorUnauthorized("X-Host-Credential is required"))?;

    let Some(pool) = pool else {
        return Err(actix_web::error::ErrorServiceUnavailable(
            "database unavailable",
        ));
    };

    let host = match enrollment::authenticate_host(pool.get_ref(), host_id, credential).await {
        Ok(host) => host,
        Err(EnrollmentError::InvalidCredentials) => {
            return Err(ErrorUnauthorized("invalid host credential"));
        }
        Err(error) => {
            tracing::error!(error = %error, "host socket authentication failed");
            return Err(actix_web::error::ErrorInternalServerError("internal error"));
        }
    };

    host_presence::host_connected(pool.get_ref(), host.organization_id, &host.public_id)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "host could not be marked connected");
            actix_web::error::ErrorServiceUnavailable("host presence unavailable")
        })?;

    ws::start(
        HostWs {
            host_public_id: host.public_id,
            pool: pool.get_ref().clone(),
            last_client_heartbeat: Instant::now(),
        },
        &req,
        stream,
    )
}
