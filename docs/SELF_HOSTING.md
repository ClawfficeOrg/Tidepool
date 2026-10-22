# Self-hosting Tidepool

Tidepool ships one compose stack: MySQL, Redis and the pool. The pool applies
its embedded migrations at startup, so a clean machine needs no manual SQL.

## Quickstart

```sh
cp .env.example .env
# edit .env: set JWT_SECRET (openssl rand -hex 32), passwords, TIDEPOOL_PUBLIC_ORIGIN
docker compose up -d
docker compose logs -f pool
```

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

## Checks

```sh
docker compose config          # validate the stack file and env
curl -fsS http://127.0.0.1:8080/healthz
docker compose restart         # data must survive
```
