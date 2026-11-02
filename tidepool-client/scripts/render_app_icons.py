#!/usr/bin/env python3
"""Render the Tidepool application icons from the pinned cell sprite.

The packaging scripts need the same mark at every size a desktop expects: PNGs
for the Linux hicolor theme and an ICNS for the macOS bundle. The artwork is the
clawffice-design 12x12 ``pool`` cell sprite already used by the tray icon, so the
app icon and the menu-bar icon are the same creature.

Only the Python standard library is used. The output is deterministic: the same
``assets/cells.json`` produces byte-identical files.

    python3 scripts/render_app_icons.py --out dist/icons
"""

from __future__ import annotations

import argparse
import json
import struct
import zlib
from pathlib import Path

CLIENT = Path(__file__).resolve().parent.parent
ASSETS = CLIENT / "assets"

# Legend colours from `tokens/cells.json` and the Biolume palette. Resolved from
# clawffice-design at commit 995b41606e65; recorded in assets/SOURCE.json.
PRODUCT_HUE = "#4A9FE0"  # tide-500 (dark scheme), the sprite's '#'
ACCENT = "#F26B21"  # claw-500 (dark scheme), the sprite's 'o'
MIST_50 = "#E9FBF6"  # biolume mist-50, the sprite's 'w'
INK_950 = "#030C11"  # biolume ink-950, the sprite's 'k'

LEGEND = {
    "#": PRODUCT_HUE,
    "o": ACCENT,
    "w": MIST_50,
    "k": INK_950,
    ".": None,
}

# The sizes a desktop icon theme and an ICNS need.
PNG_SIZES = (16, 32, 48, 64, 128, 256, 512)
# ICNS type codes, smallest to largest. Each entry is a PNG payload.
ICNS_TYPES = {
    16: b"icp4",
    32: b"icp5",
    64: b"icp6",
    128: b"ic07",
    256: b"ic08",
    512: b"ic09",
}


def rgba(hex_colour: str) -> tuple[int, int, int, int]:
    value = hex_colour.lstrip("#")
    return (int(value[0:2], 16), int(value[2:4], 16), int(value[4:6], 16), 255)


def png_bytes(width: int, height: int, pixels: list[list[tuple[int, int, int, int]]]) -> bytes:
    """Encode an 8-bit RGBA PNG without any third-party dependency."""
    raw = b"".join(b"\x00" + b"".join(bytes(pixel) for pixel in row) for row in pixels)

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def sprite() -> list[str]:
    data = json.loads((ASSETS / "cells.json").read_text())
    return data["sprites"]["pool"]["rows"]


def render(rows: list[str], canvas: int) -> list[list[tuple[int, int, int, int]]]:
    grid = len(rows)
    # Largest integer scale that fits, so every pixel stays crisp.
    scale = max(1, canvas // grid)
    content = grid * scale
    margin = (canvas - content) // 2
    pixels = [[(0, 0, 0, 0) for _ in range(canvas)] for _ in range(canvas)]

    for y, row in enumerate(rows):
        for x, symbol in enumerate(row):
            colour = LEGEND.get(symbol)
            if colour is None:
                continue
            for dy in range(scale):
                for dx in range(scale):
                    pixels[margin + y * scale + dy][margin + x * scale + dx] = rgba(colour)
    return pixels


def icns(pngs: dict[int, bytes]) -> bytes:
    """Build an ICNS file from PNG payloads (the modern ICNS form)."""
    body = b""
    for size, kind in ICNS_TYPES.items():
        data = pngs[size]
        body += kind + struct.pack(">I", len(data) + 8) + data
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="dist/icons", help="directory to write into")
    args = parser.parse_args()

    out = Path(args.out)
    if not out.is_absolute():
        out = CLIENT / out
    out.mkdir(parents=True, exist_ok=True)

    rows = sprite()
    for symbol in "".join(rows):
        if symbol not in LEGEND:
            raise SystemExit(f"unknown cell symbol {symbol!r}")

    pngs: dict[int, bytes] = {}
    for size in PNG_SIZES:
        pngs[size] = png_bytes(size, size, render(rows, size))
        (out / f"tidepool-{size}.png").write_bytes(pngs[size])

    (out / "Tidepool.icns").write_bytes(icns(pngs))
    print(f"wrote {len(PNG_SIZES)} PNGs and Tidepool.icns to {out}")


if __name__ == "__main__":
    main()
