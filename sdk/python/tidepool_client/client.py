"""The synchronous and asynchronous Tidepool clients.

Both facades share one route table, one set of typed models and one request
assembly path. They differ only in the transport and in whether a call is a
coroutine, so a route reachable from one is reachable from the other.
"""

from __future__ import annotations

import asyncio
import dataclasses
import inspect
import json
import time
import warnings
from dataclasses import dataclass
from typing import Any, Awaitable, Callable, Dict, List, Mapping, Optional, Tuple
from urllib.parse import quote, urlencode

from ._generated.models import MODEL_TYPES, Model, PoolLoginResponse
from ._generated.resources import AsyncResources, SyncResources
from ._generated.routes import ROUTES, Route
from .config import CallOptions, ClientConfig
from .errors import (
    DecodeError,
    EncodeError,
    MissingBodyError,
    ProblemError,
    UnknownOperationError,
    UsageError,
)
from .problem import problem_from_response, retry_after_ms
from .stream import ChatStream
from .transport import AsyncTransport, PreparedRequest, RawResponse, SyncTransport


@dataclass(frozen=True)
class TokenUpdate:
    """A refreshed token, optionally with the lifetime the server reported."""

    token: str
    """The new operator token."""
    expires_in: Optional[float] = None
    """How long the token is usable, in seconds."""


RefreshHook = Callable[[], Any]
"""A synchronous refresh hook; returning ``None`` lets a 401 surface."""

AsyncRefreshHook = Callable[[], Awaitable[Any]]
"""An async refresh hook; returning ``None`` lets a 401 surface."""


@dataclass
class _AuthState:
    """The mutable part of a client: the credentials currently in effect."""

    token: Optional[str] = None
    api_key: Optional[str] = None
    host_credential: Optional[str] = None
    expires_at: Optional[float] = None

    @classmethod
    def from_config(cls, config: ClientConfig) -> "_AuthState":
        deadline = (
            time.monotonic() + config.token_expiry
            if config.token_expiry is not None
            else None
        )
        return cls(
            token=config.token,
            api_key=config.api_key,
            host_credential=config.host_credential,
            expires_at=deadline,
        )

    def headers(self) -> List[Tuple[str, str]]:
        """Ambient credential headers, in the contract's precedence."""
        headers: List[Tuple[str, str]] = []
        if self.token:
            headers.append(("authorization", f"Bearer {self.token}"))
        if self.api_key:
            headers.append(("x-api-key", self.api_key))
        if self.host_credential:
            headers.append(("x-host-credential", self.host_credential))
        return headers

    def expired(self) -> bool:
        """True when a token is set and its known lifetime has passed."""
        return (
            self.token is not None
            and self.expires_at is not None
            and time.monotonic() >= self.expires_at
        )

    def set_token(self, token: Optional[str], expires_in: Optional[float] = None) -> None:
        """Store a token, optionally with a lifetime."""
        self.token = token
        self.expires_at = (
            time.monotonic() + expires_in if expires_in and expires_in > 0 else None
        )


class _ClientBase:
    """Request assembly and decoding shared by both facades."""

    config: ClientConfig
    _auth: _AuthState
    _refresh: Any
    _transport: Any
    resources: Any

    def _configure(
        self,
        *,
        base_url: Optional[str],
        config: Optional[ClientConfig],
        token: Optional[str],
        token_expiry: Optional[float],
        api_key: Optional[str],
        host_credential: Optional[str],
        timeout: Optional[float],
        retry: Any,
        headers: Any,
        user_agent: Optional[str],
        refresh: Any,
    ) -> None:
        if config is not None:
            if base_url is not None:
                raise ValueError("pass either base_url or config, not both")
            self.config = config
        else:
            if not base_url:
                raise ValueError("base_url is required")
            self.config = ClientConfig(
                base_url=base_url,
                token=token,
                token_expiry=token_expiry,
                api_key=api_key,
                host_credential=host_credential,
                headers=headers if headers is not None else (),
                retry=retry if retry is not None else _default_retry(),
                timeout=timeout if timeout is not None else 30.0,
                user_agent=user_agent or f"tidepool-client-py/{_version()}",
            )
        self._auth = _AuthState.from_config(self.config)
        self._refresh = refresh

    @property
    def token(self) -> Optional[str]:
        """The operator token currently in effect."""
        return self._auth.token

    def set_token(self, token: str, expires_in: Optional[float] = None) -> None:
        """Adopt an operator token, optionally with a lifetime."""
        self._auth.set_token(token, expires_in)

    def set_api_key(self, api_key: Optional[str]) -> None:
        """Replace the API key sent as ``x-api-key``."""
        self._auth.api_key = api_key

    def set_host_credential(self, credential: Optional[str]) -> None:
        """Replace the host credential sent as ``X-Host-Credential``."""
        self._auth.host_credential = credential

    def _route(self, operation_id: str) -> Route:
        route = ROUTES.get(operation_id)
        if route is None:
            raise UnknownOperationError(operation_id)
        return route

    def _prepare(
        self,
        route: Route,
        path_params: Optional[Mapping[str, Any]],
        query: Optional[Mapping[str, Any]],
        body: Any,
        options: CallOptions,
    ) -> PreparedRequest:
        path = self._path(route, path_params or {})
        headers: Dict[str, str] = {}
        _set_header(headers, "accept", "application/json")
        _set_header(headers, "user-agent", self.config.user_agent)
        for name, value in self.config.headers:
            _set_header(headers, name, value)
        for name, value in self._auth.headers():
            _set_header(headers, name, value)
        for name, value in options.headers:
            _set_header(headers, name, value)
        if options.request_id:
            _set_header(headers, "x-request-id", options.request_id)
        encoded = _encode_body(body)
        if encoded is not None and "content-type" not in headers:
            _set_header(headers, "content-type", "application/json")
        url = self.config.base_url + path + _query_suffix(query or {})
        return PreparedRequest(
            method=route.method,
            url=url,
            headers=tuple(headers.items()),
            body=encoded,
        )

    def _path(self, route: Route, path_params: Mapping[str, Any]) -> str:
        path = route.path
        for name in route.path_params:
            if name not in path_params or path_params[name] is None:
                raise UsageError(
                    f"{route.operation_id}: path parameter {name!r} is required"
                )
            path = path.replace("{" + name + "}", quote(str(path_params[name]), safe=""))
        return path

    def _stream(
        self,
        operation_id: str,
        path_params: Optional[Mapping[str, Any]],
        query: Optional[Mapping[str, Any]],
    ) -> ChatStream:
        route = self._route(operation_id)
        if route.response_kind != "stream":
            raise UsageError(f"{operation_id} is not a stream route")
        values: Dict[str, Any] = dict(query or {})
        token = values.get("token") or self._auth.token
        if not token:
            raise UsageError(f"{operation_id}: a token is required for the live stream")
        values["token"] = token
        after = values.get("after")
        after_value = int(after) if isinstance(after, (int, float)) else None
        path = self._path(route, path_params or {})
        scheme = "wss" if self.config.base_url.startswith("https") else "ws"
        netloc = self.config.base_url.split("://", 1)[1]
        url = f"{scheme}://{netloc}{path}{_query_suffix(values)}"
        return ChatStream(url=url, token=str(token), after=after_value)

    def _decode(self, route: Route, response: RawResponse) -> Any:
        if route.response_kind == "empty":
            return None
        if route.response_kind == "text":
            return response.body.decode("utf-8", errors="replace")
        if not response.body:
            raise MissingBodyError(route.operation_id, response.status)
        try:
            payload = json.loads(response.body)
        except (ValueError, UnicodeDecodeError) as error:
            raise DecodeError(route.operation_id, str(error)) from error
        if route.response_kind == "json":
            return payload
        if route.response_kind == "model":
            return _model(route.operation_id, route.response_models[0], payload)
        if route.response_kind == "union":
            return _model(route.operation_id, _best_model(route, payload), payload)
        return payload

    def _decode_json(self, route: Route, response: RawResponse) -> Any:
        if not response.body:
            return None
        try:
            return json.loads(response.body)
        except (ValueError, UnicodeDecodeError) as error:
            raise DecodeError(route.operation_id, str(error)) from error

    def _problem(
        self, route: Route, response: RawResponse, attempts: int
    ) -> ProblemError:
        return problem_from_response(
            operation_id=route.operation_id,
            status=response.status,
            headers=response.headers,
            body=response.body,
            attempts=attempts,
        )

    def _adopt(self, update: Any) -> bool:
        if update is None:
            return False
        if isinstance(update, TokenUpdate):
            self._auth.set_token(update.token, update.expires_in)
            return True
        if isinstance(update, str):
            self._auth.set_token(update)
            return True
        if isinstance(update, Mapping) and isinstance(update.get("token"), str):
            expires = update.get("expires_in")
            self._auth.set_token(
                update["token"], float(expires) if isinstance(expires, (int, float)) else None
            )
            return True
        raise UsageError(
            "refresh hook must return TokenUpdate, a token string, a mapping with "
            "'token', or None"
        )


class SyncClient(_ClientBase):
    """Blocking client over the whole contract.

    Args:
        base_url: Absolute pool URL (``http`` or ``https``).
        config: A :class:`ClientConfig` instead of ``base_url`` and the
            individual credential arguments.
        token: Operator JWT sent as ``Authorization: Bearer``.
        token_expiry: Known token lifetime in seconds, for proactive refresh.
        api_key: API key sent as ``x-api-key``.
        host_credential: Host credential sent as ``X-Host-Credential``.
        timeout: Per-request timeout in seconds (default 30).
        retry: A :class:`RetryPolicy` (default: two 429 retries).
        headers: Extra headers sent with every request.
        user_agent: Override the ``User-Agent`` header.
        refresh: Called at most once per request, proactively when the token
            lifetime has passed and reactively on a 401. Returning ``None``
            lets the 401 surface, so a stale credential cannot loop.
        transport: An optional transport, for tests.
    """

    def __init__(
        self,
        base_url: Optional[str] = None,
        *,
        config: Optional[ClientConfig] = None,
        token: Optional[str] = None,
        token_expiry: Optional[float] = None,
        api_key: Optional[str] = None,
        host_credential: Optional[str] = None,
        timeout: Optional[float] = None,
        retry: Any = None,
        headers: Optional[Any] = None,
        user_agent: Optional[str] = None,
        refresh: Optional[RefreshHook] = None,
        transport: Optional[SyncTransport] = None,
    ) -> None:
        self._configure(
            base_url=base_url,
            config=config,
            token=token,
            token_expiry=token_expiry,
            api_key=api_key,
            host_credential=host_credential,
            timeout=timeout,
            retry=retry,
            headers=headers,
            user_agent=user_agent,
            refresh=refresh,
        )
        self._transport = transport or SyncTransport()
        self.resources = SyncResources(self)
        for name, value in vars(self.resources).items():
            if not name.startswith("_"):
                setattr(self, name, value)

    def call(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        body: Any = None,
        options: Optional[CallOptions] = None,
    ) -> Any:
        """Call one contract operation and decode its success body.

        Args:
            operation_id: The contract's operation id.
            path_params: Values for the route's ``{placeholders}``.
            query: Query parameters; ``None`` values are omitted.
            body: A model or JSON-compatible mapping for routes with a body.
            options: Per-call headers, timeout or retry override.

        Returns:
            The typed model the contract declares, a dict for untyped bodies,
            ``None`` for an empty body, or a :class:`ChatStream` for a stream
            route.

        Raises:
            UnknownOperationError: When the id is not in the contract.
            ProblemError: For every non-2xx response.
        """
        route = self._route(operation_id)
        if route.deprecated:
            warnings.warn(
                f"the contract marks {operation_id} deprecated",
                DeprecationWarning,
                stacklevel=2,
            )
        options = options or CallOptions()
        policy = options.effective_retry(self.config)
        timeout = options.effective_timeout(self.config)
        attempts = 0
        refresh_attempted = False
        if self._auth.expired() and self._refresh is not None:
            refresh_attempted = True
            self._refresh_sync()
        while True:
            attempts += 1
            request = self._prepare(route, path_params, query, body, options)
            response = self._transport.send(request, timeout)
            if response.status == 401 and not refresh_attempted:
                refresh_attempted = True
                if self._refresh_sync():
                    continue
            if response.status == 429 and attempts <= policy.max_retries:
                requested = retry_after_ms(response.headers.get("retry-after"))
                time.sleep(
                    policy.delay_seconds(
                        attempts - 1, requested, operation_id=route.operation_id
                    )
                )
                continue
            if 200 <= response.status < 300:
                return self._decode(route, response)
            raise self._problem(route, response, attempts)

    def request_json(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        body: Any = None,
        options: Optional[CallOptions] = None,
    ) -> Any:
        """Call one operation by id and return the raw decoded JSON body."""
        route = self._route(operation_id)
        options = options or CallOptions()
        policy = options.effective_retry(self.config)
        timeout = options.effective_timeout(self.config)
        attempts = 0
        refresh_attempted = False
        if self._auth.expired() and self._refresh is not None:
            refresh_attempted = True
            self._refresh_sync()
        while True:
            attempts += 1
            request = self._prepare(route, path_params, query, body, options)
            response = self._transport.send(request, timeout)
            if response.status == 401 and not refresh_attempted:
                refresh_attempted = True
                if self._refresh_sync():
                    continue
            if response.status == 429 and attempts <= policy.max_retries:
                requested = retry_after_ms(response.headers.get("retry-after"))
                time.sleep(
                    policy.delay_seconds(
                        attempts - 1, requested, operation_id=route.operation_id
                    )
                )
                continue
            if 200 <= response.status < 300:
                return self._decode_json(route, response)
            raise self._problem(route, response, attempts)

    def login(self, email: str, password: str) -> PoolLoginResponse:
        """Sign in and adopt the returned token and lifetime."""
        response = self.call(
            "post_v1_pool_login", body={"email": email, "password": password}
        )
        if not isinstance(response, PoolLoginResponse):
            raise DecodeError("post_v1_pool_login", "unexpected response model")
        self._auth.set_token(response.token, float(response.expires_in))
        return response

    def stream(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        token: Optional[str] = None,
    ) -> ChatStream:
        """Build the typed live-stream handle for one room.

        No socket is opened. The handle carries the token in its URL because the
        pool's socket route expects it there; treat the whole URL as a secret.
        """
        values = dict(query or {})
        if token is not None:
            values["token"] = token
        return self._stream(operation_id, path_params, values)

    def _refresh_sync(self) -> bool:
        if self._refresh is None:
            return False
        result = self._refresh()
        if inspect.isawaitable(result):
            raise UsageError("the sync client's refresh hook must not return an awaitable")
        return self._adopt(result)


class AsyncClient(_ClientBase):
    """Asynchronous client over the whole contract.

    The arguments and the behaviour are the same as :class:`SyncClient`; every
    request method is a coroutine. The refresh hook may be a coroutine function.
    """

    def __init__(
        self,
        base_url: Optional[str] = None,
        *,
        config: Optional[ClientConfig] = None,
        token: Optional[str] = None,
        token_expiry: Optional[float] = None,
        api_key: Optional[str] = None,
        host_credential: Optional[str] = None,
        timeout: Optional[float] = None,
        retry: Any = None,
        headers: Optional[Any] = None,
        user_agent: Optional[str] = None,
        refresh: Optional[AsyncRefreshHook] = None,
        transport: Optional[AsyncTransport] = None,
    ) -> None:
        self._configure(
            base_url=base_url,
            config=config,
            token=token,
            token_expiry=token_expiry,
            api_key=api_key,
            host_credential=host_credential,
            timeout=timeout,
            retry=retry,
            headers=headers,
            user_agent=user_agent,
            refresh=refresh,
        )
        self._transport = transport or AsyncTransport()
        self.resources = AsyncResources(self)
        for name, value in vars(self.resources).items():
            if not name.startswith("_"):
                setattr(self, name, value)

    async def call(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        body: Any = None,
        options: Optional[CallOptions] = None,
    ) -> Any:
        """Call one contract operation and decode its success body.

        The arguments are the same as :meth:`SyncClient.call`; the request is
        awaited.
        """
        route = self._route(operation_id)
        if route.deprecated:
            warnings.warn(
                f"the contract marks {operation_id} deprecated",
                DeprecationWarning,
                stacklevel=2,
            )
        options = options or CallOptions()
        policy = options.effective_retry(self.config)
        timeout = options.effective_timeout(self.config)
        attempts = 0
        refresh_attempted = False
        if self._auth.expired() and self._refresh is not None:
            refresh_attempted = True
            await self._refresh_async()
        while True:
            attempts += 1
            request = self._prepare(route, path_params, query, body, options)
            response = await self._transport.send(request, timeout)
            if response.status == 401 and not refresh_attempted:
                refresh_attempted = True
                if await self._refresh_async():
                    continue
            if response.status == 429 and attempts <= policy.max_retries:
                requested = retry_after_ms(response.headers.get("retry-after"))
                await asyncio.sleep(
                    policy.delay_seconds(
                        attempts - 1, requested, operation_id=route.operation_id
                    )
                )
                continue
            if 200 <= response.status < 300:
                return self._decode(route, response)
            raise self._problem(route, response, attempts)

    async def request_json(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        body: Any = None,
        options: Optional[CallOptions] = None,
    ) -> Any:
        """Call one operation by id and return the raw decoded JSON body."""
        route = self._route(operation_id)
        options = options or CallOptions()
        policy = options.effective_retry(self.config)
        timeout = options.effective_timeout(self.config)
        attempts = 0
        refresh_attempted = False
        if self._auth.expired() and self._refresh is not None:
            refresh_attempted = True
            await self._refresh_async()
        while True:
            attempts += 1
            request = self._prepare(route, path_params, query, body, options)
            response = await self._transport.send(request, timeout)
            if response.status == 401 and not refresh_attempted:
                refresh_attempted = True
                if await self._refresh_async():
                    continue
            if response.status == 429 and attempts <= policy.max_retries:
                requested = retry_after_ms(response.headers.get("retry-after"))
                await asyncio.sleep(
                    policy.delay_seconds(
                        attempts - 1, requested, operation_id=route.operation_id
                    )
                )
                continue
            if 200 <= response.status < 300:
                return self._decode_json(route, response)
            raise self._problem(route, response, attempts)

    async def login(self, email: str, password: str) -> PoolLoginResponse:
        """Sign in and adopt the returned token and lifetime."""
        response = await self.call(
            "post_v1_pool_login", body={"email": email, "password": password}
        )
        if not isinstance(response, PoolLoginResponse):
            raise DecodeError("post_v1_pool_login", "unexpected response model")
        self._auth.set_token(response.token, float(response.expires_in))
        return response

    def stream(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, Any]] = None,
        query: Optional[Mapping[str, Any]] = None,
        token: Optional[str] = None,
    ) -> ChatStream:
        """Build the typed live-stream handle for one room.

        No socket is opened; the handle is the same as the sync client's.
        """
        values = dict(query or {})
        if token is not None:
            values["token"] = token
        return self._stream(operation_id, path_params, values)

    async def _refresh_async(self) -> bool:
        if self._refresh is None:
            return False
        result = self._refresh()
        if inspect.isawaitable(result):
            result = await result
        return self._adopt(result)


def _default_retry() -> Any:
    from .retry import RetryPolicy

    return RetryPolicy()


def _version() -> str:
    from ._version import __version__

    return __version__


def _encode_body(body: Any) -> Optional[bytes]:
    """Encode a request body, or return ``None`` when there is none."""
    if body is None:
        return None
    if isinstance(body, Model):
        body = body.to_dict()
    elif isinstance(body, bytes):
        return body
    try:
        return json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise EncodeError(f"request body could not be encoded as JSON: {error}") from error


def _query_suffix(query: Mapping[str, Any]) -> str:
    """Encode a query mapping, omitting ``None`` values."""
    pairs: List[Tuple[str, str]] = []
    for name, value in query.items():
        if value is None:
            continue
        if isinstance(value, bool):
            pairs.append((name, "true" if value else "false"))
        else:
            pairs.append((name, str(value)))
    if not pairs:
        return ""
    return "?" + urlencode(pairs, quote_via=quote)


def _set_header(headers: Dict[str, str], name: str, value: str) -> None:
    """Set a header, replacing any previous value with the same lower-case name."""
    text = str(value)
    if "\r" in text or "\n" in text:
        raise UsageError(f"invalid value for header {name!r}: newlines are not allowed")
    for existing in list(headers):
        if existing.lower() == name.lower():
            del headers[existing]
    headers[name.lower()] = text


def _model(operation_id: str, name: str, payload: Any) -> Any:
    """Parse a payload into a named generated model."""
    model = MODEL_TYPES.get(name)
    if model is None:
        raise DecodeError(operation_id, f"unknown model {name!r}")
    try:
        return model.from_dict(payload)
    except (TypeError, ValueError) as error:
        raise DecodeError(operation_id, str(error)) from error


def _best_model(route: Route, payload: Any) -> str:
    """Pick the union member whose members the payload carries most of."""
    if not isinstance(payload, dict):
        return route.response_models[0]
    best_name = route.response_models[0]
    best_score = -1
    for name in route.response_models:
        model = MODEL_TYPES.get(name)
        if model is None:
            continue
        known = model.json_names()
        score = 0
        for item in dataclasses.fields(model):
            json_name = known.get(item.name, item.name)
            if json_name in payload:
                score += 1
        if score > best_score:
            best_name, best_score = name, score
    return best_name
