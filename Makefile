# make — Tidepool operations
#
#   make env                       generate .env with fresh secrets
#   make config                    validate the compose stack
#   make up                        build and start where you are
#   make down | logs | health
#   make deploy                    first deploy: build, start, verify
#   make redeploy                  pull, back up, rebuild, verify, roll back on failure
#   make rollback TAG=v0.5.0       return to a previous release
#   make backup                    dump the database only
#   make verify                    health check only
#
# The proxy is a variable, so the same targets work for each documented setup:
#
#   make deploy PROXY=traefik HOSTNAME=pool.example.com   # or set it in .env
#   make deploy PROXY=caddy   HOSTNAME=pool.example.com
#   make deploy PROXY=nginx   HOSTNAME=pool.example.com
#   make up     PROXY=none                                # no bundled proxy
#
# HOSTNAME goes to Traefik/Caddy/nginx and must match TIDEPOOL_PUBLIC_ORIGIN.
# When HOSTNAME is omitted, the value in .env (TIDEPOOL_HOSTNAME) is used.

PROXY    ?= traefik
PORT     ?= 8080

export COMPOSE_PROXY := $(PROXY)
# Only override the .env value when HOSTNAME is given explicitly.
ifneq ($(strip $(HOSTNAME)),)
export TIDEPOOL_HOSTNAME := $(HOSTNAME)
endif
export TIDEPOOL_PORT := $(PORT)
export TIDEPOOL_DIR ?= $(CURDIR)

FILES := -f docker-compose.yml

ifeq ($(PROXY),traefik)
FILES += -f deploy/docker-compose.vps.yml
else ifeq ($(PROXY),caddy)
FILES += -f deploy/docker-compose.vps.yml -f deploy/docker-compose.caddy.yml
else ifeq ($(PROXY),nginx)
FILES += -f deploy/docker-compose.vps.yml -f deploy/docker-compose.nginx.yml
else ifeq ($(PROXY),none)
FILES += -f deploy/docker-compose.vps.yml
else
$(error PROXY must be one of: traefik, caddy, nginx, none)
endif

COMPOSE := docker compose $(FILES)

.PHONY: help env config up down logs health deploy redeploy rollback backup verify

help:
	@sed -n '2,22p' Makefile | sed 's/^# \{0,1\}//'

env:
	./scripts/gen-env.sh

config:
	$(COMPOSE) config >/dev/null
	@echo "compose OK for proxy=$(PROXY) hostname=$(if $(strip $(HOSTNAME)),$(HOSTNAME),from .env) port=$(PORT)"

up:
	$(COMPOSE) up -d --build

down:
	$(COMPOSE) down

logs:
	$(COMPOSE) logs -f pool

health:
	curl -fsS "http://127.0.0.1:$(PORT)/healthz" && echo

deploy:
	./deploy/deploy.sh deploy

redeploy:
	./deploy/deploy.sh deploy

rollback:
	@test -n "$(TAG)" || { echo "usage: make rollback TAG=<tag>" >&2; exit 2; }
	./deploy/deploy.sh rollback "$(TAG)"

backup:
	./deploy/deploy.sh backup

verify:
	./deploy/deploy.sh verify
