"""Client configuration and per-call overrides."""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field
from typing import Mapping, Optional, Tuple
from urllib.parse import urlsplit

from ._version import __version__
from .errors import ConfigError
from .retry import RetryPolicy

HeaderPairs = Tuple[Tuple[str, str], ...]


def normalise_base_url(value: str) -> str:
    """Validate an absolute pool URL and strip its trailing slash.

    Args:
        value: The pool base URL.

    Returns:
        The normalised URL, without a trailing slash.

    Raises:
        ConfigError: When the URL is not absolute ``http`` or ``https``.
    """
    if not isinstance(value, str) or not value.strip():
        raise ConfigError("base_url must be a non-empty string")
    text = value.strip()
    parts = urlsplit(text)
    if parts.scheme not in ("http", "https"):
        raise ConfigError(f"base_url must be absolute http or https, got {value!r}")
    if not parts.netloc:
        raise ConfigError(f"base_url must include a host, got {value!r}")
    if parts.query or parts.fragment:
        raise ConfigError("base_url must not carry a query string or fragment")
    return text.rstrip("/")


@dataclass(frozen=True)
class ClientConfig:
    """Everything a client needs before the first request.

    The defaults are the same across the three SDKs: a 30-second timeout, two
    retries with exponential backoff, and a user agent that names the package.
    """

    base_url: str
    token: Optional[str] = None
    token_expiry: Optional[float] = None
    api_key: Optional[str] = None
    host_credential: Optional[str] = None
    headers: HeaderPairs = ()
    retry: RetryPolicy = field(default_factory=RetryPolicy)
    timeout: float = 30.0
    user_agent: str = f"tidepool-client-py/{__version__}"

    def __post_init__(self) -> None:
        object.__setattr__(self, "base_url", normalise_base_url(self.base_url))
        if not isinstance(self.headers, tuple):
            object.__setattr__(
                self,
                "headers",
                tuple(self.headers.items())
                if isinstance(self.headers, Mapping)
                else tuple(self.headers),
            )
        if self.timeout <= 0:
            raise ConfigError("timeout must be positive")
        if self.token_expiry is not None and self.token_expiry < 0:
            raise ConfigError("token_expiry must not be negative")
        for name, value in self.headers:
            _validate_header(name, value)

    def with_token(self, token: str, expires_in: Optional[float] = None) -> "ClientConfig":
        """Return a copy carrying an operator JWT."""
        return dataclasses.replace(self, token=token, token_expiry=expires_in)

    def with_api_key(self, api_key: str) -> "ClientConfig":
        """Return a copy carrying an API key."""
        return dataclasses.replace(self, api_key=api_key)

    def with_host_credential(self, credential: str) -> "ClientConfig":
        """Return a copy carrying a host relationship credential."""
        return dataclasses.replace(self, host_credential=credential)

    def with_header(self, name: str, value: str) -> "ClientConfig":
        """Return a copy with one extra header."""
        _validate_header(name, value)
        return dataclasses.replace(self, headers=self.headers + ((name, value),))

    def with_retry(self, retry: RetryPolicy) -> "ClientConfig":
        """Return a copy with a different retry policy."""
        return dataclasses.replace(self, retry=retry)

    def with_timeout(self, timeout: float) -> "ClientConfig":
        """Return a copy with a different per-request timeout."""
        return dataclasses.replace(self, timeout=timeout)


@dataclass(frozen=True)
class CallOptions:
    """Per-call overrides for one request."""

    request_id: Optional[str] = None
    """Correlation id sent as ``x-request-id``."""
    headers: HeaderPairs = ()
    """Extra headers, applied after the client's and the credentials'."""
    timeout: Optional[float] = None
    """Per-call timeout; defaults to the client's timeout."""
    retry: Optional[RetryPolicy] = None
    """Per-call retry policy; defaults to the client's policy."""

    def __post_init__(self) -> None:
        if not isinstance(self.headers, tuple):
            object.__setattr__(
                self,
                "headers",
                tuple(self.headers.items())
                if isinstance(self.headers, Mapping)
                else tuple(self.headers),
            )
        if self.timeout is not None and self.timeout <= 0:
            raise ConfigError("timeout must be positive")
        for name, value in self.headers:
            _validate_header(name, value)

    def with_request_id(self, request_id: str) -> "CallOptions":
        """Return a copy that sends this correlation id."""
        return dataclasses.replace(self, request_id=request_id)

    def with_header(self, name: str, value: str) -> "CallOptions":
        """Return a copy with one extra header."""
        _validate_header(name, value)
        return dataclasses.replace(self, headers=self.headers + ((name, value),))

    def with_timeout(self, timeout: float) -> "CallOptions":
        """Return a copy with a different timeout."""
        return dataclasses.replace(self, timeout=timeout)

    def with_retry(self, retry: RetryPolicy) -> "CallOptions":
        """Return a copy with a different retry policy."""
        return dataclasses.replace(self, retry=retry)

    def without_retry(self) -> "CallOptions":
        """Return a copy that sends the request once, with no retry."""
        return dataclasses.replace(self, retry=RetryPolicy.none())

    def effective_timeout(self, config: ClientConfig) -> float:
        """The timeout in effect for this call."""
        return self.timeout if self.timeout is not None else config.timeout

    def effective_retry(self, config: ClientConfig) -> RetryPolicy:
        """The retry policy in effect for this call."""
        return self.retry if self.retry is not None else config.retry


def _validate_header(name: str, value: str) -> None:
    """Reject a header that could inject a second request line."""
    if not name or any(character in name for character in "\r\n:"):
        raise ConfigError(f"invalid header name: {name!r}")
    if any(character in str(value) for character in "\r\n"):
        raise ConfigError(f"invalid value for header {name!r}: newlines are not allowed")
