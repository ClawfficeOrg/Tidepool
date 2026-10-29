#!/bin/sh
# Write /config.json from POOL_URL at container start, so the image carries no
# pool URL. The official nginx entrypoint runs this before nginx starts.
#   POOL_URL empty or unset -> {}
#   POOL_URL set            -> must be http(s)://host[:port], nothing else
set -eu

out=/usr/share/nginx/html/config.json
url="${POOL_URL:-}"

if [ -z "$url" ]; then
  printf '{}\n' > "$out"
  exit 0
fi

# grep is line-based, so a multi-line value could pass on one line: refuse it first.
nl='
'
case "$url" in *"$nl"*) valid=0 ;; *) valid=1 ;; esac
if [ "$valid" = 0 ] || ! printf '%s' "$url" | grep -Eq '^https?://[A-Za-z0-9.-]+(:[0-9]+)?$'; then
  echo "40-pool-config: POOL_URL must look like https://host[:port] (no path, query, quotes, spaces or trailing slash)" >&2
  exit 1
fi

printf '{"poolUrl":"%s"}\n' "$url" > "$out"
