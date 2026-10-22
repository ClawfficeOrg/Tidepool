#!/usr/bin/env bash
# Deploy or roll back Tidepool on the VPS (task 1.6.4).
#
#   ./deploy/deploy.sh deploy          # pull, back up, build, restart, verify
#   ./deploy/deploy.sh rollback <tag>  # check out a previous release tag and restart
#   ./deploy/deploy.sh verify          # health check only
#
# Run it on the VPS from the pool checkout. Secrets come from the VPS-owned
# .env beside the compose files; this script never reads or prints them.
set -euo pipefail

APP_DIR="${TIDEPOOL_DIR:-/opt/clawffice/tidepool}"
COMPOSE=(docker compose -f docker-compose.yml -f deploy/docker-compose.vps.yml)
BACKUP_DIR="${TIDEPOOL_BACKUP_DIR:-/var/backups/tidepool}"
HEALTH_URL="${TIDEPOOL_HEALTH_URL:-http://127.0.0.1:8080/healthz}"

log() { printf '[deploy] %s\n' "$*"; }

backup() {
  mkdir -p "$BACKUP_DIR"
  local stamp dump
  stamp="$(date +%F-%H%M%S)"
  dump="$BACKUP_DIR/tidepool-$stamp.sql"
  log "backing up database to $dump"
  # Compose interpolation reads the same .env the stack uses, so the password
  # never appears on this command line.
  "${COMPOSE[@]}" exec -T db sh -c \
    'exec mysqldump -uroot -p"$MYSQL_ROOT_PASSWORD" --single-transaction --routines --triggers "$MYSQL_DATABASE"' \
    > "$dump"
  test -s "$dump" || { log "backup is empty; aborting"; exit 1; }
  log "backup written ($(wc -c < "$dump") bytes)"
}

verify() {
  log "waiting for $HEALTH_URL"
  for _ in $(seq 1 60); do
    if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
      log "healthy"
      curl -fsS "$HEALTH_URL"; echo
      return 0
    fi
    sleep 2
  done
  log "health check failed; inspect: ${COMPOSE[*]} logs pool"
  return 1
}

deploy() {
  cd "$APP_DIR"
  git fetch --tags --force
  local previous
  previous="$(git describe --tags --always)"
  log "current revision: $previous"
  backup
  git pull --ff-only
  log "building and starting"
  "${COMPOSE[@]}" up -d --build
  if ! verify; then
    log "deploy failed; rolling back to $previous"
    git checkout "$previous"
    "${COMPOSE[@]}" up -d --build
    verify || log "ROLLBACK ALSO UNHEALTHY — database backup: $BACKUP_DIR"
    exit 1
  fi
  log "deployed $(git describe --tags --always)"
}

rollback() {
  cd "$APP_DIR"
  local target="${1:?usage: deploy.sh rollback <tag>}"
  backup
  git checkout "$target"
  "${COMPOSE[@]}" up -d --build
  verify
  log "rolled back to $target (upgrade again with: deploy.sh deploy)"
}

case "${1:-}" in
  deploy) deploy ;;
  rollback) shift; rollback "${1:-}" ;;
  verify) verify ;;
  *) echo "usage: $0 {deploy|rollback <tag>|verify}" >&2; exit 2 ;;
esac
