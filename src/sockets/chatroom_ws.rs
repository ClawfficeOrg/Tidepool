//! Chatroom live stream (task 1.5.3).
//!
//! `GET /v1/chatrooms/{room}/stream?token=<jwt>&after=<sequence>` upgrades to
//! a WebSocket. The token is a normal operator JWT carried in the query string
//! because a browser WebSocket cannot set an `Authorization` header.
//!
//! The connection is refused unless the caller is a member of the room; a
//! non-member gets the same 404 as a missing room. On connect the socket:
//!
//! 1. joins the room hub, so live messages are not missed while catching up;
//! 2. replays history after `after` as `chat.history` frames with
//!    `catchUp: true`;
//! 3. sends `chat.current` once there is nothing newer, which is the client's
//!    signal that it is live rather than catching up.
//!
//! Live messages carry the same `sequence` the HTTP API returns, so a
//! reconnecting client resumes from its cursor and sees each message once:
//! a live frame at or below the last delivered sequence is dropped.

use crate::config::Config;
use crate::entities::chatroom::{MessageView, Principal};
use crate::services::auth as auth_svc;
use crate::services::chatrooms as chatroom_svc;
use crate::services::pool_identity;
use crate::sockets::rooms::{self, JoinRoom, LeaveRoom, RoomEvent};
use actix::prelude::*;
use actix_web::{Error, HttpRequest, HttpResponse, web};
use actix_web_actors::ws;
use serde::Deserialize;
use sqlx::MySqlPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Heartbeat interval for the socket.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// Client silence after which the connection is dropped.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);
/// Page size used while replaying a backlog.
const CATCH_UP_PAGE: u32 = 200;

static CONN_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Query parameters of the stream endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamQuery {
    /// Operator JWT.
    pub token: String,
    /// Only replay messages newer than this sequence.
    pub after: Option<u64>,
}

/// One live chatroom connection.
pub struct ChatRoomConn {
    /// Room public id.
    room: String,
    /// Connection key in the room hub.
    conn_key: usize,
    /// The member's public id, used as the watcher label.
    watcher: String,
    /// Organization the member belongs to.
    organization_id: u64,
    /// The member principal, used for membership re-checks.
    principal: Principal,
    /// Database handle for catch-up reads.
    pool: web::Data<MySqlPool>,
    /// Highest sequence delivered to this client.
    last_sequence: Option<u64>,
    /// Last heartbeat received.
    heartbeat: Instant,
}

impl ChatRoomConn {
    fn send_envelope(&self, ctx: &mut ws::WebsocketContext<Self>, payload: &serde_json::Value) {
        ctx.text(payload.to_string());
    }

    /// Deliver a live message unless the client already has it.
    fn deliver(&mut self, ctx: &mut ws::WebsocketContext<Self>, message: &MessageView) {
        if !rooms::should_forward(message.sequence, self.last_sequence) {
            return;
        }
        self.last_sequence = Some(message.sequence);
        self.send_envelope(ctx, &rooms::message_envelope(message));
    }
}

impl Actor for ChatRoomConn {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address();
        rooms::hub().do_send(JoinRoom {
            room: self.room.clone(),
            conn_key: self.conn_key,
            watcher: self.watcher.clone(),
            recipient: addr.recipient(),
        });

        ctx.run_interval(HEARTBEAT, |act, ctx| {
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });

        // Replay the backlog page by page, then announce that the client is
        // current. Live frames that arrive meanwhile are deduplicated against
        // `last_sequence`.
        let pool = self.pool.clone();
        let organization_id = self.organization_id;
        let principal = self.principal.clone();
        let room = self.room.clone();
        let mut cursor = self.last_sequence;
        ctx.spawn(
            async move {
                let mut delivered: Vec<MessageView> = Vec::new();
                loop {
                    let page = chatroom_svc::history_as(
                        pool.get_ref(),
                        organization_id,
                        &room,
                        &principal,
                        cursor,
                        Some(CATCH_UP_PAGE),
                    )
                    .await
                    .map_err(|error| error.to_string())?;

                    delivered.extend(page.messages);
                    cursor = page.next;
                    if cursor.is_none() {
                        return Ok::<_, String>(delivered);
                    }
                }
            }
            .into_actor(self)
            .map(|result, act, ctx| match result {
                Ok(messages) => {
                    for chunk in messages.chunks(CATCH_UP_PAGE as usize) {
                        act.send_envelope(ctx, &rooms::history_envelope(chunk));
                        if let Some(last) = chunk.last() {
                            act.last_sequence = Some(last.sequence);
                        }
                    }
                    act.send_envelope(ctx, &rooms::current_envelope(act.last_sequence));
                }
                Err(error) => {
                    tracing::warn!(%error, "chatroom catch-up failed");
                    ctx.text(
                        serde_json::json!({
                            "type": "chat.error",
                            "message": "catch-up failed",
                        })
                        .to_string(),
                    );
                    ctx.stop();
                }
            }),
        );
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        rooms::hub().do_send(LeaveRoom {
            room: self.room.clone(),
            conn_key: self.conn_key,
        });
        Running::Stop
    }
}

impl Handler<RoomEvent> for ChatRoomConn {
    type Result = ();

    fn handle(&mut self, event: RoomEvent, ctx: &mut Self::Context) {
        match event {
            RoomEvent::Message(payload) => {
                let Some(message) = payload.get("message") else {
                    return;
                };
                if let Ok(message) = serde_json::from_value::<MessageView>(message.clone()) {
                    self.deliver(ctx, &message);
                }
            }
            RoomEvent::Presence(watchers) => {
                self.send_envelope(ctx, &rooms::presence_envelope(&watchers));
            }
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatRoomConn {
    fn handle(&mut self, message: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match message {
            Ok(ws::Message::Ping(bytes)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&bytes);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.heartbeat = Instant::now();
                // Clients may ask for anything newer than a cursor without
                // reconnecting; anything else is ignored.
                if let Ok(frame) = serde_json::from_str::<serde_json::Value>(&text)
                    && frame.get("type").and_then(|value| value.as_str()) == Some("chat.fetch")
                {
                    // A fetch is answered by the HTTP endpoint; tell the
                    // client it is not current until that read returns.
                    ctx.text(
                        serde_json::json!({"type": "chat.catchUp", "catchUp": true}).to_string(),
                    );
                }
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            Ok(_) => {}
            Err(_) => ctx.stop(),
        }
    }
}

/// Upgrade a member's connection to a room stream.
pub async fn stream(
    req: HttpRequest,
    payload: web::Payload,
    room: web::Path<String>,
    query: web::Query<StreamQuery>,
    pool: Option<web::Data<MySqlPool>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Error> {
    let Some(pool) = pool else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };
    let claims = auth_svc::verify_jwt(&config.jwt_secret, &query.token)
        .map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;
    let user = pool_identity::find_by_public_id(pool.get_ref(), &claims.sub)
        .await
        .map_err(|_| actix_web::error::ErrorInternalServerError("identity lookup failed"))?
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("unknown member"))?;

    let principal = Principal::User(user.public_id.clone());
    chatroom_svc::member_role(pool.get_ref(), user.organization_id, &room, &principal)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("chatroom not found"))?;

    let conn = ChatRoomConn {
        room: room.into_inner(),
        conn_key: CONN_SEQ.fetch_add(1, Ordering::Relaxed),
        watcher: user.public_id,
        organization_id: user.organization_id,
        principal,
        pool,
        last_sequence: query.after,
        heartbeat: Instant::now(),
    };
    ws::start(conn, &req, payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_is_exclusive() {
        assert!(rooms::should_forward(11, Some(10)));
        assert!(!rooms::should_forward(10, Some(10)));
    }
}
