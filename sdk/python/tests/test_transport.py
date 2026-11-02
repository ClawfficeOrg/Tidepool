"""Transport details: chunked, close-delimited and malformed responses."""

from __future__ import annotations

import asyncio
import socket
import threading
import time
from typing import Optional

import pytest

from tidepool_client import AsyncClient, SyncClient, TransportError

RAW_HEALTH = b'{"ok": true}'


class RawServer:
    """A one-exchange-at-a-time TCP server that writes a fixed response."""

    def __init__(self, response: Optional[bytes]) -> None:
        self.response = response
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._socket.bind(("127.0.0.1", 0))
        self._socket.listen(8)
        self._socket.settimeout(0.2)
        self._running = True
        self._thread = threading.Thread(target=self._serve, daemon=True)

    @property
    def base_url(self) -> str:
        host, port = self._socket.getsockname()[:2]
        return f"http://{host}:{port}"

    def __enter__(self) -> "RawServer":
        self._thread.start()
        return self

    def __exit__(self, *exc_info) -> None:
        self._running = False
        self._thread.join(timeout=5)
        self._socket.close()

    def _serve(self) -> None:
        while self._running:
            try:
                connection, _ = self._socket.accept()
            except (OSError, socket.timeout):
                continue
            try:
                self._consume(connection)
                if self.response is None:
                    time.sleep(0.3)
                    continue
                connection.sendall(self.response)
            finally:
                connection.close()

    def _consume(self, connection: socket.socket) -> None:
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = connection.recv(4096)
            if not chunk:
                return
            data += chunk
        length = 0
        for line in data.split(b"\r\n"):
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":", 1)[1].strip())
        _, _, body = data.partition(b"\r\n\r\n")
        while len(body) < length:
            body += connection.recv(4096)


def test_async_transport_decodes_a_chunked_response():
    response = (
        b"HTTP/1.1 200 OK\r\n"
        b"Content-Type: application/json\r\n"
        b"Transfer-Encoding: chunked\r\n\r\n"
        b"c\r\n" + RAW_HEALTH + b"\r\n0\r\n\r\n"
    )
    with RawServer(response) as server:
        client = AsyncClient(server.base_url)
        assert asyncio.run(client.system.health()) == {"ok": True}


def test_sync_transport_reads_until_close_without_a_length():
    response = (
        b"HTTP/1.1 200 OK\r\n"
        b"Content-Type: application/json\r\n"
        b"Connection: close\r\n\r\n" + RAW_HEALTH
    )
    with RawServer(response) as server:
        client = SyncClient(server.base_url)
        assert client.system.health() == {"ok": True}


def test_malformed_status_lines_become_transport_errors():
    response = b"garbage not http\r\n\r\n"
    with RawServer(response) as server:
        with pytest.raises(TransportError):
            SyncClient(server.base_url).system.health()
    with RawServer(response) as server:
        with pytest.raises(TransportError):
            asyncio.run(AsyncClient(server.base_url).system.health())


def test_a_timeout_becomes_a_transport_error():
    with RawServer(None) as server:
        with pytest.raises(TransportError):
            SyncClient(server.base_url, timeout=0.05).system.health()
    with RawServer(None) as server:
        with pytest.raises(TransportError):
            asyncio.run(
                AsyncClient(server.base_url, timeout=0.05).system.health()
            )
