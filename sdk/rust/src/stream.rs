//! The chatroom live stream: the typed socket handle and frame parsing.
//!
//! `GET /v1/chatrooms/{room}/stream?token=<jwt>&after=<sequence>` upgrades to a
//! WebSocket. The socket first replays `chat.history` frames for everything
//! newer than `after`, then one `chat.current` frame marks the end of catch-up;
//! live `chat.message` and `chat.presence` frames follow. `chat.error` closes
//! the socket.
//!
//! The blocking client can drive this socket directly with
//! [`crate::blocking::ChatStreamExt::connect`]. The async client returns the
//! same typed handle and leaves the socket to the caller's WebSocket library,
//! so this crate does not force a second async runtime dependency on anyone.

use crate::generated::types::{JsonValue, MessageView};

/// A typed handle for one room's live stream.
///
/// The URL carries the operator token in its query string, because a browser
/// socket cannot set an `Authorization` header and the pool expects the token
/// there. Treat the whole URL as a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatStream {
    url: String,
    token: String,
    after: Option<u64>,
}

impl ChatStream {
    /// Build a handle from an already-encoded `ws://` or `wss://` URL.
    pub(crate) fn new(url: String, token: String, after: Option<u64>) -> Self {
        Self { url, token, after }
    }

    /// The absolute `ws://` or `wss://` URL, token included.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The operator token the URL carries.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The sequence the server was asked to resume after.
    pub fn after(&self) -> Option<u64> {
        self.after
    }
}

/// One frame the chatroom socket can send.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatFrame {
    /// A page of replayed history, newer than the requested cursor.
    History {
        /// True during catch-up.
        catch_up: bool,
        /// The replayed messages, oldest first.
        messages: Vec<MessageView>,
    },
    /// The end of catch-up; `cursor` is where live frames resume.
    Current {
        /// Always false in practice; carried for parity with the envelope.
        catch_up: bool,
        /// Resume point after a drop.
        cursor: Option<u64>,
    },
    /// One live message.
    Message {
        /// True during catch-up.
        catch_up: bool,
        /// The message, with its server id and sequence.
        message: MessageView,
    },
    /// A presence change: the members currently watching the room.
    Presence {
        /// True during catch-up.
        catch_up: bool,
        /// Watcher identifiers.
        watchers: Vec<String>,
    },
    /// The client asked the pool to catch up (`chat.fetch`).
    CatchUp {
        /// True while catching up.
        catch_up: bool,
    },
    /// The pool refused or ended the stream.
    Error {
        /// Machine-readable error code, when one was sent.
        code: Option<String>,
        /// Human-readable explanation, when one was sent.
        message: Option<String>,
        /// The whole frame, for diagnostics.
        raw: JsonValue,
    },
    /// A frame whose `type` this SDK does not know.
    Unknown(JsonValue),
    /// A binary frame.
    Binary(Vec<u8>),
}

impl ChatFrame {
    /// Parse one text payload into a typed frame.
    ///
    /// Non-JSON text and JSON without a string `type` become
    /// [`ChatFrame::Unknown`], so a caller never has to handle a parse error.
    pub fn parse(text: &str) -> Self {
        let Ok(value) = serde_json::from_str::<JsonValue>(text) else {
            return Self::Unknown(JsonValue::String(text.to_owned()));
        };
        let Some(kind) = value.get("type").and_then(JsonValue::as_str) else {
            return Self::Unknown(value);
        };
        let catch_up = value
            .get("catchUp")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        match kind {
            "chat.history" => {
                let messages = value
                    .get("messages")
                    .cloned()
                    .and_then(|messages| serde_json::from_value::<Vec<MessageView>>(messages).ok());
                match messages {
                    Some(messages) => Self::History { catch_up, messages },
                    None => Self::Unknown(value),
                }
            }
            "chat.current" => Self::Current {
                catch_up,
                cursor: value.get("cursor").and_then(JsonValue::as_u64),
            },
            "chat.message" => {
                let message = value
                    .get("message")
                    .cloned()
                    .and_then(|message| serde_json::from_value::<MessageView>(message).ok());
                match message {
                    Some(message) => Self::Message { catch_up, message },
                    None => Self::Unknown(value),
                }
            }
            "chat.presence" => Self::Presence {
                catch_up,
                watchers: value
                    .get("watchers")
                    .and_then(JsonValue::as_array)
                    .map(|watchers| {
                        watchers
                            .iter()
                            .filter_map(JsonValue::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            "chat.catchUp" => Self::CatchUp { catch_up },
            "chat.error" => Self::Error {
                code: value
                    .get("code")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned),
                message: value
                    .get("message")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned),
                raw: value,
            },
            _ => Self::Unknown(value),
        }
    }

    /// The contract's frame type string.
    pub fn frame_type(&self) -> &'static str {
        match self {
            Self::History { .. } => "chat.history",
            Self::Current { .. } => "chat.current",
            Self::Message { .. } => "chat.message",
            Self::Presence { .. } => "chat.presence",
            Self::CatchUp { .. } => "chat.catchUp",
            Self::Error { .. } => "chat.error",
            Self::Unknown(_) => "unknown",
            Self::Binary(_) => "binary",
        }
    }

    /// The catch-up cursor this frame carries, when it has one.
    ///
    /// Only `chat.current` carries the resume point; a live `chat.message`
    /// exposes its own sequence on [`MessageView::sequence`] instead. This
    /// matches the other Tidepool bindings, where `cursor` is the
    /// `chat.current` field and live messages do not advance it.
    pub fn cursor(&self) -> Option<u64> {
        match self {
            Self::Current { cursor, .. } => *cursor,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE: &str = r#"{"author":{"id":"user_1","kind":"user"},"body":"hi","created_at":"2026-10-29T00:00:00Z","id":"msg_1","project":"proj_1","sequence":7}"#;

    #[test]
    fn parses_history_frames() {
        let text = format!(r#"{{"type":"chat.history","catchUp":true,"messages":[{MESSAGE}]}}"#);
        let frame = ChatFrame::parse(&text);
        assert_eq!(frame.frame_type(), "chat.history");
        match frame {
            ChatFrame::History { catch_up, messages } => {
                assert!(catch_up);
                assert_eq!(messages[0].id, "msg_1");
            }
            other => panic!("unexpected frame {other:?}"),
        }
    }

    #[test]
    fn parses_current_and_tracks_the_cursor() {
        let frame = ChatFrame::parse(r#"{"type":"chat.current","catchUp":false,"cursor":42}"#);
        assert_eq!(frame.cursor(), Some(42));
    }

    #[test]
    fn parses_live_messages_and_presence() {
        let message = ChatFrame::parse(&format!(
            r#"{{"type":"chat.message","catchUp":false,"message":{MESSAGE}}}"#
        ));
        assert_eq!(message.cursor(), None);
        match &message {
            ChatFrame::Message { message, .. } => assert_eq!(message.sequence, 7),
            other => panic!("unexpected frame {other:?}"),
        }
        let presence = ChatFrame::parse(r#"{"type":"chat.presence","watchers":["a","b"]}"#);
        match presence {
            ChatFrame::Presence { watchers, .. } => assert_eq!(watchers, vec!["a", "b"]),
            other => panic!("unexpected frame {other:?}"),
        }
    }

    #[test]
    fn unknown_and_invalid_frames_stay_opaque() {
        assert!(matches!(
            ChatFrame::parse(r#"{"type":"chat.future","x":1}"#),
            ChatFrame::Unknown(_)
        ));
        assert!(matches!(
            ChatFrame::parse("not json"),
            ChatFrame::Unknown(JsonValue::String(_))
        ));
        assert!(matches!(
            ChatFrame::parse(r#"{"no":"type"}"#),
            ChatFrame::Unknown(_)
        ));
    }

    #[test]
    fn error_frames_keep_their_code_and_body() {
        let frame = ChatFrame::parse(r#"{"type":"chat.error","code":"forbidden","message":"no"}"#);
        match frame {
            ChatFrame::Error { code, message, .. } => {
                assert_eq!(code.as_deref(), Some("forbidden"));
                assert_eq!(message.as_deref(), Some("no"));
            }
            other => panic!("unexpected frame {other:?}"),
        }
    }

    #[test]
    fn stream_handles_are_typed() {
        let stream = ChatStream::new(
            "ws://localhost/v1/chatrooms/a/stream?token=t&after=3".to_owned(),
            "t".to_owned(),
            Some(3),
        );
        assert_eq!(stream.token(), "t");
        assert_eq!(stream.after(), Some(3));
        assert!(stream.url().contains("after=3"));
    }
}
