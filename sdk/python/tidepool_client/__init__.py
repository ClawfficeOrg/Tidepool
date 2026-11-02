"""Typed Python client for the Tidepool API v1.

The package exposes two facades over the same contract, the same typed models
and the same retry and problem-details behaviour:

* :class:`tidepool_client.SyncClient` — the default client, standard library
  only, no event loop required;
* :class:`tidepool_client.AsyncClient` — the same surface as coroutines, built
  on :mod:`asyncio` streams, no third-party HTTP library.

``tidepool_client.ROUTES`` is the generated route table and every operation in
it is reachable from both facades, either through a generated resource method
(``client.hosts.list()``) or generically
(``client.call("get_v1_hosts")``).
"""

from __future__ import annotations

from . import models
from ._generated.routes import (
    CONTRACT_SHA256,
    CONTRACT_VERSION,
    ROLES,
    ROUTE_LIST,
    ROUTES,
    SCOPES,
    Route,
    route,
)
from .client import AsyncClient, SyncClient, TokenUpdate
from .config import CallOptions, ClientConfig, RetryPolicy
from .errors import (
    ConfigError,
    DecodeError,
    EncodeError,
    MissingBodyError,
    ProblemError,
    RetryAfterTooLongError,
    TidepoolError,
    TransportError,
    UnknownOperationError,
    UsageError,
)
from .paging import (
    aiter_activity,
    aiter_chat_messages,
    aiter_relay_events,
    iter_activity,
    iter_chat_messages,
    iter_relay_events,
)
from .stream import ChatFrame, ChatStream

__version__ = "0.1.0"

__all__ = [
    "AsyncClient",
    "CallOptions",
    "ChatFrame",
    "ChatStream",
    "CONTRACT_SHA256",
    "CONTRACT_VERSION",
    "ClientConfig",
    "ConfigError",
    "DecodeError",
    "EncodeError",
    "MissingBodyError",
    "ProblemError",
    "ROLES",
    "ROUTE_LIST",
    "ROUTES",
    "RetryAfterTooLongError",
    "RetryPolicy",
    "SCOPES",
    "Route",
    "SyncClient",
    "TidepoolError",
    "TokenUpdate",
    "TransportError",
    "UnknownOperationError",
    "UsageError",
    "__version__",
    "aiter_activity",
    "aiter_chat_messages",
    "aiter_relay_events",
    "iter_activity",
    "iter_chat_messages",
    "iter_relay_events",
    "models",
    "route",
]
