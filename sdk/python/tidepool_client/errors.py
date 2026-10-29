"""Errors raised by the Tidepool client.

Every non-2xx response becomes :class:`ProblemError`, which flattens the RFC
9457 problem document into typed attributes. An unknown ``code`` is preserved
verbatim, as the contract requires, so an additive server change cannot break a
caller that switches on the codes it knows.
"""

from __future__ import annotations

from typing import Any, Optional


class TidepoolError(Exception):
    """Base class for every error this package raises."""


class ConfigError(TidepoolError, ValueError):
    """The client configuration is not usable."""


class UsageError(TidepoolError, ValueError):
    """The caller used the SDK in a way the contract does not allow."""


class UnknownOperationError(TidepoolError, LookupError):
    """The operation id is not in the committed contract."""

    def __init__(self, operation_id: str) -> None:
        super().__init__(f"operation {operation_id!r} is not in the contract")
        self.operation_id = operation_id


class TransportError(TidepoolError):
    """The transport failed before an HTTP response existed."""


class EncodeError(TidepoolError):
    """A request body could not be encoded as JSON."""


class DecodeError(TidepoolError):
    """A success response body was not the JSON the contract declares."""

    def __init__(self, operation_id: str, detail: str) -> None:
        super().__init__(
            f"response body for {operation_id!r} was not the JSON the contract "
            f"declares: {detail}"
        )
        self.operation_id = operation_id


class MissingBodyError(TidepoolError):
    """A success response carried no body where the contract declares one."""

    def __init__(self, operation_id: str, status: int) -> None:
        super().__init__(
            f"{operation_id!r} returned {status} without the body the contract declares"
        )
        self.operation_id = operation_id
        self.status = status


class RetryAfterTooLongError(TidepoolError):
    """The server asked for a retry delay above the configured ceiling."""

    def __init__(self, operation_id: str, requested_ms: float, limit_ms: float) -> None:
        super().__init__(
            f"server asked to retry {operation_id!r} after {requested_ms:g} ms, above "
            f"the {limit_ms:g} ms ceiling; surfacing the rate limit instead"
        )
        self.operation_id = operation_id
        self.requested_ms = requested_ms
        self.limit_ms = limit_ms


class ProblemError(TidepoolError):
    """A non-2xx response, with the problem document flattened into attributes.

    Attributes:
        status: HTTP status of the failed response.
        code: Stable machine-readable problem code, verbatim.
        detail: Human-readable explanation, when the server sent one.
        title: Short human-readable summary, when the server sent one.
        problem_type: Problem type URI, when the server sent one.
        request_id: Correlation id from the body or ``x-request-id``.
        operation_id: Operation id that produced the failure.
        attempts: HTTP attempts that produced this error (1 means no retry).
        retry_after_ms: Server-requested delay from a 429, in milliseconds.
        body: Parsed response body, preserved for diagnostics.
    """

    def __init__(
        self,
        *,
        operation_id: str,
        status: int,
        code: str,
        detail: Optional[str] = None,
        title: Optional[str] = None,
        problem_type: Optional[str] = None,
        request_id: Optional[str] = None,
        attempts: int = 1,
        retry_after_ms: Optional[float] = None,
        body: Any = None,
    ) -> None:
        self.operation_id = operation_id
        self.status = status
        self.code = code
        self.detail = detail
        self.title = title
        self.problem_type = problem_type
        self.request_id = request_id
        self.attempts = attempts
        self.retry_after_ms = retry_after_ms
        self.body = body
        super().__init__(str(self))

    def __str__(self) -> str:
        message = f"{self.operation_id} -> {self.status} {self.code}"
        if self.detail:
            message += f": {self.detail}"
        if self.request_id:
            message += f" (requestId {self.request_id})"
        return message

    @property
    def rate_limited(self) -> bool:
        """True when the failure is a rate limit."""
        return self.status == 429
