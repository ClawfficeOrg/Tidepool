"""The asynchronous client: the same routes and behaviour without blocking."""

from __future__ import annotations

import asyncio

import pytest

import support
from tidepool_client import (
    AsyncClient,
    DecodeError,
    ProblemError,
    RetryPolicy,
    TokenUpdate,
    UsageError,
)


def run(coroutine):
    return asyncio.run(coroutine)


def test_every_contract_route_is_reachable_from_the_async_client():
    from tidepool_client import ROUTE_LIST

    table = support.coverage_table(ROUTE_LIST)

    async def scenario(server):
        client = AsyncClient(server.base_url, token="operator-token")
        for route in ROUTE_LIST:
            arguments = support.route_arguments(route)
            method = support.resolve_resource(client.resources, route)
            if route.response_kind == "stream":
                result = method(**arguments)
            else:
                result = await method(**arguments)
            support.assert_route_result(route, result)

    with support.RecordedServer(support.coverage_responder(table)) as server:
        run(scenario(server))
        assert len(server.requests) == len(table)
        for request in server.requests:
            assert (request.method, request.path) in table


def test_async_rate_limit_is_retried_then_succeeds():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    calls = {"count": 0}

    def respond(request):
        calls["count"] += 1
        if calls["count"] == 1:
            return support.problem_response(429, "rate_limited")
        return support.json_response(200, body)

    async def scenario(server):
        client = AsyncClient(
            server.base_url, retry=RetryPolicy(max_retries=2, base_delay_ms=1)
        )
        return await client.hosts.list()

    with support.RecordedServer(respond) as server:
        page = run(scenario(server))
    assert len(page.hosts) == 1
    assert calls["count"] == 2


def test_async_401_refreshes_once_with_an_awaitable_hook():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    calls = {"refresh": 0}

    def respond(request):
        if request.headers.get("authorization") == "Bearer fresh":
            return support.json_response(200, body)
        return support.problem_response(401, "unauthorized")

    async def refresh():
        calls["refresh"] += 1
        await asyncio.sleep(0)
        return TokenUpdate("fresh", 60)

    async def scenario(server):
        client = AsyncClient(server.base_url, token="stale", refresh=refresh)
        page = await client.hosts.list()
        return client, page

    with support.RecordedServer(respond) as server:
        client, page = run(scenario(server))
        assert calls["refresh"] == 1
        assert [request.headers["authorization"] for request in server.requests] == [
            "Bearer stale",
            "Bearer fresh",
        ]
        assert client.token == "fresh"
    assert page.hosts


def test_async_problem_errors_and_decode_errors_match_the_sync_client():
    async def scenario_problem(server):
        client = AsyncClient(server.base_url)
        with pytest.raises(ProblemError) as caught:
            await client.hosts.list()
        return caught.value

    with support.RecordedServer(
        lambda request: support.problem_response(403, "forbidden", "not yours")
    ) as server:
        problem = run(scenario_problem(server))
    assert problem.status == 403
    assert problem.code == "forbidden"
    assert problem.attempts == 1

    async def scenario_decode(server):
        client = AsyncClient(server.base_url)
        with pytest.raises(DecodeError):
            await client.hosts.list()

    with support.RecordedServer(
        lambda request: (200, {"Content-Type": "application/json"}, b"{")
    ) as server:
        run(scenario_decode(server))


def test_async_login_and_request_json():
    login = support.sample_schema(support.SCHEMAS["PoolLoginResponse"])
    login["token"] = "async-jwt"
    login["expires_in"] = 600

    async def scenario(server):
        client = AsyncClient(server.base_url)
        response = await client.login("owner@example.com", "password")
        raw = await client.request_json("get_v1_hosts")
        return client, response, raw

    with support.RecordedServer(lambda request: support.json_response(200, login)) as server:
        client, response, raw = run(scenario(server))
    assert response.token == "async-jwt"
    assert client.token == "async-jwt"
    assert raw["token"] == "async-jwt"


def test_async_stream_handle_and_token_requirement():
    client = AsyncClient("https://pool.example.com", token="operator")
    stream = client.chatrooms.stream("room_1", after=2)
    assert stream.url == "wss://pool.example.com/v1/chatrooms/room_1/stream?token=operator&after=2"
    with pytest.raises(UsageError):
        AsyncClient("http://pool.example.com").chatrooms.stream("room_1")
