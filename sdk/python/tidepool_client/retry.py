"""Bounded retry policy for rate-limited requests.

Only ``429`` responses are retried: a 429 means the request was not processed,
so replaying it cannot duplicate a write even for a non-idempotent command. The
loop is always bounded by :attr:`RetryPolicy.max_retries`, and a
``Retry-After`` longer than :attr:`RetryPolicy.max_retry_after_ms` is surfaced
instead of slept through, the same policy as the Rust and TypeScript SDKs.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Optional

from .errors import RetryAfterTooLongError


@dataclass(frozen=True)
class RetryPolicy:
    """One retry budget."""

    max_retries: int = 2
    """Retries after the first attempt. Never unbounded."""
    base_delay_ms: float = 250.0
    """First exponential delay when the server sent no ``Retry-After``."""
    max_delay_ms: float = 4_000.0
    """Ceiling for exponential delays."""
    jitter: bool = False
    """Add up to 50% jitter to exponential delays."""
    max_retry_after_ms: float = 60_000.0
    """A ``Retry-After`` longer than this is surfaced, not slept through."""

    def __post_init__(self) -> None:
        if self.max_retries < 0:
            raise ValueError("max_retries must not be negative")
        if self.base_delay_ms < 0 or self.max_delay_ms < 0:
            raise ValueError("retry delays must not be negative")
        if self.max_retry_after_ms < 0:
            raise ValueError("max_retry_after_ms must not be negative")

    @classmethod
    def none(cls) -> "RetryPolicy":
        """A policy that never retries."""
        return cls(max_retries=0)

    def delay_seconds(
        self,
        attempt: int,
        retry_after_ms: Optional[float] = None,
        *,
        operation_id: str = "request",
    ) -> float:
        """The delay before retry number ``attempt`` (0-based).

        Args:
            attempt: Zero-based retry number.
            retry_after_ms: The server's ``Retry-After`` in milliseconds.
            operation_id: Operation the delay belongs to, for the error text.

        Returns:
            The delay in seconds.

        Raises:
            RetryAfterTooLongError: When the server asks for longer than
                :attr:`max_retry_after_ms`.
        """
        if retry_after_ms is not None:
            if retry_after_ms > self.max_retry_after_ms:
                raise RetryAfterTooLongError(
                    operation_id, retry_after_ms, self.max_retry_after_ms
                )
            return retry_after_ms / 1000.0
        exponent = min(max(attempt, 0), 31)
        delay = min(self.base_delay_ms * (2**exponent), self.max_delay_ms)
        if self.jitter:
            # Deterministic-free jitter without a random dependency: the clock's
            # sub-second component gives the entropy, and the factor stays in
            # 0.5..=1.0 so the delay never exceeds the cap.
            fraction = (time.time_ns() // 1000) % 1000 / 2000.0
            delay = min(delay * (0.5 + fraction), self.max_delay_ms)
        return delay / 1000.0
