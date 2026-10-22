# Self-hosting Tidepool

Tidepool ships one compose stack: MySQL, Redis and the pool. The pool applies
its embedded migrations at startup, so a clean machine needs no manual SQL.

## Quickstart

```sh
make env                      # writes .env with fresh secrets, mode 600
# edit .env: set TIDEPOOL_PUBLIC_ORIGIN (and TIDEBOOT_* for an unattended install)
make up                       # PROXY=none by default for local use
make logs
```

The `Makefile` wraps the same operations the rest of this document describes:

| command | what it does |
|---|---|
| `make env` | `scripts/gen-env.sh` (never overwrites `.env`) |
| `make config` | validates the compose layers for the chosen proxy |
| `make up` / `make down` / `make logs` / `make health` | day-to-day stack control |
| `make deploy` | build, start and verify (first deploy) |
| `make redeploy` | pull, back up, rebuild, verify, roll back on failure |
| `make rollback TAG=v0.5.0` | return to a previous release tag |
| `make backup` | database dump only |
| `make verify` | health check only |

`PROXY` selects the compose layers: `traefik` (default), `caddy`, `nginx` or
`none`, e.g. `make deploy PROXY=caddy HOSTNAME=pool.example.com`. `HOSTNAME`
must match `TIDEPOOL_PUBLIC_ORIGIN`.

`gen-env.sh` copies `.env.example` and fills in `JWT_SECRET` (128 hex chars),
`TIDEPOOL_DB_PASSWORD`, `TIDEPOOL_DB_ROOT_PASSWORD` and a matching
`DATABASE_URL`. It refuses to run when `.env` already exists, so an existing
deployment is never clobbered; move the old file aside first. It never prints a
secret.

Then either open `https://<your origin>/` and use the browser setup, or set the
`TIDEBOOT_*` variables (below) and let the first boot create the operator.

`docker compose down` stops the stack and keeps the data; only
`docker compose down -v` deletes the volumes.

## One port, one origin

The stack publishes exactly one port (`TIDEPOOL_PORT`, default `8080`).
MySQL and Redis are reachable only inside the compose network.

`TIDEPOOL_PUBLIC_ORIGIN` is the single setting that changes when the pool moves
or gains a hostname. It is an origin only — scheme, host and optional port, no
trailing path:

```
TIDEPOOL_PUBLIC_ORIGIN=https://pool.example.com
```

Consequences when it changes:

- **Webhook URLs handed to providers** are built from it. Every GitHub, Slack
  and Discord app that POSTs to `/v1/webhooks/<provider>/<slug>` must be updated
  to the new origin, or deliveries will keep going to the old host.
- **Links in relayed or activity output** that point back at the pool use it.
- **The reverse proxy's `Host`** must match it, because the pool trusts the
  origin it was configured with rather than the inbound `Host` header.
- **Cookies and sign-in flows** are scoped to the browser origin, so a scheme
  change (`http` → `https`) requires signing in again.

Nothing else needs editing: the database URL and Redis URL are relative to the
compose network and unchanged by a public address move.

## Unattended install (no browser)

Set all four `TIDEBOOT_*` variables before the first start:

```
TIDEBOOT_ORGANIZATION_NAME=Rock Pool
TIDEBOOT_OWNER_EMAIL=owner@example.com
TIDEBOOT_OWNER_PASSWORD=at-least-12-characters
TIDEBOOT_OWNER_NAME=Owner
```

The first boot creates the organization and owner, and marks the password for
change on first sign-in. The bootstrap is insert-only, so restarting never
re-applies the temporary password. Without `TIDEBOOT_*`, the pool reports
`needsSetup` and the browser setup path is used instead.

## Reverse proxy

Terminate TLS at the proxy and forward everything to the published port. The
pool speaks plain HTTP inside the stack.

Caddy:

```
pool.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

nginx:

```nginx
server {
    listen 443 ssl;
    server_name pool.example.com;
    client_max_body_size 2m;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # The live streams are WebSockets.
    location /v1/chatrooms/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }

    location /ws {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

Rate limiting reads the socket peer by default. If you want per-client buckets
behind a proxy that overwrites the header, set `RATE_LIMIT_TRUST_XFF=true`.

## Backup and restore

MySQL is the single store; there is no embedded-database mode. A backup is a
logical dump, taken with `--single-transaction` so it does not lock writes.

```sh
# Back up. Keep the file off the host if the machine itself is what fails.
docker compose exec -T db \
  mysqldump -uroot -p"$TIDEPOOL_DB_ROOT_PASSWORD" \
  --single-transaction --routines --triggers tidepool \
  > "tidepool-$(date +%F-%H%M).sql"

# Restore into a running stack (this replaces the database contents).
docker compose stop pool
docker compose exec -T db \
  mysql -uroot -p"$TIDEPOOL_DB_ROOT_PASSWORD" tidepool < tidepool-2026-10-22-1200.sql
docker compose start pool
```

The pool applies any migrations the restored dump is missing on the next start,
because migrations are forward-only and embedded; a dump from an older release
is upgraded in place and never downgraded.

Verified against the dev stack: dump, restore into an empty database, and the
restored copy contained every table the live database had.

What a restore costs — everything written after the dump is gone:

- operator accounts and invitations created since;
- connection credentials and connection resources;
- projects, configuration revisions, triggers and their revisions;
- runs, steps, activity receipts and agent conversations;
- chatrooms, members and messages;
- relay opt-ins and relayed events;
- audit events.

Restore into a copy first if you need to compare. Files, agent output and
anything the relay was never allowed to carry are not in the database and are
not covered by this procedure.

## Data and restarts

| what | where |
|---|---|
| MySQL data | named volume `pool_db_data` |
| Redis data | named volume `pool_redis_data` |
| pool binary | image, rebuilt by `docker compose build` |

Migrations run on every boot and are idempotent, so an upgrade is
`docker compose pull && docker compose up -d --build`. Backup and restore are
covered by the migrations/backup task; until then, back up the MySQL volume by
dumping the database rather than copying the volume files.

## VPS deployment with TLS

The VPS runs the pool behind a TLS-terminating proxy. The pool itself is bound
to loopback and never publishes a public port.

| what | value |
|---|---|
| checkout | `/opt/clawffice/tidepool` (`TIDEPOOL_DIR`) |
| compose files | `docker-compose.yml` + `deploy/docker-compose.vps.yml` |
| reverse proxy | Traefik, routing declared in `deploy/docker-compose.vps.yml` (docker provider) or `deploy/traefik.yml` (file provider) |
| hostname | `TIDEPOOL_HOSTNAME` in the VPS `.env`; the same value goes in `TIDEPOOL_PUBLIC_ORIGIN` |
| certificate | Traefik ACME (`TRAEFIK_CERTRESOLVER`, usually `le`), renewed automatically |
| backups | `/var/backups/tidepool` (`TIDEPOOL_BACKUP_DIR`) |

The VPS-owned `.env` sits beside the compose files, is never committed, and
holds `JWT_SECRET`, database passwords, `TIDEPOOL_PUBLIC_ORIGIN` and (only for
the first boot) `TIDEBOOT_*`.

### First deploy

```sh
# on the VPS, once the DNS record for the hostname points at it
sudo mkdir -p /opt/clawffice/tidepool /var/backups/tidepool
cd /opt/clawffice/tidepool
# place the VPS .env here (0600, owned by the deploy user)
docker network create proxy 2>/dev/null || true
./deploy/deploy.sh deploy
```

**Traefik note.** The routing below is declared as docker-provider labels, so
Traefik must be configured with `--providers.docker` (and usually
`exposedByDefault=false`, which the `traefik.enable=true` label reverses). Three
things are easy to get wrong:

- the pool must be attached to the *same* network Traefik watches; the
  `traefik.docker.network` label picks the right one when a container is on
  several. `TIDEPOOL_PROXY_NETWORK` names it.
- `TRAEFIK_ENTRYPOINT` and `TRAEFIK_CERTRESOLVER` must match the names in
  Traefik's static configuration; `websecure` and `le` are only the common
  defaults.
- if the VPS Traefik uses the **file** provider instead of docker, the labels
  are ignored: install `deploy/traefik.yml` into its dynamic directory instead
  (that file is the same router, middlewares and service in YAML).

Traefik must already be running on the VPS with an HTTPS entrypoint
(`TRAEFIK_ENTRYPOINT`, usually `websecure`), an HTTP entrypoint for the
redirect, and an ACME certificate resolver (`TRAEFIK_CERTRESOLVER`). The pool
joins the network Traefik watches (`TIDEPOOL_PROXY_NETWORK`, usually `proxy`).

Deploy with the proxy that is actually installed:

```sh
make deploy PROXY=traefik HOSTNAME=pool.example.com
```

Then, over HTTPS only:

1. Sign in with the `TIDEBOOT_OWNER_EMAIL` / `TIDEBOOT_OWNER_PASSWORD`.
2. Change the password when prompted.
3. **Remove the `TIDEBOOT_*` block from the VPS `.env`** and restart:
   `${COMPOSE} up -d`. The bootstrap is insert-only so it will not re-run, and
   removing it means the temporary credential no longer exists anywhere.
4. Enrol one host and dispatch one run as the smoke test.

### Certificate renewal

Traefik renews through its ACME resolver (typically ~30 days before expiry)
and hot-reloads the certificate; there is no separate renewal job. Check with:

```sh
curl -sSI https://<hostname> | head -1        # expect 200/302, never a cert error
docker logs <traefik-container> 2>&1 | grep -i "certificate" | tail
```

If renewal fails the usual causes are a stale DNS record or port 80 closed for
the HTTP-01 challenge (or blocked egress for DNS-01). Traefik keeps serving the
previous certificate until it expires, so alert on expiry, do not discover it.

### Upgrade

```sh
cd /opt/clawffice/tidepool
./deploy/deploy.sh deploy     # pull, backup, build, restart, verify
```

The script refuses to continue if the backup is empty, applies migrations on
start (a failed migration refuses to start), and rolls back to the previous
revision automatically when the health check does not come back.

### Rollback

```sh
./deploy/deploy.sh rollback v0.5.0
```

That checks out the tag, rebuilds, restarts and verifies. Data is **not**
rolled back: migrations are forward-only, so an older binary may not read a
schema written by a newer one. Prefer rolling forward with a fix; treat a
rollback across a migration boundary as restore-from-backup, using the dump the
deploy step took.

## Checks

```sh
docker compose config          # validate the stack file and env
curl -fsS http://127.0.0.1:8080/healthz
docker compose restart         # data must survive
```
