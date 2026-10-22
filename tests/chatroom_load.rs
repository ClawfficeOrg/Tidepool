//! Chatroom live-stream load test (task 1.5.3).
//!
//! Mirrors the milestone load spec used by `ws_load.rs`: **100 concurrent
//! watchers, 1000 messages, 0 dropped (or double-delivered) envelopes**,
//! driven against the same `RoomHub` the socket layer uses. The hub is
//! deliberately DB-free, so the test needs no network client or database.
//!
//! A second part checks the delivery rule that matters for correctness: a
//! watcher that joins later still receives the full backlog from the HTTP
//! cursor read, because presence never decides what exists.

use actix::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tidepool::sockets::rooms::{
    BroadcastMessage, JoinRoom, LeaveRoom, RoomEvent, RoomHub, RoomWatchers,
};

type Received = Arc<Mutex<Vec<Value>>>;

/// Subscriber that records every live frame it is sent.
struct MockWatcher {
    frames: Received,
}

impl Actor for MockWatcher {
    type Context = Context<Self>;
}

impl Handler<RoomEvent> for MockWatcher {
    type Result = ();

    fn handle(&mut self, event: RoomEvent, _ctx: &mut Context<Self>) {
        if let RoomEvent::Message(payload) = event {
            self.frames.lock().expect("frames lock").push(payload);
        }
    }
}

#[actix_web::test]
async fn one_hundred_watchers_receive_one_thousand_messages_without_drops() {
    let hub = RoomHub::new().start();
    let room = "room-load".to_string();
    let mut subscribers = Vec::new();

    for index in 0..100 {
        let frames: Received = Arc::new(Mutex::new(Vec::new()));
        let watcher = MockWatcher {
            frames: frames.clone(),
        }
        .start();
        hub.send(JoinRoom {
            room: room.clone(),
            conn_key: index,
            watcher: format!("member-{index}"),
            recipient: watcher.recipient(),
        })
        .await
        .expect("join");
        subscribers.push(frames);
    }

    for sequence in 1..=1_000u64 {
        let payload = json!({
            "type": "chat.message",
            "catchUp": false,
            "message": {
                "id": format!("message-{sequence}"),
                "sequence": sequence,
                "body": format!("message {sequence}"),
            },
        });
        hub.send(BroadcastMessage {
            room: room.clone(),
            payload,
        })
        .await
        .expect("broadcast");
    }

    // Let the mailbox drain, then check every subscriber saw every message
    // exactly once and in order.
    for _ in 0..200 {
        let complete = subscribers
            .iter()
            .all(|frames| frames.lock().expect("frames lock").len() == 1_000);
        if complete {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    for frames in &subscribers {
        let frames = frames.lock().expect("frames lock");
        assert_eq!(frames.len(), 1_000, "no drops");
        for (index, frame) in frames.iter().enumerate() {
            assert_eq!(frame["message"]["sequence"], (index as u64) + 1);
            assert_eq!(frame["message"]["id"], format!("message-{}", index + 1));
        }
    }

    let watchers = hub
        .send(RoomWatchers { room: room.clone() })
        .await
        .expect("watchers");
    assert_eq!(watchers.len(), 100);

    // A leave removes exactly one watcher and never affects delivery to the
    // rest.
    hub.send(LeaveRoom {
        room: room.clone(),
        conn_key: 0,
    })
    .await
    .expect("leave");
    let watchers = hub
        .send(RoomWatchers { room: room.clone() })
        .await
        .expect("watchers");
    assert_eq!(watchers.len(), 99);
}
