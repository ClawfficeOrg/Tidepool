#!/usr/bin/env python3
"""Conformance adapter for the Python SDK (``tidepool-client``).

The adapter is deliberately thin: it turns the suite's generic commands into
calls on the SDK and normalises the answers. It holds no scenario logic, so a
new scenario never needs a change here unless it uses a command the protocol
does not have yet.

Protocol: one JSON request per line on stdin, one JSON response per line on
stdout. Requests are ``{"id", "command", "params"}``; responses are
``{"id", "ok": true, "result"}`` or ``{"id", "ok": false, "error"}``. Nothing
else may be written to stdout; diagnostics go to stderr.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional
from urllib.parse import parse_qs, urlsplit

REPOSITORY = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPOSITORY / "sdk" / "python"))

from tidepool_client import (  # noqa: E402  (path set up above)
    CONTRACT_VERSION,
    ROUTES,
    CallOptions,
    ChatFrame,
    ProblemError,
    SyncClient,
    iter_activity,
    iter_chat_messages,
    iter_relay_events,
)

MAX_PAGES = 50


class ProtocolError(Exception):
    """The request cannot be translated for this SDK."""


def build_client(base_url: str, auth: Mapping[str, Any]) -> SyncClient:
    """Build one client for the step's ambient credentials."""
    return SyncClient(
        base_url,
        token=auth.get("token"),
        api_key=auth.get("apiKey"),
        host_credential=auth.get("hostCredential"),
        timeout=60.0,
    )


def call(base_url: str, params: Mapping[str, Any]) -> Dict[str, Any]:
    """Call one operation by id and normalise success or problem details."""
    client = build_client(base_url, params.get("auth") or {})
    headers = params.get("headers") or {}
    options = CallOptions(
        request_id=params.get("requestId"),
        headers=tuple((str(name), str(value)) for name, value in headers.items()),
    )
    try:
        body = client.request_json(
            params["operation"],
            path_params=params.get("pathParams") or {},
            query=params.get("query") or {},
            body=params.get("body"),
            options=options,
        )
    except ProblemError as error:
        return {
            "outcome": "error",
            "status": error.status,
            "code": error.code,
            "title": error.title,
            "detail": error.detail,
            "type": error.problem_type,
            "request_id": error.request_id,
            "attempts": error.attempts,
            "retry_after_ms": error.retry_after_ms,
        }
    return {"outcome": "ok", "body": body}


def describe(params: Mapping[str, Any]) -> Dict[str, Any]:
    """Return the Python SDK's route metadata for one operation id."""
    operation = params["operation"]
    route = ROUTES.get(operation)
    if route is None:
        raise ProtocolError(f"operation {operation!r} is not in the contract")
    return {
        "operation_id": route.operation_id,
        "method": route.method,
        "path": route.path,
        "auth": route.auth,
        "scope": route.scope,
        "role": route.role,
        "surface": route.surface,
        "path_params": list(route.path_params),
        "query_params": list(route.query_params),
    }


def _message(message: Any) -> Dict[str, Any]:
    return {"sequence": message.sequence, "body": message.body}


def page(base_url: str, params: Mapping[str, Any]) -> Dict[str, Any]:
    """Walk one of the SDK's cursor iterators and normalise the items."""
    client = build_client(base_url, params.get("auth") or {})
    kind = params["kind"]
    identifier = params.get("id")
    after = params.get("after")
    limit = params.get("limit")
    items: List[Dict[str, Any]] = []
    if kind == "chat":
        iterator = iter_chat_messages(
            client, identifier, after=after, limit=limit, max_pages=MAX_PAGES
        )
        items = [_message(message) for message in iterator]
    elif kind == "relay":
        for event in iter_relay_events(
            client, identifier, after=after, limit=limit, max_pages=MAX_PAGES
        ):
            items.append({"sequence": event.sequence, "kind": event.kind})
    elif kind == "activity":
        for entry in iter_activity(
            client, cursor=params.get("cursor"), limit=limit, max_pages=MAX_PAGES
        ):
            items.append({"kind": entry.kind, "status": entry.status})
    else:
        raise ProtocolError(f"unknown paging kind {kind!r}")
    return {"items": items}


def stream(base_url: str, params: Mapping[str, Any]) -> Dict[str, Any]:
    """Build the SDK's live-stream handle without opening a socket."""
    client = build_client(base_url, params.get("auth") or {})
    handle = client.stream(
        params["operation"],
        path_params=params.get("pathParams") or {},
        query={"after": params.get("after")},
    )
    parts = urlsplit(handle.url)
    query = parse_qs(parts.query, keep_blank_values=True)
    token = params.get("auth", {}).get("token")
    after_values = query.get("after") or []
    return {
        "scheme": parts.scheme,
        "after": handle.after,
        "token_present": bool(query.get("token")),
        "token_matches": bool(query.get("token")) and query["token"][0] == token,
        "after_param": after_values[0] if after_values else None,
        "path": parts.path,
    }


def frames(params: Mapping[str, Any]) -> Dict[str, Any]:
    """Parse socket payloads with the SDK's typed frame parser."""
    parsed = []
    for text in params.get("texts") or []:
        frame = ChatFrame.parse(text)
        sequences = [message.sequence for message in frame.messages]
        if frame.message is not None:
            sequences.append(frame.message.sequence)
        parsed.append(
            {
                "kind": frame.kind,
                "catch_up": frame.catch_up,
                "cursor": frame.cursor,
                "sequences": sequences,
                "code": frame.code,
            }
        )
    return {"frames": parsed}


def handle(base_url: str, request: Mapping[str, Any]) -> Any:
    """Dispatch one protocol request to the SDK adapter function."""
    command = request.get("command")
    params = request.get("params") or {}
    if command == "contract":
        return {"contract_version": CONTRACT_VERSION, "operation_count": len(ROUTES)}
    if command == "describe":
        return describe(params)
    if command == "call":
        return call(base_url, params)
    if command == "page":
        return page(base_url, params)
    if command == "stream":
        return stream(base_url, params)
    if command == "frames":
        return frames(params)
    raise ProtocolError(f"unknown command {command!r}")


def main() -> int:
    """Serve requests until stdin closes."""
    base_url = os.environ.get("TIDEPOOL_CONFORMANCE_BASE_URL")
    if not base_url:
        print(
            "python adapter: TIDEPOOL_CONFORMANCE_BASE_URL is not set",
            file=sys.stderr,
        )
        return 2
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except ValueError:
            print("python adapter: request is not JSON", file=sys.stderr)
            return 2
        request_id = request.get("id")
        try:
            result = handle(base_url, request)
        except ProblemError as error:
            response = {
                "id": request_id,
                "ok": False,
                "error": {
                    "type": "ProblemError",
                    "message": f"{error.operation_id} -> {error.status} {error.code}",
                },
            }
        except ProtocolError as error:
            response = {
                "id": request_id,
                "ok": False,
                "error": {"type": "ProtocolError", "message": str(error)},
            }
        except Exception as error:  # noqa: BLE001 - the adapter must answer every line
            print(f"python adapter: {error.__class__.__name__}", file=sys.stderr)
            response = {
                "id": request_id,
                "ok": False,
                "error": {
                    "type": error.__class__.__name__,
                    "message": "the SDK raised an unexpected error",
                },
            }
        else:
            response = {"id": request_id, "ok": True, "result": result}
        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
