"""Cursor paging across chat, relay and activity."""

from __future__ import annotations

import asyncio

import support
from tidepool_client import (
    AsyncClient,
    SyncClient,
    aiter_activity,
    aiter_chat_messages,
    aiter_relay_events,
    iter_activity,
    iter_chat_messages,
    iter_relay_events,
)


def message(sequence: int):
    body = support.sample_schema(support.SCHEMAS["MessageView"])
    body["sequence"] = sequence
    body["id"] = f"msg_{sequence}"
    return body


def event(sequence: int):
    body = support.sample_schema(support.SCHEMAS["RelayEvent"])
    body["sequence"] = sequence
    body["id"] = f"evt_{sequence}"
    return body


def entry(entry_id: str):
    body = support.sample_schema(support.SCHEMAS["ActivityEntry"])
    body["id"] = entry_id
    return body


def chat_responder(request):
    after = request.query.get("after")
    if after is None:
        payload = {"messages": [message(1)], "next": 2}
    else:
        assert after == "2"
        payload = {"messages": [message(2)], "next": None}
    return support.json_response(200, payload)


def relay_responder(request):
    after = request.query.get("after")
    if after is None:
        payload = {"events": [event(1)], "next": 2}
    else:
        assert after == "2"
        payload = {"events": [event(2)], "next": None}
    return support.json_response(200, payload)


def activity_responder(request):
    cursor = request.query.get("cursor")
    if cursor is None:
        payload = {"items": [entry("a1")], "hasMore": True, "nextCursor": "c2"}
    else:
        assert cursor == "c2"
        payload = {"items": [entry("a2")], "hasMore": False, "nextCursor": None}
    return support.json_response(200, payload)


def test_chat_messages_follow_next():
    with support.RecordedServer(chat_responder) as server:
        client = SyncClient(server.base_url)
        messages = list(iter_chat_messages(client, "room_1", limit=1))
    assert [item.id for item in messages] == ["msg_1", "msg_2"]
    assert [request.query.get("after") for request in server.requests] == [None, "2"]


def test_relay_events_follow_next():
    with support.RecordedServer(relay_responder) as server:
        client = SyncClient(server.base_url)
        events = list(iter_relay_events(client, "session_1"))
    assert [item.id for item in events] == ["evt_1", "evt_2"]
    assert len(server.requests) == 2


def test_activity_follows_next_cursor_and_passes_filters():
    with support.RecordedServer(activity_responder) as server:
        client = SyncClient(server.base_url)
        items = list(iter_activity(client, project="proj_1", limit=5))
    assert [item.id for item in items] == ["a1", "a2"]
    assert server.requests[0].query["project"] == "proj_1"
    assert server.requests[0].query["limit"] == "5"


def test_max_pages_bounds_a_server_that_keeps_offering_a_cursor():
    def responder(request):
        cursor = int(request.query.get("cursor") or 0)
        return support.json_response(
            200,
            {
                "items": [entry(f"a{cursor}")],
                "hasMore": True,
                "nextCursor": str(cursor + 1),
            },
        )

    with support.RecordedServer(responder) as server:
        client = SyncClient(server.base_url)
        items = list(iter_activity(client, max_pages=2))
    assert len(items) == 2
    assert len(server.requests) == 2


def test_async_paging_mirrors_the_sync_iterators():
    async def scenario(chat_server, relay_server, activity_server):
        client = AsyncClient(chat_server.base_url)
        chat = [item.id async for item in aiter_chat_messages(client, "room_1")]
        relay_client = AsyncClient(relay_server.base_url)
        relay = [item.id async for item in aiter_relay_events(relay_client, "session_1")]
        activity_client = AsyncClient(activity_server.base_url)
        activity = [item.id async for item in aiter_activity(activity_client)]
        return chat, relay, activity

    with support.RecordedServer(chat_responder) as chat_server:
        with support.RecordedServer(relay_responder) as relay_server:
            with support.RecordedServer(activity_responder) as activity_server:
                chat, relay, activity = asyncio.run(
                    scenario(chat_server, relay_server, activity_server)
                )
    assert chat == ["msg_1", "msg_2"]
    assert relay == ["evt_1", "evt_2"]
    assert activity == ["a1", "a2"]
