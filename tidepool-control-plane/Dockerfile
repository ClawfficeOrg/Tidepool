# Tidepool control plane: build the static Svelte/Vite app, serve it with nginx.
#
# Build from the REPOSITORY ROOT (the app aliases ../sdk/ts at build time):
#   docker build -f tidepool-control-plane/Dockerfile -t tidepool-control-plane:local .
#
# No secrets and no pool URL are baked in; the pool URL is entered in the app.

# Node 24 LTS ships npm 11; npm 10.x crashes resolving this package.json.
FROM node:24.11.0-alpine3.22 AS build
WORKDIR /repo

# vite.config.ts aliases '@clawffice/tidepool-sdk' to ../sdk/ts/src/browser.js,
# so the SDK source is copied in and needs no install of its own.
COPY sdk/ts ./sdk/ts

# Dependency layer first, so source edits do not reinstall.
# There is no committed package-lock.json, so `npm ci` is not possible yet;
# switch to `npm ci` once a lockfile is committed.
WORKDIR /repo/tidepool-control-plane
COPY tidepool-control-plane/package.json ./
RUN npm install --no-audit --no-fund

COPY tidepool-control-plane/ ./
RUN npm run build

FROM nginx:1.27.3-alpine3.20
COPY tidepool-control-plane/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /repo/tidepool-control-plane/dist /usr/share/nginx/html
EXPOSE 80
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1/healthz || exit 1
