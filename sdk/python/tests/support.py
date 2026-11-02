"""Test support: the contract, schema samples and a recorded HTTP server.

Nothing here needs a running pool, MySQL or Redis. The recorded server answers
the exact exchanges each test scripts, and the sample builder derives request
and response bodies from the committed contract so route coverage is generated
rather than hand-maintained.
"""

from __future__ import annotations

import json
import threading
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable, Dict, List, Mapping, Optional, Tuple
from urllib.parse import parse_qs, urlsplit

REPOSITORY = Path(__file__).resolve().parents[3]
CONTRACT_PATH = REPOSITORY / "docs/api/openapi-v1.json"
REF_PREFIX = "#/components/schemas/"

with CONTRACT_PATH.open(encoding="utf-8") as handle:
    CONTRACT = json.load(handle)

SCHEMAS: Dict[str, Any] = CONTRACT["components"]["schemas"]

ResponseTuple = Tuple[int, Mapping[str, str], Any]
Responder = Callable[["RecordedRequest"], ResponseTuple]


@dataclass
class RecordedRequest:
    """One request the recorded server received."""

    method: str
    path: str
    query: Dict[str, str]
    headers: Dict[str, str]
    body: bytes
    captured: List[Any] = field(default_factory=list)

    @property
    def json(self) -> Any:
        """The request body decoded as JSON, or ``None``."""
        if not self.body:
            return None
        try:
            return json.loads(self.body)
        except ValueError:
            return None


class RecordedServer:
    """A threading HTTP server that scripts each response."""

    def __init__(self, responder: Responder) -> None:
        self.responder = responder
        self.requests: List[RecordedRequest] = []
        self._lock = threading.Lock()
        self._httpd = ThreadingHTTPServer(("127.0.0.1", 0), self._handler())
        self._httpd.daemon_threads = True
        self._thread = threading.Thread(target=self._httpd.serve_forever, daemon=True)

    @property
    def base_url(self) -> str:
        """The server's ``http://host:port`` URL."""
        host, port = self._httpd.server_address[:2]
        return f"http://{host}:{port}"

    def __enter__(self) -> "RecordedServer":
        self._thread.start()
        return self

    def __exit__(self, *exc_info: Any) -> None:
        self._httpd.shutdown()
        self._httpd.server_close()
        self._thread.join(timeout=5)

    def reset(self) -> None:
        """Forget the requests recorded so far."""
        with self._lock:
            self.requests.clear()

    def _handler(self):
        server = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_GET(self) -> None:
                self._dispatch("GET")

            def do_POST(self) -> None:
                self._dispatch("POST")

            def do_PUT(self) -> None:
                self._dispatch("PUT")

            def do_PATCH(self) -> None:
                self._dispatch("PATCH")

            def do_DELETE(self) -> None:
                self._dispatch("DELETE")

            def _dispatch(self, method: str) -> None:
                length = int(self.headers.get("content-length") or 0)
                body = self.rfile.read(length) if length else b""
                parts = urlsplit(self.path)
                query = {
                    name: values[-1]
                    for name, values in parse_qs(parts.query, keep_blank_values=True).items()
                }
                request = RecordedRequest(
                    method=method,
                    path=parts.path,
                    query=query,
                    headers={name.lower(): value for name, value in self.headers.items()},
                    body=body,
                )
                with server._lock:
                    server.requests.append(request)
                status, headers, payload = server.responder(request)
                data = _payload_bytes(payload)
                self.send_response(status)
                for name, value in headers.items():
                    self.send_header(name, value)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                if data:
                    self.wfile.write(data)

            def log_message(self, *args: Any) -> None:
                pass

        return Handler


def _payload_bytes(payload: Any) -> bytes:
    """Render a responder payload as response bytes."""
    if isinstance(payload, bytes):
        return payload
    if isinstance(payload, str):
        return payload.encode("utf-8")
    return json.dumps(payload).encode("utf-8")


def json_response(
    status: int, payload: Any, headers: Optional[Mapping[str, str]] = None
) -> ResponseTuple:
    """A JSON response tuple."""
    merged = {"Content-Type": "application/json"}
    if headers:
        merged.update(headers)
    return status, merged, payload


def problem_response(
    status: int, code: str, detail: str = "", headers: Optional[Mapping[str, str]] = None
) -> ResponseTuple:
    """An RFC 9457 problem-details response tuple."""
    payload = {"type": "https://tidepool.test/problems", "status": status, "code": code}
    if detail:
        payload["detail"] = detail
    return json_response(status, payload, headers)


def operation(operation_id: str) -> Dict[str, Any]:
    """The contract operation with this id."""
    for operations in CONTRACT["paths"].values():
        for method, item in operations.items():
            if method not in ("get", "post", "put", "patch", "delete"):
                continue
            if item.get("operationId") == operation_id:
                return item
    raise KeyError(operation_id)


def sample_schema(schema: Optional[Dict[str, Any]], depth: int = 0) -> Any:
    """Build a contract-shaped sample value for a schema.

    Every declared property is populated so model round-trip tests exercise the
    whole shape. Recursion is bounded for safety.
    """
    if not isinstance(schema, dict) or not schema:
        return {"sample": True}
    if depth > 8:
        return None
    if "$ref" in schema:
        name = schema["$ref"][len(REF_PREFIX) :]
        return sample_schema(SCHEMAS[name], depth + 1)
    types = schema.get("type")
    candidates = types if isinstance(types, list) else [types]
    kind = next((item for item in candidates if item != "null"), None)
    if schema.get("enum"):
        return schema["enum"][0]
    if kind == "string":
        return "2026-10-29T12:00:00Z" if schema.get("format") == "date-time" else "text"
    if kind == "integer":
        return 7
    if kind == "number":
        return 1.5
    if kind == "boolean":
        return True
    if kind == "array":
        return [sample_schema(schema.get("items") or {}, depth + 1)]
    if kind == "object" or "properties" in schema:
        if "properties" in schema:
            return {
                name: sample_schema(prop, depth + 1)
                for name, prop in schema["properties"].items()
            }
        additional = schema.get("additionalProperties")
        if isinstance(additional, dict):
            return {"key": sample_schema(additional, depth + 1)}
        return {
            key: value
            for key, value in (("ok", True), ("status", "ok"))
        }
    return {"sample": True}


def sample_response(operation_id: str) -> Any:
    """Build a success body for one operation from the contract."""
    item = operation(operation_id)
    for status, response in sorted(item.get("responses", {}).items()):
        if not status.isdigit() or not 200 <= int(status) < 300:
            continue
        document = (response.get("content") or {}).get("application/json")
        if document:
            return sample_schema(document.get("schema") or {})
    return None


def sample_request(operation_id: str) -> Any:
    """Build a request body for one operation, or ``None`` when it has none."""
    item = operation(operation_id)
    document = (item.get("requestBody") or {}).get("content", {}).get(
        "application/json"
    )
    if document:
        return sample_schema(document.get("schema") or {})
    if item.get("requestBody"):
        return {}
    return None


def success_status(operation_id: str) -> int:
    """The lowest success status the contract declares for an operation."""
    item = operation(operation_id)
    statuses = sorted(
        int(status)
        for status in item.get("responses", {})
        if status.isdigit() and 200 <= int(status) < 300
    )
    return statuses[0] if statuses else 200


def route_path_params(operation_id: str) -> List[str]:
    """The path parameter names of an operation."""
    return [
        parameter["name"]
        for parameter in operation(operation_id).get("parameters") or []
        if parameter.get("in") == "path"
    ]


def resolve_resource(resources: Any, route: Any) -> Any:
    """Resolve a route's generated method on a client's resource tree."""
    node = resources
    for segment in route.namespace:
        node = getattr(node, segment)
    return getattr(node, route.name)


NUMERIC_QUERY = {"after", "limit", "cursor", "page", "per_page"}


def route_arguments(route: Any) -> Dict[str, Any]:
    """Keyword arguments that call a generated route method successfully."""
    arguments: Dict[str, Any] = {name: f"{name}_1" for name in route.path_params}
    if route.has_body:
        arguments["body"] = sample_request(route.operation_id)
    for name in route.query_params:
        arguments[name] = 3 if name in NUMERIC_QUERY else "value"
    return arguments


def resolved_path(route: Any, arguments: Mapping[str, Any]) -> str:
    """The concrete request path for a route and its arguments."""
    path = route.path
    for name in route.path_params:
        path = path.replace("{" + name + "}", str(arguments[name]))
    return path


def coverage_table(route_list: Any) -> Dict[Tuple[str, str], Tuple[int, Any]]:
    """A recorded-server routing table covering every non-stream route."""
    table: Dict[Tuple[str, str], Tuple[int, Any]] = {}
    for route in route_list:
        if route.response_kind == "stream":
            continue
        arguments = route_arguments(route)
        key = (route.method, resolved_path(route, arguments))
        if route.response_kind == "empty":
            table[key] = (success_status(route.operation_id), b"")
        elif route.response_kind == "text":
            table[key] = (200, "ok")
        else:
            table[key] = (
                success_status(route.operation_id),
                sample_response(route.operation_id),
            )
    return table


def coverage_responder(table: Dict[Tuple[str, str], Tuple[int, Any]]) -> Responder:
    """A responder that answers from a coverage table."""

    def respond(request: RecordedRequest) -> ResponseTuple:
        entry = table.get((request.method, request.path))
        if entry is None:
            return problem_response(404, "not_found", "unexpected route")
        status, payload = entry
        if status == 204:
            return status, {}, b""
        return json_response(status, payload)

    return respond


def assert_route_result(route: Any, result: Any) -> None:
    """Assert a decoded result matches the route's declared success kind."""
    from tidepool_client import ChatStream
    from tidepool_client._generated.models import MODEL_TYPES

    if route.response_kind == "empty":
        assert result is None, route.operation_id
    elif route.response_kind == "text":
        assert isinstance(result, str), route.operation_id
    elif route.response_kind == "json":
        assert isinstance(result, dict), route.operation_id
    elif route.response_kind == "model":
        assert isinstance(result, MODEL_TYPES[route.response_models[0]]), route.operation_id
    elif route.response_kind == "union":
        expected = tuple(MODEL_TYPES[name] for name in route.response_models)
        assert isinstance(result, expected), route.operation_id
    elif route.response_kind == "stream":
        assert isinstance(result, ChatStream), route.operation_id
