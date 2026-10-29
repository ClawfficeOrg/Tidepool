"""The chatroom live stream: the typed socket handle and frame parsing.

``GET /v1/chatrooms/{room}/stream`` upgrades to a WebSocket. The socket first
replays ``chat.history`` frames for everything newer than ``after``, then one
``chat.current`` frame marks the end of catch-up; live ``chat.message`` and
``chat.presence`` frames follow. ``chat.error`` closes the socket.

This module owns the URL and the frame shapes. It does not open a socket, so a
caller on either facade can hand the URL to the WebSocket library it already
uses; both clients return the same typed handle.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Optional, Tuple

from ._generated.models import MessageView


@dataclass(frozen=True)
class ChatStream:
    """A typed handle for one room's live stream.

    The URL carries the operator token in its query string, because a socket
    client cannot always set an ``Authorization`` header and the pool expects
    the token there. Treat the whole URL as a secret.
    """

    url: str
    """The absolute ``ws://`` or ``wss://`` URL, token included."""
    token: str = field(repr=False)
    """The operator token the URL carries."""
    after: Optional[int] = None
    """The sequence the server was asked to resume after."""


@dataclass(frozen=True)
class ChatFrame:
    """One frame the chatroom socket can send.

    ``kind`` is the envelope's ``type`` string, or ``"unknown"`` / ``"binary"``
    for a frame this SDK does not name. A caller can switch on it and ignore the
    fields that do not apply.
    """

    kind: str
    catch_up: bool = False
    messages: Tuple[MessageView, ...] = ()
    cursor: Optional[int] = None
    message: Optional[MessageView] = None
    watchers: Tuple[str, ...] = ()
    code: Optional[str] = None
    error_message: Optional[str] = None
    raw: Any = None

    @classmethod
    def parse(cls, text: str) -> "ChatFrame":
        """Parse one text payload into a typed frame.

        Non-JSON text and JSON without a string ``type`` become an ``unknown``
        frame, so a caller never has to handle a parse error.

        Args:
            text: One socket text payload.

        Returns:
            The typed frame.
        """
        try:
            value = json.loads(text)
        except (ValueError, TypeError):
            return cls(kind="unknown", raw=text)
        if not isinstance(value, dict):
            return cls(kind="unknown", raw=value)
        kind = value.get("type")
        if not isinstance(kind, str):
            return cls(kind="unknown", raw=value)
        catch_up = value.get("catchUp") is True
        if kind == "chat.history":
            raw_messages = value.get("messages")
            if not isinstance(raw_messages, list):
                return cls(kind="unknown", raw=value)
            try:
                messages = tuple(
                    MessageView.from_dict(item)
                    for item in raw_messages
                    if isinstance(item, dict)
                )
            except (TypeError, ValueError):
                return cls(kind="unknown", raw=value)
            if len(messages) != len(raw_messages):
                return cls(kind="unknown", raw=value)
            return cls(
                kind="chat.history", catch_up=catch_up, messages=messages, raw=value
            )
        if kind == "chat.current":
            cursor = value.get("cursor")
            return cls(
                kind="chat.current",
                catch_up=catch_up,
                cursor=cursor if isinstance(cursor, int) else None,
                raw=value,
            )
        if kind == "chat.message":
            item = value.get("message")
            if not isinstance(item, dict):
                return cls(kind="unknown", raw=value)
            try:
                message = MessageView.from_dict(item)
            except (TypeError, ValueError):
                return cls(kind="unknown", raw=value)
            return cls(kind="chat.message", catch_up=catch_up, message=message, raw=value)
        if kind == "chat.presence":
            watchers = value.get("watchers")
            return cls(
                kind="chat.presence",
                catch_up=catch_up,
                watchers=tuple(
                    item for item in watchers if isinstance(item, str)
                )
                if isinstance(watchers, list)
                else (),
                raw=value,
            )
        if kind == "chat.catchUp":
            return cls(kind="chat.catchUp", catch_up=catch_up, raw=value)
        if kind == "chat.error":
            code = value.get("code")
            message = value.get("message")
            return cls(
                kind="chat.error",
                catch_up=catch_up,
                code=code if isinstance(code, str) else None,
                error_message=message if isinstance(message, str) else None,
                raw=value,
            )
        return cls(kind="unknown", raw=value)

    @classmethod
    def binary(cls, payload: bytes) -> "ChatFrame":
        """Wrap one binary payload as a frame."""
        return cls(kind="binary", raw=payload)
