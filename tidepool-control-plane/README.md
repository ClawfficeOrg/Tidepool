# Tidepool control plane

The browser app an operator uses to run a Tidepool pool: hosts, connections,
projects, triggers, runs, rooms and settings. Svelte 5, Vite and TypeScript,
built on the Clawffice design system.

This is the 2.1.1 scaffold. It ships the shell: all five palettes with Biolume
as the default, the tide product hue driving focus and selection, light and dark,
hash routing, the typed SDK client, and an error boundary that renders a
problem-details failure with its request id. The pages themselves arrive in
tasks 2.1.2 to 2.1.10.

## Run it

```sh
npm install
npm run dev          # http://localhost:5173
npm run build        # dist/
npm test             # vitest, no DOM and no network
npm run typecheck
```

With no pool configured the app builds, starts and says so: nothing is fetched
until a pool URL is set (`VITE_POOL_URL` at build time, or the value saved by
settings).

## Design tokens

`clawffice-design` is the source of truth. It is not vendored into this package:
`src/lib/design/clawffice-design.snapshot.json` is a pinned snapshot generated
from a checkout, and `src/lib/design/design.drift.test.ts` fails when the
checkout and the snapshot disagree.

```sh
# beside this app: ../clawffice-design, or set CLAWFFICE_DESIGN_DIR
npm run design:check      # fail when the snapshot is stale
npm run design:snapshot   # regenerate it
```

Every colour, radius, space and font size in `src/app.css` is a `--tp-*` custom
property produced by `src/lib/design/tokens.ts`. Focus and selection use
Tidepool's product hue (`tide`: `#4A9FE0` dark, `#1864AB` light), not the
palette's primary, so the app keeps its identity across all five palettes.

Status is never colour alone: `src/lib/status.ts` gives every state an icon and
a word, and `StatusBadge.svelte` renders all three.

## Assets

`public/tidepool-mark.svg`, `tidepool-icon.svg`, the icon PNGs and
`tidepool-cell.svg` come from `clawffice-design/logos/tidepool` and
`tokens/cells.json` (the 12x12 "pool" sprite, rendered by `snapshot.mjs`). The
favicon is the cell sprite; the app icon and manifest use the Tidepool icon.

## Container image

The built app is static, so the image is a multi-stage build that runs
`vite build` and serves `dist/` with nginx. The build context must be the
repository root, because the app aliases the SDK source in `sdk/ts`.

```bash
# from the repository root
docker build -f tidepool-control-plane/Dockerfile -t tidepool-control-plane:local .
docker run --rm -p 8088:80 tidepool-control-plane:local   # http://localhost:8088
```

No pool URL or secret is baked in; enter the pool URL in the app. `/healthz`
returns 200 for health checks, and unknown paths fall back to `index.html`.
There is no committed lockfile yet, so the image uses `npm install`; switch the
Dockerfile to `npm ci` once one exists.
