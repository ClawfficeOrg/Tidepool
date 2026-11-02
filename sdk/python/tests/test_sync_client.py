"""The synchronous client: route coverage, retries, refresh and errors."""

from __future__ import annotations

import pytest

import support
from tidepool_client import (
    CallOptions,
    ClientConfig,
    ConfigError,
    DecodeError,
    MissingBodyError,
    ProblemError,
    RetryAfterTooLongError,
    RetryPolicy,
    SyncClient,
    UnknownOperationError,
    UsageError,
)
from tidepool_client._generated.models import MODEL_TYPES, CreateHostRequest


def test_every_contract_route_is_reachable_from_the_sync_client():
    from tidepool_client import ROUTE_LIST

    table = support.coverage_table(ROUTE_LIST)
    with support.RecordedServer(support.coverage_responder(table)) as server:
        client = SyncClient(server.base_url, token="operator-token")
        for route in ROUTE_LIST:
            arguments = support.route_arguments(route)
            method = support.resolve_resource(client.resources, route)
            result = method(**arguments)
            support.assert_route_result(route, result)
        assert len(server.requests) == len(table)
        for request in server.requests:
            assert (request.method, request.path) in table


def test_credentials_and_headers_are_sent():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    with support.RecordedServer(lambda request: support.json_response(200, body)) as server:
        client = SyncClient(
            server.base_url,
            token="operator-token",
            api_key="api-key",
            host_credential="host-cred",
            headers=(("x-extra", "client"),),
            user_agent="test-agent/1",
        )
        client.hosts.list(
            options=CallOptions().with_request_id("req_9").with_header("x-call", "one")
        )
        headers = server.requests[0].headers
        assert headers["authorization"] == "Bearer operator-token"
        assert headers["x-api-key"] == "api-key"
        assert headers["x-host-credential"] == "host-cred"
        assert headers["x-extra"] == "client"
        assert headers["x-call"] == "one"
        assert headers["x-request-id"] == "req_9"
        assert headers["user-agent"] == "test-agent/1"
        assert headers["accept"] == "application/json"


def test_body_is_encoded_as_json():
    host = support.sample_schema(support.SCHEMAS["HostView"])
    with support.RecordedServer(lambda request: support.json_response(201, host)) as server:
        client = SyncClient(server.base_url, token="t")
        client.hosts.create(body=CreateHostRequest(hostname="build-box.local"))
        request = server.requests[0]
        assert request.headers["content-type"] == "application/json"
        assert request.json == {"hostname": "build-box.local"}


def test_query_parameters_omit_none_and_encode():
    page = support.sample_schema(support.SCHEMAS["ActivityPage"])
    with support.RecordedServer(lambda request: support.json_response(200, page)) as server:
        client = SyncClient(server.base_url)
        client.activity.list(project="proj 1", limit=5)
        assert server.requests[0].query == {"project": "proj 1", "limit": "5"}


def test_path_parameters_are_percent_encoded():
    host = support.sample_schema(support.SCHEMAS["HostView"])
    with support.RecordedServer(lambda request: support.json_response(200, host)) as server:
        client = SyncClient(server.base_url)
        client.hosts.get("host/one two")
        assert server.requests[0].path.startswith("/v1/hosts/host%2Fone%20two")


def test_problem_details_are_typed_and_unknown_codes_survive():
    def respond(request):
        return support.problem_response(
            409, "a_future_conflict", "already there", headers={"x-request-id": "req_7"}
        )

    with support.RecordedServer(respond) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(ProblemError) as caught:
            client.hosts.create(body={"hostname": "b"})
    problem = caught.value
    assert problem.status == 409
    assert problem.code == "a_future_conflict"
    assert problem.detail == "already there"
    assert problem.request_id == "req_7"
    assert problem.attempts == 1
    assert problem.operation_id == "post_v1_hosts"
    assert "a_future_conflict" in str(problem)


def test_legacy_error_shape_and_plain_text_are_normalised():
    def respond(request):
        if request.path.endswith("/hosts"):
            return support.json_response(400, {"error": "legacy", "message": "old shape"})
        return 500, {"Content-Type": "text/plain"}, "boom"

    with support.RecordedServer(respond) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(ProblemError) as caught:
            client.hosts.list()
        assert caught.value.code == "legacy"
        assert caught.value.detail == "old shape"
    with support.RecordedServer(respond) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(ProblemError) as caught:
            client.hosts.get("h1")
        assert caught.value.code == "http_500"
        assert caught.value.detail == "boom"


def test_rate_limit_is_retried_then_succeeds():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    calls = {"count": 0}

    def respond(request):
        calls["count"] += 1
        if calls["count"] == 1:
            return support.problem_response(429, "rate_limited", headers={"Retry-After": "0.01"})
        return support.json_response(200, body)

    with support.RecordedServer(respond) as server:
        client = SyncClient(
            server.base_url, retry=RetryPolicy(max_retries=2, base_delay_ms=1)
        )
        page = client.hosts.list()
    assert len(page.hosts) == 1
    assert calls["count"] == 2


def test_rate_limit_exhaustion_surfaces_with_attempts():
    with support.RecordedServer(lambda request: support.problem_response(429, "rate_limited")) as server:
        client = SyncClient(
            server.base_url, retry=RetryPolicy(max_retries=2, base_delay_ms=1)
        )
        with pytest.raises(ProblemError) as caught:
            client.hosts.list()
        assert caught.value.attempts == 3
        assert caught.value.rate_limited
        assert len(server.requests) == 3


def test_excessive_retry_after_is_surfaced_without_sleeping():
    with support.RecordedServer(
        lambda request: support.problem_response(429, "rate_limited", headers={"Retry-After": "120"})
    ) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(RetryAfterTooLongError) as caught:
            client.hosts.list()
        assert caught.value.requested_ms == 120_000
        assert caught.value.limit_ms == 60_000
        assert len(server.requests) == 1


def test_server_errors_are_not_retried():
    with support.RecordedServer(lambda request: support.problem_response(500, "internal_error")) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(ProblemError):
            client.hosts.list()
        assert len(server.requests) == 1


def test_per_call_options_can_disable_retries():
    with support.RecordedServer(lambda request: support.problem_response(429, "rate_limited")) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(ProblemError) as caught:
            client.hosts.list(options=CallOptions().without_retry())
        assert caught.value.attempts == 1
        assert len(server.requests) == 1


def test_a_401_refreshes_once_and_replays_with_the_new_token():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    calls = {"refresh": 0}

    def respond(request):
        if request.headers.get("authorization") == "Bearer fresh":
            return support.json_response(200, body)
        return support.problem_response(401, "unauthorized")

    def refresh():
        calls["refresh"] += 1
        return "fresh"

    with support.RecordedServer(respond) as server:
        client = SyncClient(server.base_url, token="stale", refresh=refresh)
        page = client.hosts.list()
        assert calls["refresh"] == 1
        assert [request.headers["authorization"] for request in server.requests] == [
            "Bearer stale",
            "Bearer fresh",
        ]
        assert client.token == "fresh"
    assert page.hosts


def test_a_401_without_a_refresh_token_surfaces():
    calls = {"refresh": 0}

    def refresh():
        calls["refresh"] += 1
        return None

    with support.RecordedServer(lambda request: support.problem_response(401, "unauthorized")) as server:
        client = SyncClient(server.base_url, token="stale", refresh=refresh)
        with pytest.raises(ProblemError) as caught:
            client.hosts.list()
        assert caught.value.status == 401
        assert calls["refresh"] == 1
        assert len(server.requests) == 1


def test_expired_token_refreshes_proactively():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    seen = {}

    def respond(request):
        seen["authorization"] = request.headers.get("authorization")
        return support.json_response(200, body)

    with support.RecordedServer(respond) as server:
        client = SyncClient(
            server.base_url,
            token="stale",
            token_expiry=0,
            refresh=lambda: {"token": "fresh", "expires_in": 60},
        )
        client.hosts.list()
    assert seen["authorization"] == "Bearer fresh"


def test_malformed_json_and_missing_bodies_are_decode_errors():
    with support.RecordedServer(
        lambda request: (200, {"Content-Type": "application/json"}, b"{not json")
    ) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(DecodeError):
            client.hosts.list()
    with support.RecordedServer(
        lambda request: (200, {"Content-Type": "application/json"}, b"")
    ) as server:
        client = SyncClient(server.base_url)
        with pytest.raises(MissingBodyError):
            client.hosts.list()


def test_empty_success_is_none():
    with support.RecordedServer(lambda request: (204, {}, b"")) as server:
        client = SyncClient(server.base_url)
        assert client.hosts.remove("host_1") is None


def test_metrics_return_text():
    with support.RecordedServer(lambda request: (200, {"Content-Type": "text/plain"}, "metrics")) as server:
        client = SyncClient(server.base_url)
        assert client.system.metrics() == "metrics"


def test_untyped_game_routes_return_dicts():
    with support.RecordedServer(lambda request: support.json_response(200, {"ok": True})) as server:
        client = SyncClient(server.base_url)
        assert client.game.games.settings("game_1") == {"ok": True}


def test_union_responses_decode_by_shape():
    challenge = support.sample_schema(support.SCHEMAS["ChallengeResponse"])
    webhook = support.sample_schema(support.SCHEMAS["WebhookResponse"])
    calls = {"count": 0}

    def respond(request):
        calls["count"] += 1
        if calls["count"] == 1:
            return support.json_response(200, challenge)
        return support.json_response(202, webhook)

    with support.RecordedServer(respond) as server:
        client = SyncClient(server.base_url)
        first = client.webhooks.slack("hooks", body={})
        second = client.webhooks.slack("hooks", body={})
    assert isinstance(first, MODEL_TYPES["ChallengeResponse"])
    assert isinstance(second, MODEL_TYPES["WebhookResponse"])


def test_headers_may_be_passed_as_a_mapping():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    with support.RecordedServer(lambda request: support.json_response(200, body)) as server:
        client = SyncClient(server.base_url, headers={"x-from": "mapping"})
        client.hosts.list()
    assert server.requests[0].headers["x-from"] == "mapping"



def test_request_json_is_the_generic_escape_hatch():
    body = support.sample_schema(support.SCHEMAS["HostsResponse"])
    with support.RecordedServer(lambda request: support.json_response(200, body)) as server:
        client = SyncClient(server.base_url)
        raw = client.request_json("get_v1_hosts")
        assert raw == body
        assert server.requests[0].path == "/v1/hosts"


def test_login_adopts_the_token_and_lifetime():
    login = support.sample_schema(support.SCHEMAS["PoolLoginResponse"])
    login["token"] = "signed-jwt"
    login["expires_in"] = 3600
    with support.RecordedServer(lambda request: support.json_response(200, login)) as server:
        client = SyncClient(server.base_url)
        response = client.login("owner@example.com", "password")
        assert response.token == "signed-jwt"
        assert client.token == "signed-jwt"
        assert server.requests[0].json == {
            "email": "owner@example.com",
            "password": "password",
        }


def test_stream_builds_a_typed_websocket_url():
    client = SyncClient("https://pool.example.com/", token="operator")
    stream = client.chatrooms.stream("room_1", after=5)
    assert stream.url == "wss://pool.example.com/v1/chatrooms/room_1/stream?token=operator&after=5"
    assert stream.token == "operator"
    assert stream.after == 5
    with pytest.raises(UsageError):
        SyncClient("http://pool.example.com").chatrooms.stream("room_1")


def test_unknown_operations_and_missing_path_parameters_are_usage_errors():
    client = SyncClient("http://pool.example.com", token="t")
    with pytest.raises(UnknownOperationError):
        client.call("no_such_operation")
    with pytest.raises(UsageError):
        client.call("get_v1_hosts_id")


def test_configuration_is_validated():
    assert SyncClient("http://pool.example.com/").config.base_url == "http://pool.example.com"
    with pytest.raises(ConfigError):
        SyncClient("ftp://pool.example.com")
    with pytest.raises(ConfigError):
        ClientConfig("http://pool.example.com", timeout=0)
    with pytest.raises(ConfigError):
        ClientConfig("http://pool.example.com", headers=(("x-bad", "one\r\ntwo"),))
