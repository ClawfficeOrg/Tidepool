"""Problem-details normalisation for non-2xx responses."""

from __future__ import annotations

import json
from typing import Any, Dict, Mapping, Optional

from .errors import ProblemError


def retry_after_ms(value: Optional[str]) -> Optional[float]:
    """Parse a ``Retry-After`` header into milliseconds.

    The delta-seconds form is honoured; an HTTP-date is treated as absent
    because the pool sends delta-seconds only. A nonsense value is ignored, the
    same rule the other SDKs apply.
    """
    if value is None:
        return None
    text = value.strip()
    if not text:
        return None
    try:
        seconds = float(text)
    except ValueError:
        return None
    if seconds < 0:
        return None
    return seconds * 1000.0


def problem_from_response(
    *,
    operation_id: str,
    status: int,
    headers: Mapping[str, str],
    body: bytes,
    attempts: int,
) -> ProblemError:
    """Build a :class:`ProblemError` from one failed HTTP response."""
    document: Any = None
    try:
        document = json.loads(body) if body else None
    except (ValueError, TypeError):
        document = None
    fields: Dict[str, Any] = document if isinstance(document, dict) else {}
    code = fields.get("code")
    if not isinstance(code, str) or not code:
        fallback = fields.get("error")
        code = fallback if isinstance(fallback, str) and fallback else f"http_{status}"
    detail = fields.get("detail")
    if not isinstance(detail, str):
        detail = fields.get("message") if isinstance(fields.get("message"), str) else None
    if detail is None:
        detail = _raw_detail(body)
    title = fields.get("title") if isinstance(fields.get("title"), str) else None
    problem_type = fields.get("type") if isinstance(fields.get("type"), str) else None
    request_id = fields.get("requestId")
    if not isinstance(request_id, str):
        request_id = fields.get("id") if isinstance(fields.get("id"), str) else None
    if request_id is None:
        request_id = headers.get("x-request-id")
    return ProblemError(
        operation_id=operation_id,
        status=status,
        code=code,
        detail=detail,
        title=title,
        problem_type=problem_type,
        request_id=request_id,
        attempts=attempts,
        retry_after_ms=retry_after_ms(headers.get("retry-after")),
        body=document,
    )


def _raw_detail(body: bytes) -> Optional[str]:
    """A short, single-line description of a body that is not a problem document."""
    if not body:
        return None
    text = body.decode("utf-8", errors="replace")
    compact = " ".join(text.split())
    if not compact:
        return None
    return compact[:512]
