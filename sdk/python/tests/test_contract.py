"""Contract parity: routes, metadata, resources and generated artifacts."""

from __future__ import annotations

import hashlib
import inspect
import pathlib
import subprocess
import sys

import pytest

import support
from tidepool_client import AsyncClient, ROUTES, ROUTE_LIST, SCOPES, ROLES, SyncClient
from tidepool_client._generated.models import MODEL_TYPES

REPOSITORY = pathlib.Path(__file__).resolve().parents[3]


def contract_operations():
    operations = {}
    for path, item in support.CONTRACT["paths"].items():
        for method, operation in item.items():
            if method in ("get", "post", "put", "patch", "delete"):
                operations[operation["operationId"]] = (method, path, operation)
    return operations


def test_every_contract_route_is_in_the_generated_table():
    operations = contract_operations()
    assert len(operations) == 84
    assert set(ROUTES) == set(operations)
    assert len(ROUTE_LIST) == len(operations)


def test_route_metadata_matches_the_contract():
    for operation_id, (method, path, operation) in contract_operations().items():
        route = ROUTES[operation_id]
        assert route.method == method.upper()
        assert route.path == path
        assert route.path_params == tuple(support.route_path_params(operation_id))
        assert route.auth == (operation.get("x-auth") or "public")
        assert route.scope == operation.get("x-required-scope")
        assert route.role == operation.get("x-required-role")
        assert route.surface == (operation.get("x-surface") or "unknown")
        assert route.deprecated is (operation.get("deprecated") is True)
        statuses = [
            int(status)
            for status in operation.get("responses", {})
            if status.isdigit()
        ]
        assert route.rate_limited is (429 in statuses)


def test_query_table_matches_the_documented_handler_names():
    assert ROUTES["get_v1_activity"].query_params == (
        "project",
        "provider",
        "trigger",
        "status",
        "kind",
        "cursor",
        "limit",
    )
    assert ROUTES["get_v1_chatrooms_room_messages"].query_params == ("after", "limit")
    assert ROUTES["get_v1_chatrooms_room_stream"].query_params == ("token", "after")
    assert ROUTES["get_v1_sessions_session_relay_events"].query_params == (
        "after",
        "limit",
    )
    assert ROUTES["get_v1_leaderboards_game_id"].query_params == ("page", "per_page")


def test_scope_and_role_constants_match_the_contract_document():
    assert SCOPES == (
        "projects:read",
        "configuration:validate",
        "configuration:install",
        "runs:dispatch",
        "daemons:enroll",
    )
    assert ROLES == ("owner", "admin", "member", "viewer")


def test_every_response_model_exists():
    from tidepool_client._generated import models as generated

    for route in ROUTE_LIST:
        for name in route.response_models:
            assert name in MODEL_TYPES, route.operation_id
        if route.body_model:
            assert hasattr(generated, route.body_model), route.operation_id


def test_generated_modules_match_the_contract_hash():
    digest = hashlib.sha256(support.CONTRACT_PATH.read_bytes()).hexdigest()
    from tidepool_client import CONTRACT_SHA256, CONTRACT_VERSION

    assert CONTRACT_SHA256 == digest
    assert CONTRACT_VERSION == support.CONTRACT["info"]["version"]


def test_generated_artifacts_are_current():
    result = subprocess.run(
        [sys.executable, "sdk/python/scripts/generate.py", "--check"],
        cwd=REPOSITORY,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    assert "current" in result.stdout


@pytest.mark.parametrize("client_class", [SyncClient, AsyncClient])
def test_every_route_has_a_resource_method_on_both_facades(client_class):
    client = client_class("http://127.0.0.1:1")
    for route in ROUTE_LIST:
        method = support.resolve_resource(client.resources, route)
        assert callable(method), route.operation_id
        signature = inspect.signature(method)
        parameters = signature.parameters
        for name in route.path_params:
            assert name in parameters, (route.operation_id, name)
        if route.has_body:
            assert "body" in parameters, route.operation_id
        for name in route.query_params:
            assert name in parameters, (route.operation_id, name)
        expected_async = client_class is AsyncClient and route.response_kind != "stream"
        assert inspect.iscoroutinefunction(method) is expected_async, route.operation_id
