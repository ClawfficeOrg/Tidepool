"""The live-stream handle and typed frame parsing."""

from __future__ import annotations

import json

import support
from tidepool_client import ChatFrame, ChatStream
from tidepool_client._generated.models import MessageView


def message_body():
    body = support.sample_schema(support.SCHEMAS["MessageView"])
    body["id"] = "msg_1"
    body["sequence"] = 4
    return body


def test_history_frame_carries_typed_messages():
    frame = ChatFrame.parse(
        json.dumps(
            {
                "type": "chat.history",
                "catchUp": True,
                "messages": [message_body()],
            }
        )
    )
    assert frame.kind == "chat.history"
    assert frame.catch_up is True
    assert len(frame.messages) == 1
    assert isinstance(frame.messages[0], MessageView)
    assert frame.messages[0].id == "msg_1"


def test_current_message_and_presence_frames():
    current = ChatFrame.parse(json.dumps({"type": "chat.current", "cursor": 9}))
    assert current.kind == "chat.current"
    assert current.cursor == 9

    message = ChatFrame.parse(
        json.dumps({"type": "chat.message", "message": message_body()})
    )
    assert message.kind == "chat.message"
    assert message.message.sequence == 4

    presence = ChatFrame.parse(
        json.dumps({"type": "chat.presence", "watchers": ["a", "b", 3]})
    )
    assert presence.kind == "chat.presence"
    assert presence.watchers == ("a", "b")

    catch_up = ChatFrame.parse(json.dumps({"type": "chat.catchUp"}))
    assert catch_up.kind == "chat.catchUp"


def test_error_and_unknown_frames_do_not_raise():
    error = ChatFrame.parse(
        json.dumps({"type": "chat.error", "code": "forbidden", "message": "no"})
    )
    assert error.kind == "chat.error"
    assert error.code == "forbidden"
    assert error.error_message == "no"

    unknown = ChatFrame.parse(json.dumps({"type": "chat.future", "x": 1}))
    assert unknown.kind == "unknown"
    assert unknown.raw["x"] == 1

    no_type = ChatFrame.parse(json.dumps({"x": 1}))
    assert no_type.kind == "unknown"

    not_json = ChatFrame.parse("plain text")
    assert not_json.kind == "unknown"
    assert not_json.raw == "plain text"

    malformed_history = ChatFrame.parse(json.dumps({"type": "chat.history", "messages": "x"}))
    assert malformed_history.kind == "unknown"


def test_binary_frames_are_wrapped():
    frame = ChatFrame.binary(b"\x00\x01")
    assert frame.kind == "binary"
    assert frame.raw == b"\x00\x01"


def test_stream_repr_does_not_include_the_token():
    stream = ChatStream(url="wss://pool.example.com/v1/chatrooms/room_1/stream", token="secret")
    assert "secret" not in repr(stream)
