"""Transports for the synchronous and asynchronous clients.

Both facades build the same :class:`PreparedRequest`; only the transport that
puts it on the wire differs. The sync transport uses :mod:`http.client`, the
async transport uses :mod:`asyncio` streams and its own HTTP/1.1 reader. Neither
adds a third-party dependency, so the package installs and works with no extras.
"""

from __future__ import annotations

import asyncio
import http.client
import ssl
from dataclasses import dataclass
from typing import Dict, Mapping, Optional, Tuple
from urllib.parse import urlsplit

from .errors import TransportError

HeaderPairs = Tuple[Tuple[str, str], ...]


@dataclass(frozen=True)
class PreparedRequest:
    """A request ready for one transport."""

    method: str
    url: str
    headers: HeaderPairs
    body: Optional[bytes]


@dataclass(frozen=True)
class RawResponse:
    """One HTTP response, before problem or JSON handling."""

    status: int
    headers: Mapping[str, str]
    body: bytes


class SyncTransport:
    """Blocking HTTP transport over :mod:`http.client`."""

    def send(self, request: PreparedRequest, timeout: float) -> RawResponse:
        """Send one request and read its complete response.

        Args:
            request: The prepared request.
            timeout: Timeout in seconds for the connection and the read.

        Returns:
            The status, lower-cased headers and body.

        Raises:
            TransportError: When no complete HTTP response could be read.
        """
        parts = urlsplit(request.url)
        target = _target(parts)
        connection: http.client.HTTPConnection
        try:
            if parts.scheme == "https":
                connection = http.client.HTTPSConnection(
                    parts.hostname,
                    parts.port,
                    timeout=timeout,
                    context=ssl.create_default_context(),
                )
            else:
                connection = http.client.HTTPConnection(
                    parts.hostname, parts.port, timeout=timeout
                )
            try:
                connection.request(
                    request.method,
                    target,
                    body=request.body,
                    headers=dict(request.headers),
                )
                response = connection.getresponse()
                body = response.read()
                headers = {name.lower(): value for name, value in response.getheaders()}
                return RawResponse(status=response.status, headers=headers, body=body)
            finally:
                connection.close()
        except (OSError, http.client.HTTPException, ssl.SSLError, ValueError) as error:
            raise TransportError(f"HTTP transport failed: {error}") from error


class AsyncTransport:
    """Asynchronous HTTP/1.1 transport over :mod:`asyncio` streams."""

    async def send(self, request: PreparedRequest, timeout: float) -> RawResponse:
        """Send one request without blocking the event loop.

        Args:
            request: The prepared request.
            timeout: Timeout in seconds for the connection and the response.

        Returns:
            The status, lower-cased headers and body.

        Raises:
            TransportError: When no complete HTTP response could be read.
        """
        try:
            return await asyncio.wait_for(_send_async(request), timeout)
        except asyncio.TimeoutError as error:
            raise TransportError(
                f"HTTP transport timed out after {timeout:g}s"
            ) from error
        except (OSError, ssl.SSLError, ValueError, asyncio.IncompleteReadError) as error:
            raise TransportError(f"HTTP transport failed: {error}") from error


async def _send_async(request: PreparedRequest) -> RawResponse:
    """Open one connection, write the request and read the response."""
    parts = urlsplit(request.url)
    host = parts.hostname
    if host is None:
        raise TransportError("request URL has no host")
    port = parts.port or (443 if parts.scheme == "https" else 80)
    context = ssl.create_default_context() if parts.scheme == "https" else None
    reader, writer = await asyncio.open_connection(
        host, port, ssl=context, server_hostname=host if context else None
    )
    try:
        headers = _wire_headers(request, parts.netloc)
        payload = bytearray()
        payload.extend(f"{request.method} {_target(parts)} HTTP/1.1\r\n".encode("ascii"))
        for name, value in headers:
            payload.extend(f"{name}: {value}\r\n".encode("latin-1"))
        payload.extend(b"\r\n")
        if request.body:
            payload.extend(request.body)
        writer.write(bytes(payload))
        await writer.drain()
        return await _read_response(reader)
    finally:
        writer.close()
        try:
            await writer.wait_closed()
        except (OSError, ssl.SSLError):
            pass


def _wire_headers(request: PreparedRequest, netloc: str) -> HeaderPairs:
    """Add the headers HTTP/1.1 requires to a prepared request."""
    headers: Dict[str, str] = {name.lower(): value for name, value in request.headers}
    headers.setdefault("host", netloc)
    headers.setdefault("connection", "close")
    if request.body is not None:
        headers.setdefault("content-length", str(len(request.body)))
    return tuple(headers.items())


async def _read_response(reader: asyncio.StreamReader) -> RawResponse:
    """Read one status line, header block and body."""
    status_line = await reader.readline()
    if not status_line:
        raise TransportError("connection closed before a response was received")
    try:
        _, status_text, _ = status_line.decode("latin-1").split(" ", 2)
        status = int(status_text)
    except (ValueError, UnicodeDecodeError) as error:
        raise TransportError(f"malformed HTTP status line: {status_line!r}") from error
    headers: Dict[str, str] = {}
    while True:
        line = await reader.readline()
        if not line or line in (b"\r\n", b"\n"):
            break
        name, separator, value = line.decode("latin-1").partition(":")
        if not separator:
            continue
        headers[name.strip().lower()] = value.strip()
    encoding = headers.get("transfer-encoding", "").lower()
    if "chunked" in encoding:
        body = await _read_chunked(reader)
    elif "content-length" in headers:
        try:
            length = int(headers["content-length"])
        except ValueError as error:
            raise TransportError("malformed content-length header") from error
        body = await reader.readexactly(length)
    else:
        body = await reader.read()
    return RawResponse(status=status, headers=headers, body=body)


async def _read_chunked(reader: asyncio.StreamReader) -> bytes:
    """Read a chunked body."""
    chunks = bytearray()
    while True:
        line = await reader.readline()
        if not line:
            raise TransportError("connection closed inside a chunked body")
        size_text = line.split(b";", 1)[0].strip()
        try:
            size = int(size_text, 16)
        except ValueError as error:
            raise TransportError("malformed chunk size") from error
        if size == 0:
            while True:
                trailer = await reader.readline()
                if not trailer or trailer in (b"\r\n", b"\n"):
                    return bytes(chunks)
        chunks.extend(await reader.readexactly(size))
        await reader.readexactly(2)


def _target(parts) -> str:
    """The origin-form request target for a split URL."""
    target = parts.path or "/"
    if parts.query:
        target = f"{target}?{parts.query}"
    return target
