# `tidepool-client/assets`

The tray icon is the clawffice-design Tidepool identity, not a drawn
placeholder. Nothing here is invented by this repository.

| File | What it is | Where it comes from |
|---|---|---|
| `cells.json` | the design system's cell sprites, including the 12x12 `pool` creature | copied from `clawffice-design/tokens/cells.json` at commit `995b41606e65` |
| `tidepool-icon.ico` | the Tidepool app icon, 7 images from 16 to 256 px | copied from `clawffice-design/logos/tidepool/tidepool-icon.ico` at the same commit |
| `tray-icon.ico` | the Windows tray icon | byte-for-byte copy of `tidepool-icon.ico` |
| `tray-icon.png` | the macOS menu-bar icon at 1x (16x16) | generated from the `pool` sprite |
| `tray-icon@2x.png` | the macOS menu-bar icon at 2x (32x32) | generated from the same sprite |

`SOURCE.json` records the design commit and a sha256 for every file.

## Menu-bar size

macOS draws the menu-bar image at 16pt (`tray_icon.rs` pins the logical size),
and marks it as a template image so AppKit tints it for light and dark menubars.
The sprite is 12x12, so it is rasterised with **one pixel per sprite cell at 1x**
and **two at 2x**, centred in the canvas. There is no antialiasing: every pixel
is either fully transparent or fully opaque, which keeps pixel art crisp instead
of turning it into a grey smudge. The colours use the Biolume palette's roles
(`tide` product hue, `claw` accent, `mist-50`, `ink-950`).

## Regenerating

Needs only Python 3 — no packages:

```sh
python3 assets/render_tray_icons.py
```

The script reads `cells.json` (keep it at the design commit), writes both PNGs,
copies `tidepool-icon.ico` to `tray-icon.ico`, and prints the new hashes for
`SOURCE.json`. The icon tests decode the PNGs and ICO and fail if the size,
crispness, palette or 1x/2x relationship drifts.

## Updating the design

Copy `tokens/cells.json` and `logos/tidepool/tidepool-icon.ico` from a newer
`clawffice-design` checkout, run the script, and update `SOURCE.json`'s commit
and hashes. The cell sprite is shared with the control plane and the console;
keeping the same commit makes the three clients recognisably one product.
