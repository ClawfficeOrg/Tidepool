//! In-memory room fan-out for the chatroom live stream (task 1.5.3).
//!
//! The hub tracks which connections are watching which room and forwards live
//! messages and presence changes to them. Presence is **not** delivery: a
//! message is always readable from the database by cursor, and the hub never
//! decides whether a message exists. A dropped or stale presence entry can
//! therefore lose at most a notification, never a message.
//!
//! Fan-out is keyed by the room's public id, so the socket layer and the HTTP
//! layer agree on identity.

use actix::prelude::*;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;

/// Upper bound of live connections per room. Exceeding it prunes dead
/// recipients instead of refusing the join.
const MAX_CONNS_PER_ROOM: usize = 1_024;

/// Something the hub pushes to one watching connection.
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub enum RoomEvent {
    /// A live message envelope, already serialized as the client's JSON shape.
    Message(Value),
    /// The current watchers of the room, sorted by label.
    Presence(Vec<String>),
}

/// Register a connection as watching a room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct JoinRoom {
    /// Room public id.
    pub room: String,
    /// Unique connection key, used to unregister later.
    pub conn_key: usize,
    /// Human-readable watcher label (the member's public id).
    pub watcher: String,
    /// Recipient for subsequent events.
    pub recipient: Recipient<RoomEvent>,
}

/// Unregister a connection from its room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct LeaveRoom {
    /// Room public id.
    pub room: String,
    /// Connection key that is leaving.
    pub conn_key: usize,
}

/// Publish one message to everyone watching a room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastMessage {
    /// Room public id.
    pub room: String,
    /// Message envelope.
    pub payload: Value,
}

/// Ask who is currently watching a room.
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct RoomWatchers {
    /// Room public id.
    pub room: String,
}

/// Actor owning the room registry.
#[derive(Default)]
pub struct RoomHub {
    /// Connection key -> (watcher label, recipient), per room.
    rooms: HashMap<String, HashMap<usize, (String, Recipient<RoomEvent>)>>,
}

impl RoomHub {
    /// Create an empty hub.
    pub fn new() -> Self {
        Self::default()
    }

    fn watchers(&self, room: &str) -> Vec<String> {
        let mut labels: Vec<String> = self
            .rooms
            .get(room)
            .map(|conns| conns.values().map(|(label, _)| label.clone()).collect())
            .unwrap_or_default();
        labels.sort();
        labels
    }

    fn prune_dead(&mut self, room: &str) {
        if let Some(conns) = self.rooms.get_mut(room) {
            conns.retain(|_, (_, recipient)| recipient.connected());
            if conns.is_empty() {
                self.rooms.remove(room);
            }
        }
    }

    fn broadcast_presence(&self, room: &str) {
        if let Some(conns) = self.rooms.get(room) {
            let watchers = self.watchers(room);
            for (_, recipient) in conns.values() {
                recipient.do_send(RoomEvent::Presence(watchers.clone()));
            }
        }
    }
}

impl Actor for RoomHub {
    type Context = Context<Self>;
}

impl Handler<JoinRoom> for RoomHub {
    type Result = ();

    fn handle(&mut self, msg: JoinRoom, _ctx: &mut Self::Context) {
        let full = self
            .rooms
            .get(&msg.room)
            .is_some_and(|conns| conns.len() >= MAX_CONNS_PER_ROOM);
        if full {
            self.prune_dead(&msg.room);
        }
        if self
            .rooms
            .entry(msg.room.clone())
            .or_default()
            .insert(msg.conn_key, (msg.watcher, msg.recipient))
            .is_none()
        {
            crate::middleware::metrics::metrics().chatroom_viewer_connected();
        }
        self.broadcast_presence(&msg.room);
    }
}

impl Handler<LeaveRoom> for RoomHub {
    type Result = ();

    fn handle(&mut self, msg: LeaveRoom, _ctx: &mut Self::Context) {
        if let Some(conns) = self.rooms.get_mut(&msg.room)
            && conns.remove(&msg.conn_key).is_some()
        {
            crate::middleware::metrics::metrics().chatroom_viewer_disconnected();
            if conns.is_empty() {
                self.rooms.remove(&msg.room);
            }
        }
        self.broadcast_presence(&msg.room);
    }
}

impl Handler<BroadcastMessage> for RoomHub {
    type Result = ();

    fn handle(&mut self, msg: BroadcastMessage, _ctx: &mut Self::Context) {
        let Some(conns) = self.rooms.get(&msg.room) else {
            return;
        };
        for (_, recipient) in conns.values() {
            recipient.do_send(RoomEvent::Message(msg.payload.clone()));
        }
    }
}

impl Handler<RoomWatchers> for RoomHub {
    type Result = Vec<String>;

    fn handle(&mut self, msg: RoomWatchers, _ctx: &mut Self::Context) -> Self::Result {
        self.watchers(&msg.room)
    }
}

/// The process-wide room hub.
pub fn hub() -> Addr<RoomHub> {
    static HUB: Lazy<Addr<RoomHub>> = Lazy::new(|| RoomHub::new().start());
    HUB.clone()
}

/// Publish a message envelope to a room without waiting for the hub.
pub fn broadcast_message(room: &str, payload: Value) {
    hub().do_send(BroadcastMessage {
        room: room.to_string(),
        payload,
    });
}

/// True when a live message is newer than everything the client already has.
///
/// The catch-up read and the live stream can overlap; a message at or below the
/// last delivered sequence is a duplicate and is dropped.
pub fn should_forward(sequence: u64, last_delivered: Option<u64>) -> bool {
    match last_delivered {
        Some(last) => sequence > last,
        None => true,
    }
}

/// The envelope a catching-up client receives for a page of history.
pub fn history_envelope(messages: &[crate::entities::chatroom::MessageView]) -> Value {
    serde_json::json!({
        "type": "chat.history",
        "catchUp": true,
        "messages": messages,
    })
}

/// The envelope that marks the end of catch-up.
pub fn current_envelope(cursor: Option<u64>) -> Value {
    serde_json::json!({
        "type": "chat.current",
        "catchUp": false,
        "cursor": cursor,
    })
}

/// The envelope for one live message.
pub fn message_envelope(message: &crate::entities::chatroom::MessageView) -> Value {
    serde_json::json!({
        "type": "chat.message",
        "catchUp": false,
        "message": message,
    })
}

/// The envelope for a presence change.
pub fn presence_envelope(watchers: &[String]) -> Value {
    serde_json::json!({
        "type": "chat.presence",
        "catchUp": false,
        "watchers": watchers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_at_or_below_the_cursor_are_dropped() {
        assert!(should_forward(5, None));
        assert!(!should_forward(5, Some(5)));
        assert!(!should_forward(4, Some(5)));
        assert!(should_forward(6, Some(5)));
    }

    #[test]
    fn catch_up_and_live_envelopes_are_distinguishable() {
        let history = history_envelope(&[]);
        assert_eq!(history["type"], "chat.history");
        assert_eq!(history["catchUp"], true);

        let current = current_envelope(Some(7));
        assert_eq!(current["type"], "chat.current");
        assert_eq!(current["catchUp"], false);
        assert_eq!(current["cursor"], 7);

        let presence = presence_envelope(&["a".to_string()]);
        assert_eq!(presence["type"], "chat.presence");
        assert_eq!(presence["catchUp"], false);
    }
}
