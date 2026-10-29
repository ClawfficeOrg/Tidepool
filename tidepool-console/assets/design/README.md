# `tidepool-console/assets/design`

A pinned copy of the `clawffice-design` files the console renders. Nothing here
is hand-written: each file is byte-for-byte the design repository's file at the
commit recorded in [`SOURCE.json`](SOURCE.json).

| Path | Used for |
|---|---|
| `banners/tidepool/tidepool-banner-static.ans` | the startup splash and about banner, parsed as true-colour ANSI |
| `banners/tidepool/tidepool-banner-static.txt` | the banner's plain-text twin; the tests prove the parser reproduces it |
| `tokens/core.json` | the default palette, product hues (`tide` for focus, `claw` for the sprite accent) |
| `tokens/apps/tidepool.json` | the app name and wordmark |
| `tokens/cells.json` | the 12x12 `pool` sprite drawn on the about screen |
| `tokens/palettes/*.json` | all five Clawffice palettes; `biolume` is the default |

## Regenerating

From a checkout of `clawffice-design`:

```sh
DESIGN=../clawffice-design
for file in \
  banners/tidepool/tidepool-banner-static.ans \
  banners/tidepool/tidepool-banner-static.txt \
  tokens/core.json \
  tokens/apps/tidepool.json \
  tokens/cells.json \
  tokens/palettes/biolume.json \
  tokens/palettes/boiled.json \
  tokens/palettes/deep-tide.json \
  tokens/palettes/phosphor.json \
  tokens/palettes/sea-glass.json
do
  cp "$DESIGN/$file" "tidepool-console/assets/design/$file"
done
```

Then rewrite `SOURCE.json`'s `source.commit` and `sha256` values. The tests
compare the copies to `$CLAWFFICE_DESIGN_DIR` or a sibling checkout when one is
present, so a forgotten regeneration is a test failure on a developer machine
and still builds offline in CI.

## Licence and provenance

These files come from `clawffice-design`, the suite's design system. They are
referenced by the console the same way the control plane references its pinned
snapshot: the copy exists so an offline checkout builds, and the commit records
what it is.
