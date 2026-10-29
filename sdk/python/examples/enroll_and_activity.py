#!/usr/bin/env python3
"""Enrol a host with a Tidepool pool, then read the activity feed.

This is the shortest end-to-end example: create the host relationship, mint a
one-time enrolment token, finish the enrolment with the host-generated
credential, and read the first page of activity.

Run it against a pool you own::

    python sdk/python/examples/enroll_and_activity.py \
        --base-url http://localhost:8080 \
        --token "$TIDEPOOL_TOKEN" \
        --hostname build-box.local

The token is read from ``--token`` or ``TIDEPOOL_TOKEN``; it is never printed.
"""

from __future__ import annotations

import argparse
import os
import sys
from dataclasses import dataclass
from typing import Optional

from tidepool_client import SyncClient
from tidepool_client.models import ActivityPage, HostView


@dataclass
class Enrolment:
    """The result of the example."""

    host: HostView
    """The host created before enrolment."""
    enrolled: HostView
    """The host as the pool returned it after enrolment."""
    activity: ActivityPage
    """The first page of the activity feed."""


def run(
    base_url: str,
    token: str,
    *,
    hostname: str = "build-box.local",
    display_name: str = "Build box",
    credential: str = "host-generated-secret",
) -> Enrolment:
    """Create a host, enrol it, and read one page of activity.

    Args:
        base_url: Absolute pool URL.
        token: Operator JWT.
        hostname: Hostname the host reports.
        display_name: Human-readable host name.
        credential: The host-generated relationship secret.

    Returns:
        The created host, the enrolled host and the first activity page.
    """
    client = SyncClient(base_url, token=token)

    host = client.hosts.create(
        body={"display_name": display_name, "hostname": hostname}
    )

    minted = client.hosts.mint_enrollment_token(body={"host_id": host.id})
    enrolled = client.hosts.enroll(
        body={"token": minted.token, "credential": credential}
    )

    activity = client.activity.list(limit=10)
    return Enrolment(host=host, enrolled=enrolled, activity=activity)


def main(argv: Optional[list] = None) -> int:
    """Command-line entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=os.environ.get("TIDEPOOL_BASE_URL"))
    parser.add_argument("--token", default=os.environ.get("TIDEPOOL_TOKEN"))
    parser.add_argument("--hostname", default="build-box.local")
    parser.add_argument("--display-name", default="Build box")
    arguments = parser.parse_args(argv)
    if not arguments.base_url or not arguments.token:
        parser.error("--base-url and --token (or TIDEPOOL_BASE_URL / TIDEPOOL_TOKEN) are required")
    result = run(
        arguments.base_url,
        arguments.token,
        hostname=arguments.hostname,
        display_name=arguments.display_name,
    )
    print(f"enrolled {result.enrolled.slug} ({result.enrolled.status})")
    print(f"activity entries on page one: {len(result.activity.items)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
