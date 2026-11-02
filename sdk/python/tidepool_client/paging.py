"""Cursor paging helpers over the contract's page shapes.

The hub returns ``next`` (a sequence cursor) for chat and relay history and
``nextCursor``/``hasMore`` for activity. These iterators follow the cursor until
the server stops offering one, so a caller never hand-rolls the loop.

Both a synchronous and an asynchronous iterator are provided; each issues one
page at a time through the generated resource surface.
"""

from __future__ import annotations

from typing import Any, AsyncIterator, Iterator, Optional

from ._generated.models import ActivityEntry, MessageView, RelayEvent
from .client import AsyncClient, SyncClient


def iter_chat_messages(
    client: SyncClient,
    room: str,
    *,
    after: Optional[int] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
) -> Iterator[MessageView]:
    """Iterate a room's messages in sequence order.

    Args:
        client: The sync client.
        room: Room id or slug.
        after: Sequence to resume after.
        limit: Page size the server should use.
        max_pages: Stop after this many pages; ``None`` follows to the end.

    Yields:
        One :class:`MessageView` per message, oldest first.
    """
    pages = 0
    cursor = after
    while True:
        page = client.chatrooms.messages(room, after=cursor, limit=limit)
        for message in page.messages:
            yield message
        pages += 1
        following = page.next
        if following is None or following == cursor or (max_pages is not None and pages >= max_pages):
            return
        cursor = following


async def aiter_chat_messages(
    client: AsyncClient,
    room: str,
    *,
    after: Optional[int] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
) -> AsyncIterator[MessageView]:
    """Asynchronous form of :func:`iter_chat_messages`."""
    pages = 0
    cursor = after
    while True:
        page = await client.chatrooms.messages(room, after=cursor, limit=limit)
        for message in page.messages:
            yield message
        pages += 1
        following = page.next
        if following is None or following == cursor or (max_pages is not None and pages >= max_pages):
            return
        cursor = following


def iter_relay_events(
    client: SyncClient,
    session: str,
    *,
    after: Optional[int] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
) -> Iterator[RelayEvent]:
    """Iterate one session relay's events in sequence order."""
    pages = 0
    cursor = after
    while True:
        page = client.relay.events(session, after=cursor, limit=limit)
        for event in page.events:
            yield event
        pages += 1
        following = page.next
        if following is None or following == cursor or (max_pages is not None and pages >= max_pages):
            return
        cursor = following


async def aiter_relay_events(
    client: AsyncClient,
    session: str,
    *,
    after: Optional[int] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
) -> AsyncIterator[RelayEvent]:
    """Asynchronous form of :func:`iter_relay_events`."""
    pages = 0
    cursor = after
    while True:
        page = await client.relay.events(session, after=cursor, limit=limit)
        for event in page.events:
            yield event
        pages += 1
        following = page.next
        if following is None or following == cursor or (max_pages is not None and pages >= max_pages):
            return
        cursor = following


def iter_activity(
    client: SyncClient,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
    **filters: Any,
) -> Iterator[ActivityEntry]:
    """Iterate the activity feed, following ``nextCursor``.

    Args:
        client: The sync client.
        cursor: Opaque cursor to resume from.
        limit: Page size the server should use.
        max_pages: Stop after this many pages; ``None`` follows to the end.
        **filters: ``project``, ``provider``, ``trigger``, ``status`` or ``kind``.

    Yields:
        One :class:`ActivityEntry` per feed item.
    """
    pages = 0
    current = cursor
    while True:
        page = client.activity.list(cursor=current, limit=limit, **filters)
        for item in page.items:
            yield item
        pages += 1
        following = page.next_cursor
        if (
            not page.has_more
            or following is None
            or following == current
            or (max_pages is not None and pages >= max_pages)
        ):
            return
        current = following


async def aiter_activity(
    client: AsyncClient,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    max_pages: Optional[int] = None,
    **filters: Any,
) -> AsyncIterator[ActivityEntry]:
    """Asynchronous form of :func:`iter_activity`."""
    pages = 0
    current = cursor
    while True:
        page = await client.activity.list(cursor=current, limit=limit, **filters)
        for item in page.items:
            yield item
        pages += 1
        following = page.next_cursor
        if (
            not page.has_more
            or following is None
            or following == current
            or (max_pages is not None and pages >= max_pages)
        ):
            return
        current = following
