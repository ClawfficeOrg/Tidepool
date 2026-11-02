"""Opt-in checks against a running dev stack.

These are skipped unless ``TIDEPOOL_BASE_URL`` points at a pool the caller owns;
they never start a server, and they never print a credential.
"""

from __future__ import annotations

import asyncio
import os

import pytest

from tidepool_client import AsyncClient, SyncClient
from tidepool_client.models import HostsResponse

BASE_URL = os.environ.get("TIDEPOOL_BASE_URL")
TOKEN = os.environ.get("TIDEPOOL_TOKEN")

pytestmark = pytest.mark.skipif(
    not BASE_URL,
    reason="set TIDEPOOL_BASE_URL (and optionally TIDEPOOL_TOKEN) to run the live checks",
)


def test_live_sync_client_reads_hosts_and_health():
    client = SyncClient(BASE_URL, token=TOKEN)
    hosts = client.hosts.list()
    assert isinstance(hosts, HostsResponse)
    assert client.system.health() is not None


def test_live_async_client_reads_hosts():
    async def scenario():
        client = AsyncClient(BASE_URL, token=TOKEN)
        return await client.hosts.list()

    hosts = asyncio.run(scenario())
    assert isinstance(hosts, HostsResponse)
