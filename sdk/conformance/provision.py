#!/usr/bin/env python3
"""Provision a real pool for the cross-SDK conformance suite.

The suite itself only reads: it needs an operator, a project, two keys, two
hosts, a connection and three triggers already in place. This script creates
them over the public API, then writes the variable file
``run_conformance.py --vars`` expects. It talks to a real server backed by a
real database; there is no mock mode on purpose, because a mock would not catch
a binding that disagrees with the pool.

Usage::

    python3 sdk/conformance/provision.py \\
        --base-url http://127.0.0.1:8090 \\
        --out /tmp/tidepool-conformance-vars.json \\
        --agent-bin target/debug/tidepool-host-agent

The agent binary is optional: without it the "online" host is left offline and
the scenarios that need a connected daemon (``dispatch.idempotent``,
``paging.relay``) fail. The pid of the started agent is written to
``<out>.agent.pid`` so the calling job can stop it.

Nothing secret is printed: credentials go into the variable file, which the
caller should treat as a secret.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

CONFORMANCE_DIR = Path(__file__).resolve().parent
REPOSITORY = CONFORMANCE_DIR.parents[1]

#: Absolute working directory the configuration revision declares. The
#: reference agent does not use it; the pool only validates that it is absolute.
WORK_DIRECTORY = "/tmp/tidepool-conformance"

#: Agent selection every conformance workflow uses. The reference agent reports
#: exactly this variant, and the offline host reports it here.
AGENT = {
    "provider": "anthropic",
    "model": "claude",
    "execution_mode": "default",
}


class ProvisionError(Exception):
    """The pool refused a provisioning step, or never became ready."""


class Pool:
    """A minimal hub client for provisioning over HTTP."""

    def __init__(self, base_url: str, timeout: float = 30.0) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    def call(
        self,
        method: str,
        path: str,
        body: Any = None,
        token: Optional[str] = None,
        api_key: Optional[str] = None,
        expect: Tuple[int, ...] = (200, 201, 204),
    ) -> Tuple[int, Any]:
        """Send one request and return ``(status, decoded body)``."""
        url = self.base_url + path
        data = None
        headers = {"accept": "application/json"}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["content-type"] = "application/json"
        if token:
            headers["authorization"] = f"Bearer {token}"
        if api_key:
            headers["x-api-key"] = api_key
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                status = response.status
                payload = response.read()
        except urllib.error.HTTPError as error:
            status = error.code
            payload = error.read()
        except urllib.error.URLError as error:
            raise ProvisionError(
                f"{method} {path}: cannot reach {self.base_url}: {error.reason}"
            ) from error
        decoded: Any = None
        if payload:
            try:
                decoded = json.loads(payload)
            except ValueError:
                decoded = payload.decode("utf-8", "replace")
        if status not in expect:
            detail = ""
            if isinstance(decoded, dict):
                detail = str(decoded.get("code") or decoded.get("detail") or "")
            raise ProvisionError(f"{method} {path}: {status} {detail}".strip())
        return status, decoded


def login(pool: Pool, args: argparse.Namespace) -> str:
    """Create the first operator, or sign in to an existing pool."""
    _, setup = pool.call("GET", "/v1/setup/status", expect=(200,))
    if isinstance(setup, dict) and setup.get("needs_setup"):
        pool.call(
            "POST",
            "/v1/setup",
            {
                "organization_name": args.organization,
                "email": args.email,
                "display_name": args.display_name,
                "password": args.password,
            },
        )
    elif not args.existing:
        raise ProvisionError(
            "the pool is already set up; pass --existing with "
            "--email/--password (env TIDEPOOL_CONFORMANCE_EMAIL / "
            "TIDEPOOL_CONFORMANCE_PASSWORD also work) to reuse it"
        )
    _, body = pool.call(
        "POST", "/v1/pool/login", {"email": args.email, "password": args.password}
    )
    token = body.get("token") if isinstance(body, dict) else None
    if not isinstance(token, str) or not token:
        raise ProvisionError("POST /v1/pool/login returned no token")
    return token


def create_key(pool: Pool, token: str, name: str, scopes: List[str]) -> str:
    """Mint one API key and return its secret."""
    _, body = pool.call(
        "POST", "/v1/api-keys", {"name": name, "scopes": scopes}, token=token
    )
    secret = body.get("secret") if isinstance(body, dict) else None
    if not isinstance(secret, str) or not secret:
        raise ProvisionError(f"POST /v1/api-keys ({name}) returned no secret")
    return secret


def register_host(pool: Pool, token: str, label: str) -> Dict[str, str]:
    """Register a host and return its id and slug."""
    _, body = pool.call(
        "POST",
        "/v1/hosts",
        {"hostname": label, "display_name": f"Conformance {label}"},
        token=token,
    )
    if not isinstance(body, dict) or not body.get("id") or not body.get("slug"):
        raise ProvisionError(f"POST /v1/hosts ({label}) returned no id/slug")
    return {"id": body["id"], "slug": body["slug"]}


def enroll_host(pool: Pool, token: str, host_id: str, credential: str) -> None:
    """Mint a one-time token for a host and exchange it for a credential."""
    _, body = pool.call(
        "POST",
        "/v1/hosts/enrollment-tokens",
        {"host_id": host_id},
        token=token,
    )
    enrollment = body.get("token") if isinstance(body, dict) else None
    if not isinstance(enrollment, str) or not enrollment:
        raise ProvisionError("POST /v1/hosts/enrollment-tokens returned no token")
    pool.call(
        "POST",
        "/v1/hosts/enroll",
        {"token": enrollment, "credential": credential},
    )


def grant_execution(pool: Pool, token: str, host_id: str) -> None:
    """Grant host.execute so the host may report capabilities."""
    pool.call("POST", f"/v1/hosts/{host_id}/execution", token=token)


def report_capabilities(pool: Pool, host_id: str, credential: str, variants: List[Dict[str, Any]]) -> None:
    """Report capabilities as the host itself would."""
    request = urllib.request.Request(
        pool.base_url + f"/v1/hosts/{host_id}/capabilities",
        data=json.dumps({"capabilities": variants}).encode("utf-8"),
        headers={
            "content-type": "application/json",
            "x-host-id": host_id,
            "x-host-credential": credential,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=pool.timeout):
            return
    except urllib.error.HTTPError as error:
        raise ProvisionError(
            f"POST /v1/hosts/{host_id}/capabilities: {error.code}"
        ) from error
    except urllib.error.URLError as error:
        raise ProvisionError(
            f"POST /v1/hosts/{host_id}/capabilities: {error.reason}"
        ) from error


def install_configuration(
    pool: Pool,
    token: str,
    project_id: str,
    connection_slug: str,
    offline_slug: str,
    online_slug: str,
    triggers: Dict[str, str],
) -> None:
    """Install one revision whose workflows start each conformance trigger."""
    environments = [
        {
            "name": "offline",
            "host": offline_slug,
            "directory": WORK_DIRECTORY,
        },
        {
            "name": "online",
            "host": online_slug,
            "directory": WORK_DIRECTORY,
        },
    ]
    workflows = [
        {
            "name": name,
            "trigger": trigger,
            "maxRuntime": 900,
            "steps": [
                {
                    "id": "run",
                    "environment": environment,
                    "agent": dict(AGENT),
                    "prompt": [{"text": "conformance"}],
                    "maxRuntime": 600,
                    "idleTimeout": 120,
                }
            ],
        }
        for (name, trigger), environment in zip(
            triggers.items(), ("offline", "online", "online")
        )
    ]
    manifest = {
        "connections": [connection_slug],
        "hosts": [offline_slug, online_slug],
        "environments": environments,
        "workflows": workflows,
    }
    _, body = pool.call(
        "POST",
        f"/v1/projects/{project_id}/configuration/install",
        {"files": [{"path": "tidepool.json", "content": json.dumps(manifest)}]},
        token=token,
    )
    revision = body.get("revision") if isinstance(body, dict) else None
    activated = body.get("activated") if isinstance(body, dict) else None
    if activated is not True or not isinstance(revision, dict) or revision.get("status") != "valid":
        errors = revision.get("errors") if isinstance(revision, dict) else None
        raise ProvisionError(
            "configuration revision was not activated: "
            + json.dumps(errors or [], sort_keys=True)
        )


def install_trigger(
    pool: Pool,
    token: str,
    project_id: str,
    name: str,
    connection_slug: str,
    host_slug: str,
    from_users: List[str],
) -> None:
    """Install one trigger document bound to a host and connection."""
    document = {
        "project": project_id,
        "name": name,
        "event": {"provider": "github", "type": "issue.comment_created"},
        "filters": {
            "connection": connection_slug,
            "resource": {"kind": "repository", "id": "acme/conformance"},
            "from_users": from_users,
        },
        "run": {"host": host_slug, "agent": dict(AGENT)},
    }
    pool.call("POST", "/v1/triggers/install", document, token=token)


def start_agent(pool: Pool, args: argparse.Namespace, host: Dict[str, str], credential: str) -> subprocess.Popen:
    """Start the reference host agent and return the process."""
    binary = Path(args.agent_bin)
    if not binary.is_absolute():
        binary = REPOSITORY / binary
    if not binary.exists():
        raise ProvisionError(
            f"host agent not found at {binary}; build it with: "
            "cargo build -p tidepool-host-agent"
        )
    base_url = pool.base_url
    environment = dict(os.environ)
    environment.update(
        {
            "POOL_HTTP_URL": base_url,
            "POOL_WS_URL": base_url.replace("http://", "ws://", 1).replace(
                "https://", "wss://", 1
            )
            + "/ws/host",
            "HOST_ID": host["id"],
            "CREDENTIAL": credential,
            "CAPABILITIES": json.dumps(
                [
                    {"provider": "local", "model": "echo", "execution_mode": "default"},
                    AGENT,
                ]
            ),
        }
    )
    process = subprocess.Popen(
        [str(binary)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=environment,
    )
    return process


def wait_for_connected(pool: Pool, token: str, host_id: str, timeout: float = 30.0) -> None:
    """Poll until the pool reports the host as connected."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        _, body = pool.call("GET", f"/v1/hosts/{host_id}", token=token)
        if isinstance(body, dict) and body.get("status") == "connected":
            return
        time.sleep(0.5)
    raise ProvisionError(
        f"host {host_id} never reported connected; is the reference agent running?"
    )


def provision(args: argparse.Namespace) -> Dict[str, Any]:
    """Run every step and return the variable document."""
    pool = Pool(args.base_url)
    token = login(pool, args)
    run_id = args.run_id or uuid.uuid4().hex[:8]

    _, project = pool.call(
        "POST",
        "/v1/projects",
        {"name": f"Conformance {run_id}", "slug": f"conformance-{run_id}"},
        token=token,
    )
    project_id = project.get("id")
    if not isinstance(project_id, str) or not project_id:
        raise ProvisionError("POST /v1/projects returned no id")

    dispatch_key = create_key(
        pool,
        token,
        f"conformance-dispatch-{run_id}",
        [
            "projects:read",
            "configuration:validate",
            "configuration:install",
            "runs:dispatch",
            "daemons:enroll",
        ],
    )
    read_key = create_key(
        pool, token, f"conformance-read-{run_id}", ["projects:read"]
    )

    connection_slug = f"conformance-github-{run_id}"
    pool.call(
        "POST",
        "/v1/connections",
        {
            "provider": "github",
            "slug": connection_slug,
            "display_name": f"Conformance GitHub {run_id}",
            "credential_ref": "env:TIDEPOOL_CONFORMANCE_GITHUB_TOKEN",
            "resources": [
                {
                    "resource_kind": "repository",
                    "external_id": "acme/conformance",
                    "display_name": "acme/conformance",
                }
            ],
        },
        token=token,
    )

    offline = register_host(pool, token, f"conformance-offline-{run_id}")
    offline_credential = f"conformance-offline-credential-{run_id}-{'x' * 16}"
    enroll_host(pool, token, offline["id"], offline_credential)
    grant_execution(pool, token, offline["id"])
    # Validate-time agent selection reads the report, so the offline host must
    # have one before a trigger can name it.
    report_capabilities(pool, offline["id"], offline_credential, [AGENT])

    online = register_host(pool, token, f"conformance-online-{run_id}")
    online_credential = f"conformance-online-credential-{run_id}-{'x' * 16}"
    enroll_host(pool, token, online["id"], online_credential)
    grant_execution(pool, token, online["id"])

    actor = args.actor or f"conformance-{run_id}@tidepool.invalid"
    triggers = {
        "conformance-offline": f"conformance-offline-{run_id}",
        "conformance-online": f"conformance-online-{run_id}",
        "conformance-forbidden": f"conformance-forbidden-{run_id}",
    }

    agent: Optional[subprocess.Popen] = None
    try:
        if args.agent_bin:
            agent = start_agent(pool, args, online, online_credential)
            wait_for_connected(pool, token, online["id"])
        install_configuration(
            pool,
            token,
            project_id,
            connection_slug,
            offline["slug"],
            online["slug"],
            triggers,
        )
        install_trigger(
            pool,
            token,
            project_id,
            triggers["conformance-offline"],
            connection_slug,
            offline["slug"],
            [actor],
        )
        install_trigger(
            pool,
            token,
            project_id,
            triggers["conformance-online"],
            connection_slug,
            online["slug"],
            [actor],
        )
        install_trigger(
            pool,
            token,
            project_id,
            triggers["conformance-forbidden"],
            connection_slug,
            online["slug"],
            [f"not-{actor}"],
        )
    except Exception:
        if agent is not None:
            agent.terminate()
        raise

    variables = {
        "token": token,
        "operator_email": args.email,
        "actor": actor,
        "project_id": project_id,
        "api_key": dispatch_key,
        "read_key": read_key,
        "connection_slug": connection_slug,
        "offline_host_id": offline["id"],
        "offline_host_slug": offline["slug"],
        "offline_host_credential": offline_credential,
        "online_host_id": online["id"],
        "online_host_slug": online["slug"],
        "online_host_credential": online_credential,
        "trigger_offline": triggers["conformance-offline"],
        "trigger_online": triggers["conformance-online"],
        "trigger_forbidden": triggers["conformance-forbidden"],
    }

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(variables, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    out.chmod(0o600)
    if agent is not None:
        pid_file = Path(str(out) + ".agent.pid")
        pid_file.write_text(f"{agent.pid}\n", encoding="utf-8")
    return variables


def _parse_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="provision.py",
        description="Create the pool state the conformance suite needs.",
    )
    parser.add_argument("--base-url", required=True, help="absolute pool URL")
    parser.add_argument("--out", required=True, type=Path, help="variable file to write")
    parser.add_argument(
        "--agent-bin",
        default="target/debug/tidepool-host-agent",
        help="reference host agent binary (empty disables the online host)",
    )
    parser.add_argument(
        "--existing",
        action="store_true",
        help="the pool is already set up; sign in instead of creating it",
    )
    parser.add_argument(
        "--organization",
        default="Conformance Pool",
        help="organization name used when setting the pool up",
    )
    parser.add_argument(
        "--email",
        default=os.environ.get("TIDEPOOL_CONFORMANCE_EMAIL", "conformance@tidepool.invalid"),
        help="operator email",
    )
    parser.add_argument(
        "--display-name",
        default="Conformance Operator",
        help="operator display name used when setting the pool up",
    )
    parser.add_argument(
        "--password",
        default=os.environ.get("TIDEPOOL_CONFORMANCE_PASSWORD", "conformance-operator-password"),
        help="operator password (env TIDEPOOL_CONFORMANCE_PASSWORD wins at call time)",
    )
    parser.add_argument("--actor", default="", help="actor string the triggers allow")
    parser.add_argument("--run-id", default="", help="suffix for created names")
    return parser.parse_args(argv)


def main(argv: List[str]) -> int:
    """Provision the pool and report where the variables went."""
    args = _parse_args(argv)
    try:
        provision(args)
    except ProvisionError as error:
        print(f"provision: {error}", file=sys.stderr)
        return 1
    print(f"provision: wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
