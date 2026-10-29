#!/usr/bin/env python3
"""Generate the committed Python artifacts from the API contract.

``docs/api/openapi-v1.json`` is the only input. This script renders three
committed modules deterministically, so ``--check`` can regenerate them in
memory and compare byte for byte:

* ``tidepool_client/_generated/models.py`` — one dataclass per contract schema;
* ``tidepool_client/_generated/routes.py`` — the typed route table both clients
  dispatch on;
* ``tidepool_client/_generated/resources.py`` — one resource namespace per
  operation, for the sync and the async client.

Every route in the document must have an explicit ``{namespace, name}`` mapping
below. A route without a mapping fails generation rather than shipping untyped,
which is the same rule the TypeScript and Rust generators apply.

Usage (from the repository root)::

    python sdk/python/scripts/generate.py
    python sdk/python/scripts/generate.py --check
    python sdk/python/scripts/generate.py --spec path/to/openapi-v1.json
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple

HERE = Path(__file__).resolve()
PYTHON_ROOT = HERE.parents[1]
REPOSITORY = HERE.parents[3]
DEFAULT_CONTRACT = REPOSITORY / "docs/api/openapi-v1.json"
REF_PREFIX = "#/components/schemas/"

#: Operation id to ``namespace segments..., method name``, mirroring the Rust
#: and TypeScript SDKs. A new contract route without an entry fails generation.
NAMES: Dict[str, Sequence[str]] = {
    "get_healthz": ("system", "health"),
    "get_metrics": ("system", "metrics"),
    "get_openapi_json": ("system", "openapi"),
    "get_openapi_v1_json": ("system", "openapiV1"),
    "post_v1_setup": ("setup", "create"),
    "get_v1_setup_status": ("setup", "status"),
    "post_v1_pool_login": ("pool", "login"),
    "get_v1_pool_me": ("pool", "me"),
    "get_v1_pool_members": ("pool", "members"),
    "patch_v1_pool_members_id": ("pool", "updateMember"),
    "delete_v1_pool_members_id": ("pool", "removeMember"),
    "get_v1_pool_invitations": ("pool", "invitations"),
    "post_v1_pool_invitations": ("pool", "invite"),
    "post_v1_pool_invitations_accept": ("pool", "acceptInvitation"),
    "delete_v1_pool_invitations_id": ("pool", "revokeInvitation"),
    "post_v1_pool_password": ("pool", "changePassword"),
    "post_v1_pool_logout": ("pool", "logout"),
    "get_v1_api_keys": ("apiKeys", "list"),
    "post_v1_api_keys": ("apiKeys", "create"),
    "get_v1_api_keys_whoami": ("apiKeys", "whoami"),
    "delete_v1_api_keys_id": ("apiKeys", "remove"),
    "get_v1_hosts": ("hosts", "list"),
    "post_v1_hosts": ("hosts", "create"),
    "post_v1_hosts_enroll": ("hosts", "enroll"),
    "post_v1_hosts_enrollment_tokens": ("hosts", "mintEnrollmentToken"),
    "get_v1_hosts_id": ("hosts", "get"),
    "patch_v1_hosts_id": ("hosts", "rename"),
    "delete_v1_hosts_id": ("hosts", "remove"),
    "get_v1_hosts_id_capabilities": ("hosts", "capabilities"),
    "post_v1_hosts_id_capabilities": ("hosts", "reportCapabilities"),
    "post_v1_hosts_id_execution": ("hosts", "startExecution"),
    "delete_v1_hosts_id_execution": ("hosts", "stopExecution"),
    "post_v1_hosts_id_executions": ("hosts", "createExecution"),
    "post_v1_hosts_id_executions_execution_id_interrupt": ("hosts", "interruptExecution"),
    "get_v1_connections": ("connections", "list"),
    "post_v1_connections": ("connections", "create"),
    "get_v1_connections_id": ("connections", "get"),
    "patch_v1_connections_id": ("connections", "update"),
    "delete_v1_connections_id": ("connections", "remove"),
    "get_v1_projects": ("projects", "list"),
    "post_v1_projects": ("projects", "create"),
    "get_v1_projects_id": ("projects", "get"),
    "patch_v1_projects_id": ("projects", "rename"),
    "delete_v1_projects_id": ("projects", "remove"),
    "get_v1_projects_id_configuration": ("projects", "configuration"),
    "post_v1_projects_id_configuration_install": (
        "projects",
        "installConfiguration",
    ),
    "post_v1_projects_id_configuration_validate": (
        "projects",
        "validateConfiguration",
    ),
    "get_v1_projects_id_revisions": ("projects", "revisions"),
    "get_v1_projects_id_revisions_revision_id": ("projects", "revision"),
    "post_v1_triggers_validate": ("triggers", "validate"),
    "post_v1_triggers_install": ("triggers", "install"),
    "post_v1_runs": ("runs", "dispatch"),
    "get_v1_activity": ("activity", "list"),
    "get_v1_activity_runs_run": ("activity", "run"),
    "get_v1_chatrooms": ("chatrooms", "list"),
    "get_v1_chatrooms_room_members": ("chatrooms", "members"),
    "get_v1_chatrooms_room_messages": ("chatrooms", "messages"),
    "post_v1_chatrooms_room_messages": ("chatrooms", "send"),
    "get_v1_chatrooms_room_stream": ("chatrooms", "stream"),
    "get_v1_sessions_session_relay": ("relay", "get"),
    "put_v1_sessions_session_relay": ("relay", "enable"),
    "delete_v1_sessions_session_relay": ("relay", "stop"),
    "get_v1_sessions_session_relay_events": ("relay", "events"),
    "post_v1_sessions_session_relay_events": ("relay", "publish"),
    "post_v1_webhooks_github_slug": ("webhooks", "github"),
    "post_v1_webhooks_slack_slug": ("webhooks", "slack"),
    "post_v1_auth_login": ("game", "auth", "login"),
    "post_v1_auth_register": ("game", "auth", "register"),
    "post_v1_events": ("game", "events", "create"),
    "post_v1_feedback": ("game", "feedback", "create"),
    "get_v1_games_game_id_settings": ("game", "games", "settings"),
    "put_v1_games_game_id_settings": ("game", "games", "updateSettings"),
    "get_v1_leaderboards_game_id": ("game", "leaderboards", "get"),
    "post_v1_leaderboards_game_id_entries": (
        "game",
        "leaderboards",
        "submitEntry",
    ),
    "put_v1_leaderboards_game_id_entries_player_id": (
        "game",
        "leaderboards",
        "putEntry",
    ),
    "post_v1_players_subaccount": ("game", "players", "createSubaccount"),
    "get_v1_players_id": ("game", "players", "get"),
    "put_v1_players_id": ("game", "players", "update"),
    "delete_v1_players_id": ("game", "players", "remove"),
    "get_v1_players_id_subaccounts": ("game", "players", "subaccounts"),
    "post_v1_saves": ("game", "saves", "create"),
    "get_v1_saves_player_id": ("game", "saves", "list"),
    "get_v1_saves_player_id_save_id": ("game", "saves", "get"),
    "delete_v1_saves_player_id_save_id": ("game", "saves", "remove"),
    "post_v1_socket_tickets": ("game", "socketTickets", "create"),
}

#: Query parameters read by handlers but absent from the document
#: (``docs/API_CONTRACT.md`` §5.1). Mirrors the other SDKs' tables.
QUERY: Dict[str, Dict[str, str]] = {
    "get_v1_activity": {
        "project": "string",
        "provider": "string",
        "trigger": "string",
        "status": "string",
        "kind": "string",
        "cursor": "string",
        "limit": "number",
    },
    "get_v1_chatrooms_room_messages": {"after": "number", "limit": "number"},
    "get_v1_chatrooms_room_stream": {"token": "string", "after": "number"},
    "get_v1_sessions_session_relay_events": {"after": "number", "limit": "number"},
    "get_v1_leaderboards_game_id": {"page": "number", "per_page": "number"},
}

#: The one operation that upgrades to a WebSocket instead of returning JSON.
STREAM_OPS = {"get_v1_chatrooms_room_stream"}

#: Operations whose success body is plain text rather than JSON.
TEXT_OPS = {"get_metrics"}

METHODS = ("get", "post", "put", "patch", "delete")

HEADER = (
    '"""Generated from docs/api/openapi-v1.json. Do not edit by hand.\n\n'
    "Regenerate: python sdk/python/scripts/generate.py\n"
    '"""\n\n'
    "from __future__ import annotations\n\n"
)

def snake(name: str) -> str:
    """Snake-case a camelCase, kebab-case or space-separated identifier."""
    value = re.sub(r"[-.\s]+", "_", name)
    value = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", value)
    value = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", value)
    return value.lower()


def pascal(name: str) -> str:
    """Pascal-case an identifier."""
    return "".join(part[:1].upper() + part[1:] for part in snake(name).split("_") if part)


def field_name(name: str) -> str:
    """A Python attribute name for a JSON property name."""
    value = snake(name)
    return "type_" if value == "type" else value


def nullable(schema: Optional[Dict[str, Any]]) -> bool:
    """True when a schema allows ``null``."""
    if not schema:
        return False
    return isinstance(schema.get("type"), list) and "null" in schema["type"]


def base_type(schema: Optional[Dict[str, Any]]) -> str:
    """Python annotation for a schema, ignoring nullability."""
    if not schema or not schema:
        return "Any"
    if "$ref" in schema:
        ref = schema["$ref"]
        if not ref.startswith(REF_PREFIX):
            raise ValueError(f"unsupported $ref: {ref}")
        return ref[len(REF_PREFIX) :]
    types = schema.get("type")
    candidates = types if isinstance(types, list) else [types]
    only = next((item for item in candidates if item != "null"), None)
    if only == "string":
        return "str"
    if only == "integer":
        return "int"
    if only == "number":
        return "float"
    if only == "boolean":
        return "bool"
    if only == "array":
        return f"List[{annotation(schema.get('items') or {}, required=True)}]"
    if only == "object":
        additional = schema.get("additionalProperties")
        if isinstance(additional, dict):
            return f"Dict[str, {annotation(additional, required=True)}]"
        return "Dict[str, Any]"
    return "Any"


def annotation(schema: Optional[Dict[str, Any]], *, required: bool) -> str:
    """Full annotation for a field, optional when absent or nullable."""
    base = base_type(schema)
    if base == "Any":
        return base
    if nullable(schema) or not required:
        if base.startswith("Optional["):
            return base
        return f"Optional[{base}]"
    return base


def docstring(description: Optional[str], indent: str) -> List[str]:
    """Render a description as a Python docstring body."""
    if not description:
        return []
    text = " ".join(str(description).replace('"""', "'''").split())
    if not text:
        return []
    return [f'{indent}"""{text}"""']


def request_body_schema(operation: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    """The JSON request schema, or ``None`` when the route takes no body."""
    content = (operation.get("requestBody") or {}).get("content") or {}
    document = content.get("application/json")
    if document:
        return document.get("schema") or {}
    if operation.get("requestBody"):
        return {}
    return None


def response_info(
    operation_id: str, operation: Dict[str, Any]
) -> Tuple[str, Tuple[str, ...], Tuple[int, ...]]:
    """Classify a route's success body: kind, model names, success statuses."""
    success: List[int] = []
    models: List[str] = []
    has_untyped = False
    has_empty = False
    for status, response in (operation.get("responses") or {}).items():
        try:
            code = int(status)
        except (TypeError, ValueError):
            continue
        if not 200 <= code < 300:
            continue
        success.append(code)
        content = (response.get("content") or {})
        document = content.get("application/json") or content.get("*/*")
        if not document:
            has_empty = True
            continue
        schema = document.get("schema") or {}
        if "$ref" in schema:
            models.append(schema["$ref"][len(REF_PREFIX) :])
        else:
            has_untyped = True
    success.sort()
    if operation_id in STREAM_OPS:
        return "stream", (), tuple(success)
    if operation_id in TEXT_OPS:
        return "text", (), tuple(success)
    if models and not has_untyped and not has_empty and len(set(models)) == 1:
        return "model", (models[0],), tuple(success)
    if models and not has_untyped and not has_empty:
        return "union", tuple(dict.fromkeys(models)), tuple(success)
    if not models and not has_untyped:
        return "empty", (), tuple(success)
    return "json", (), tuple(success)


def derive_routes(spec: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Build the route table, failing on any unmapped operation."""
    routes: List[Dict[str, Any]] = []
    for path, operations in spec["paths"].items():
        for method, operation in operations.items():
            if method not in METHODS:
                continue
            operation_id = operation.get("operationId")
            if not operation_id:
                raise ValueError(f"contract route {method.upper()} {path} has no operationId")
            naming = NAMES.get(operation_id)
            if not naming:
                raise ValueError(
                    f"contract route {method.upper()} {path} ({operation_id}) has no SDK "
                    "mapping; add it to NAMES in sdk/python/scripts/generate.py"
                )
            path_params = tuple(
                parameter["name"]
                for parameter in operation.get("parameters") or []
                if parameter.get("in") == "path"
            )
            errors = sorted(
                int(status)
                for status in (operation.get("responses") or {})
                if status.isdigit()
                and 400 <= int(status) < 500
                and int(status) != 429
            )
            kind, models, success = response_info(operation_id, operation)
            body = request_body_schema(operation)
            routes.append(
                {
                    "operation_id": operation_id,
                    "method": method.upper(),
                    "path": path,
                    "namespace": tuple(snake(part) for part in naming[:-1]),
                    "name": snake(naming[-1]),
                    "path_params": path_params,
                    "query": tuple((QUERY.get(operation_id) or {}).keys()),
                    "query_kinds": dict(QUERY.get(operation_id) or {}),
                    "has_body": body is not None
                    or method.upper() in ("POST", "PUT", "PATCH"),
                    "body_model": body.get("$ref", "")[len(REF_PREFIX) :]
                    if body and "$ref" in body
                    else None,
                    "response_kind": kind,
                    "response_models": models,
                    "success": success,
                    "errors": tuple(errors),
                    "rate_limited": "429" in (operation.get("responses") or {}),
                    "auth": operation.get("x-auth") or "public",
                    "scope": operation.get("x-required-scope"),
                    "role": operation.get("x-required-role"),
                    "surface": operation.get("x-surface") or "unknown",
                    "deprecated": operation.get("deprecated") is True,
                    "summary": operation.get("summary") or "",
                }
            )
    routes.sort(key=lambda route: route["operation_id"])
    return routes


def render_routes(spec: Dict[str, Any], routes: List[Dict[str, Any]], digest: str) -> str:
    """Render ``tidepool_client/_generated/routes.py``."""
    out = [HEADER]
    out.append("from dataclasses import dataclass\n")
    out.append("from typing import Dict, Optional, Tuple\n\n")
    out.append(f"CONTRACT_SHA256 = {digest!r}\n\n")
    out.append(f"CONTRACT_VERSION = {spec['info']['version']!r}\n\n")
    out.append(
        "SCOPES = (\n"
        '    "projects:read",\n'
        '    "configuration:validate",\n'
        '    "configuration:install",\n'
        '    "runs:dispatch",\n'
        '    "daemons:enroll",\n'
        ")\n\n"
    )
    out.append('ROLES = ("owner", "admin", "member", "viewer")\n\n\n')
    out.append("@dataclass(frozen=True)\n")
    out.append("class Route:\n")
    out.append('    """One contract operation and the access model it declares."""\n\n')
    for name, description in (
        ("operation_id", "The contract's operation id."),
        ("method", "HTTP method, upper case."),
        ("path", "Path template, with `{param}` placeholders."),
        ("namespace", "Generated resource namespace path (snake_case)."),
        ("name", "Generated resource method name (snake_case)."),
        ("path_params", "Path parameter names, in contract order."),
        ("query_params", "Query parameter names the SDK sends."),
        ("has_body", "True when the route accepts a JSON request body."),
        ("body_model", "Generated model name for the request body, when typed."),
        ("response_kind", "How the success body is decoded."),
        ("response_models", "Generated model names for the success body."),
        ("success", "Success statuses declared by the contract."),
        ("errors", "Non-429 4xx statuses declared by the contract."),
        ("rate_limited", "True when the contract declares a 429 response."),
        ("auth", "Contract authentication model (`x-auth`)."),
        ("scope", "API-key scope required (`x-required-scope`)."),
        ("role", "Member role required (`x-required-role`)."),
        ("surface", "Contract surface: `hub`, `game` or `system`."),
        ("deprecated", "True when the contract marks the route deprecated."),
        ("summary", "Human-readable summary."),
    ):
        out.append(f"    # {description}\n")
        out.append(f"    {name}: {_route_annotation(name)}\n")
    out.append("ROUTES: Dict[str, Route] = {\n")
    for route in routes:
        out.append(f"    {route['operation_id']!r}: Route(\n")
        out.append(f"        operation_id={route['operation_id']!r},\n")
        out.append(f"        method={route['method']!r},\n")
        out.append(f"        path={route['path']!r},\n")
        out.append(f"        namespace={route['namespace']!r},\n")
        out.append(f"        name={route['name']!r},\n")
        out.append(f"        path_params={route['path_params']!r},\n")
        out.append(f"        query_params={route['query']!r},\n")
        out.append(f"        has_body={route['has_body']!r},\n")
        out.append(f"        body_model={route['body_model']!r},\n")
        out.append(f"        response_kind={route['response_kind']!r},\n")
        out.append(f"        response_models={route['response_models']!r},\n")
        out.append(f"        success={route['success']!r},\n")
        out.append(f"        errors={route['errors']!r},\n")
        out.append(f"        rate_limited={route['rate_limited']!r},\n")
        out.append(f"        auth={route['auth']!r},\n")
        out.append(f"        scope={route['scope']!r},\n")
        out.append(f"        role={route['role']!r},\n")
        out.append(f"        surface={route['surface']!r},\n")
        out.append(f"        deprecated={route['deprecated']!r},\n")
        out.append(f"        summary={route['summary']!r},\n")
        out.append("    ),\n")
    out.append("}\n\n")
    out.append('ROUTE_LIST: Tuple[Route, ...] = tuple(ROUTES.values())\n\n\n')
    out.append("def route(operation_id: str) -> Optional[Route]:\n")
    out.append('    """Route metadata by operation id, or ``None`` when unknown."""\n')
    out.append("    return ROUTES.get(operation_id)\n")
    return "".join(out)


def _route_annotation(name: str) -> str:
    """The annotation for one generated ``Route`` field."""
    return {
        "operation_id": "str",
        "method": "str",
        "path": "str",
        "namespace": "Tuple[str, ...]",
        "name": "str",
        "path_params": "Tuple[str, ...]",
        "query_params": "Tuple[str, ...]",
        "has_body": "bool",
        "body_model": "Optional[str]",
        "response_kind": "str",
        "response_models": "Tuple[str, ...]",
        "success": "Tuple[int, ...]",
        "errors": "Tuple[int, ...]",
        "rate_limited": "bool",
        "auth": "str",
        "scope": "Optional[str]",
        "role": "Optional[str]",
        "surface": "str",
        "deprecated": "bool",
        "summary": "str",
    }[name]


def render_models(spec: Dict[str, Any]) -> str:
    """Render ``tidepool_client/_generated/models.py``."""
    schemas: Dict[str, Any] = spec["components"]["schemas"]
    out = [HEADER]
    out.append("from dataclasses import dataclass\n")
    out.append("from typing import Any, Dict, List, Optional\n\n")
    out.append("from ..model import Model, ModelList\n\n")
    aliases: List[str] = []
    classes: List[str] = []
    model_names: List[str] = []
    for name in sorted(schemas):
        schema = schemas[name]
        if name == "RawJson":
            aliases.append(
                'RawJson = Any\n"""Any JSON value; the contract does not constrain it."""\n'
            )
            continue
        if not schema.get("properties"):
            if schema.get("type") == "array":
                item = annotation(schema.get("items") or {}, required=True)
                description = schema.get("description") or f"A list of {item}."
                classes.append(
                    f"\n\nclass {name}(ModelList):\n"
                    f'    """{description}"""\n\n'
                    f"    item_type = {item}\n"
                )
                model_names.append(name)
                continue
            base = base_type(schema)
            if base == name:
                base = "Any"
            description = schema.get("description") or f"Alias for {base}."
            aliases.append(f"{name} = {base}\n# {description}\n")
            continue
        model_names.append(name)
        required = set(schema.get("required") or [])
        lines: List[str] = []
        lines.append("\n\n@dataclass\n")
        lines.append(f"class {name}(Model):\n")
        description = schema.get("description")
        lines.extend(docstring(description, "    ") or ['    """Contract model."""'])
        lines.append("\n\n")
        json_names: Dict[str, str] = {}
        for json_name, prop in schema.get("properties", {}).items():
            attr = "type_" if name == "Problem" and json_name == "type" else field_name(json_name)
            # `Problem` is parsed from whatever a failing server sent, so only
            # `code` is treated as required; the rest degrade to None.
            is_required = json_name == "code" if name == "Problem" else json_name in required
            notes = prop.get("description")
            if prop.get("enum"):
                values = ", ".join(repr(value) for value in prop["enum"])
                notes = f"{notes or ''} Known values: {values}. Unknown values are tolerated."
            if notes:
                sanitized = " ".join(str(notes).split()).replace('"""', "'''")
                lines.append(f"    # {sanitized}\n")
            lines.append(f"    {attr}: {annotation(prop, required=is_required)} = None\n")
            if attr != json_name:
                json_names[attr] = json_name
        if json_names:
            lines.append(f"\n    __json_names__ = {json_names!r}\n")
        classes.append("".join(lines))
    out.append("".join(classes))
    out.append("\n\n")
    out.append("".join(aliases))
    out.append("\n\nMODEL_TYPES: Dict[str, Any] = {\n")
    for name in model_names:
        out.append(f"    {name!r}: {name},\n")
    out.append("}\n")
    out.append("\n\n__all__ = [\n")
    for name in sorted(schemas):
        out.append(f"    {name!r},\n")
    out.append("    'MODEL_TYPES',\n")
    out.append("]\n")
    return "".join(out)


def _return_annotation(route: Dict[str, Any]) -> str:
    kind = route["response_kind"]
    if kind == "empty":
        return "None"
    if kind == "text":
        return "str"
    if kind == "json":
        return "Dict[str, Any]"
    if kind == "stream":
        return "ChatStream"
    models = route["response_models"]
    if len(models) == 1:
        return models[0]
    return "Union[" + ", ".join(models) + "]"


def _method_parameters(route: Dict[str, Any]) -> List[str]:
    """Render the parameter list of one generated resource method."""
    params = [f"{parameter}: str" for parameter in route["path_params"]]
    if route["has_body"]:
        body_type = "Any"
        if route["body_model"]:
            body_type = f'Union["{route["body_model"]}", Mapping[str, Any]]'
        params.append(f"body: {body_type} = None")
    for name in route["query"]:
        kind = route["query_kinds"].get(name, "string")
        if kind == "number":
            params.append(f"{name}: Optional[int] = None")
        else:
            params.append(f"{name}: Optional[str] = None")
    if route["response_kind"] != "stream":
        params.append('options: "CallOptions" = None')
    return params


def _method_body(route: Dict[str, Any], async_mode: bool) -> List[str]:
    """Render the body of one generated resource method, eight-space indented."""
    lines: List[str] = []
    if route["response_kind"] == "stream":
        path_parts = ", ".join(f"{name!r}: {name}" for name in route["path_params"])
        query_parts = ", ".join(f"{name!r}: {name}" for name in route["query"])
        lines.append("        return self._client.stream(\n")
        lines.append(f"            {route['operation_id']!r},\n")
        lines.append(f"            path_params={{{path_parts}}},\n")
        lines.append(f"            query={{{query_parts}}},\n")
        lines.append("        )\n")
        return lines
    path_parts = ", ".join(f"{name!r}: {name}" for name in route["path_params"])
    query_parts = ", ".join(f"{name!r}: {name}" for name in route["query"])
    lines.append("        return await self._client.call(\n" if async_mode else "        return self._client.call(\n")
    lines.append(f"            {route['operation_id']!r},\n")
    if route["path_params"]:
        lines.append(f"            path_params={{{path_parts}}},\n")
    if route["query"]:
        lines.append(f"            query={{{query_parts}}},\n")
    if route["has_body"]:
        lines.append("            body=body,\n")
    lines.append("            options=options,\n")
    lines.append("        )\n")
    return lines


def _render_namespace_tree(
    routes: List[Dict[str, Any]], async_mode: bool, prefix: str, client: str
) -> List[str]:
    """Render resource namespace classes for one facade."""
    tree: Dict[str, Any] = {"methods": [], "children": {}}
    for route in routes:
        node = tree
        for segment in route["namespace"]:
            node = node["children"].setdefault(segment, {"methods": [], "children": {}})
        node["methods"].append(route)
    out: List[str] = []

    def class_name(path: Sequence[str]) -> str:
        if not path:
            return f"{prefix}Resources"
        stem = "".join(pascal(part) for part in path)
        return f"{prefix}{stem}"

    def render(node: Dict[str, Any], path: Sequence[str]) -> None:
        out.append(f"class {class_name(path)}:\n")
        out.append(f'    """Contract operations for the {".".join(path) or "root"} namespace."""\n\n')
        out.append(f'    def __init__(self, client: "{client}") -> None:\n')
        out.append("        self._client = client\n")
        for segment in sorted(node["children"]):
            out.append(f"        self.{segment} = {class_name(path + (segment,))}(client)\n")
        out.append("\n")
        for route in sorted(node["methods"], key=lambda item: item["name"]):
            parameters = ", ".join(["self", *_method_parameters(route)])
            keyword = (
                "async def" if async_mode and route["response_kind"] != "stream" else "def"
            )
            out.append(
                f"    {keyword} {route['name']}({parameters}) -> {_return_annotation(route)}:\n"
            )
            summary = (route["summary"] or f"Call `{route['operation_id']}`.").replace('"""', "'''")
            out.append(f'        """{summary}"""\n')
            out.extend(_method_body(route, async_mode))
            out.append("\n")
        for segment in sorted(node["children"]):
            render(node["children"][segment], path + (segment,))

    render(tree, ())
    return out


def render_resources(routes: List[Dict[str, Any]]) -> str:
    """Render ``tidepool_client/_generated/resources.py``."""
    out = [HEADER]
    out.append("from typing import TYPE_CHECKING, Any, Dict, Mapping, Optional, Union\n\n")
    out.append("if TYPE_CHECKING:\n")
    out.append("    from ..client import AsyncClient, CallOptions, SyncClient\n")
    out.append("    from ..stream import ChatStream\n")
    model_names = sorted(
        {
            model
            for route in routes
            for model in route["response_models"]
        }
        | {
            route["body_model"]
            for route in routes
            if route["body_model"]
        }
    )
    for name in model_names:
        out.append(f"    from .models import {name}\n")
    out.append("\n\n")
    out.extend(_render_namespace_tree(routes, False, "Sync", "SyncClient"))
    out.append("\n")
    out.extend(_render_namespace_tree(routes, True, "Async", "AsyncClient"))
    return "".join(out)


def render(spec: Dict[str, Any], digest: str) -> Dict[str, str]:
    """Render every artifact, keyed by path relative to ``sdk/python``."""
    routes = derive_routes(spec)
    return {
        "tidepool_client/_generated/models.py": render_models(spec),
        "tidepool_client/_generated/routes.py": render_routes(spec, routes, digest),
        "tidepool_client/_generated/resources.py": render_resources(routes),
    }


def main(argv: Sequence[str]) -> int:
    """Write every artifact, or check them with ``--check``."""
    check = "--check" in argv
    spec_path = DEFAULT_CONTRACT
    if "--spec" in argv:
        spec_path = Path(argv[argv.index("--spec") + 1]).resolve()
    unknown = [
        argument
        for index, argument in enumerate(argv)
        if argument not in ("--check", "--spec")
        and (index == 0 or argv[index - 1] != "--spec")
    ]
    if unknown:
        sys.stderr.write(f"unknown argument: {unknown[0]}\n")
        return 2
    raw = spec_path.read_bytes()
    spec = json.loads(raw.decode("utf-8"))
    digest = hashlib.sha256(raw).hexdigest()
    artifacts = render(spec, digest)
    stale: List[str] = []
    for relative, content in artifacts.items():
        target = PYTHON_ROOT / relative
        if check:
            try:
                current = target.read_text(encoding="utf-8")
            except OSError:
                current = None
            if current != content:
                stale.append(relative)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        sys.stdout.write(f"wrote {relative}\n")
    if check:
        if stale:
            sys.stderr.write(
                "generated artifacts are stale:\n"
                + "".join(f"  - {path}\n" for path in stale)
                + "run `python sdk/python/scripts/generate.py` from the repository root\n"
            )
            return 1
        sys.stdout.write(f"sdk/python generated artifacts are current ({len(derive_routes(spec))} routes)\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
