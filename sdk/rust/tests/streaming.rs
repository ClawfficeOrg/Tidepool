//! The blocking chatroom socket yields typed frames from a real handshake.

#![cfg(feature = "blocking")]

use std::net::TcpListener;
use std::thread;

use tidepool_client::blocking::Client;
use tidepool_client::endpoints::GetV1ChatroomsRoomStream;
use tidepool_client::stream::ChatFrame;
use tungstenite::Message;

const MESSAGE: &str = r#"{"id":"msg_1","project":"proj_1","body":"hello","created_at":"2026-10-29T00:00:00Z","sequence":8,"author":{"id":"session_1","kind":"session"}}"#;

#[test]
fn the_stream_yields_history_current_and_live_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut socket = tungstenite::accept(stream).expect("websocket handshake");
        for text in [
            format!(r#"{{"type":"chat.history","catchUp":true,"messages":[{MESSAGE}]}}"#),
            r#"{"type":"chat.current","catchUp":false,"cursor":8}"#.to_owned(),
            format!(r#"{{"type":"chat.message","catchUp":false,"message":{MESSAGE}}}"#),
            r#"{"type":"chat.presence","watchers":["user_1"]}"#.to_owned(),
        ] {
            socket.send(Message::Text(text)).expect("send frame");
        }
        let _ = socket.close(None);
    });

    let client = Client::from_base_url(format!("http://{address}"))
        .expect("client")
        .with_token("jwt");
    let stream = client
        .stream(&GetV1ChatroomsRoomStream::new("room_1"))
        .expect("stream handle");
    assert_eq!(
        stream.url(),
        format!("ws://{address}/v1/chatrooms/room_1/stream?token=jwt")
    );

    let mut frames_iter = stream.connect().expect("connect");
    let frames: Vec<ChatFrame> = frames_iter
        .by_ref()
        .map(|frame| frame.expect("frame"))
        .collect();
    handle.join().expect("server");

    assert_eq!(frames.len(), 4);
    assert_eq!(frames[0].frame_type(), "chat.history");
    match &frames[0] {
        ChatFrame::History { messages, .. } => assert_eq!(messages[0].id, "msg_1"),
        other => panic!("unexpected frame {other:?}"),
    }
    assert_eq!(frames[1].cursor(), Some(8));
    assert_eq!(frames[2].cursor(), None, "a live message is not a cursor");
    match &frames[2] {
        ChatFrame::Message { message, .. } => assert_eq!(message.sequence, 8),
        other => panic!("unexpected frame {other:?}"),
    }
    assert_eq!(
        frames_iter.cursor(),
        Some(8),
        "a live message still advances the resume point"
    );
    match &frames[3] {
        ChatFrame::Presence { watchers, .. } => assert_eq!(watchers, &["user_1"]),
        other => panic!("unexpected frame {other:?}"),
    }
}
