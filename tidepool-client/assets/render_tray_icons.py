#!/usr/bin/env python3
"""Render the Tidepool tray icons from the pinned clawffice-design cell sprite.

The menu-bar icon is the design system's 12x12 ``pool`` cell sprite, rasterised
with no anti-aliasing: one pixel per sprite cell at 1x, two at 2x, centred in a
16x16 / 32x32 canvas (the logical size the tray code pins). The Windows icon is
the design system's own multi-image ``tidepool-icon.ico`` copied verbatim.

Only the Python standard library is used, so the assets can be regenerated on a
machine that has nothing installed.

    python3 assets/render_tray_icons.py

The source JSON is the pinned copy in ``assets/cells.json``; rewrite that file
from ``clawffice-design`` at the commit recorded in ``assets/SOURCE.json`` and
update the hash before regenerating.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import struct
import zlib
from pathlib import Path

ASSETS = Path(__file__).resolve().parent

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


def rgba(hex_colour: str) -> tuple[int, int, int, int]:
    value = hex_colour.lstrip("#")
    return (int(value[0:2], 16), int(value[2:4], 16), int(value[4:6], 16), 255)


def write_png(path: Path, width: int, height: int, pixels: list[list[tuple[int, int, int, int]]]) -> None:
    """Write an 8-bit RGBA PNG without any third-party dependency."""
    raw = b"".join(b"\x00" + b"".join(bytes(pixel) for pixel in row) for row in pixels)

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def sprite() -> list[str]:
    data = json.loads((ASSETS / "cells.json").read_text())
    return data["sprites"]["pool"]["rows"]


def render(rows: list[str], canvas: int, scale: int) -> list[list[tuple[int, int, int, int]]]:
    grid = len(rows)
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


def main() -> None:
    rows = sprite()
    for symbol in "".join(rows):
        if symbol not in LEGEND:
            raise SystemExit(f"unknown cell symbol {symbol!r}")

    write_png(ASSETS / "tray-icon.png", 16, 16, render(rows, canvas=16, scale=1))
    write_png(ASSETS / "tray-icon@2x.png", 32, 32, render(rows, canvas=32, scale=2))

    source_icon = ASSETS / "tidepool-icon.ico"
    if source_icon.exists():
        shutil.copyfile(source_icon, ASSETS / "tray-icon.ico")
    else:
        raise SystemExit(
            "assets/tidepool-icon.ico is missing; copy it from "
            "clawffice-design/logos/tidepool/tidepool-icon.ico at the commit in assets/SOURCE.json"
        )

    for name in ("tray-icon.png", "tray-icon@2x.png", "tray-icon.ico"):
        digest = hashlib.sha256((ASSETS / name).read_bytes()).hexdigest()
        print(f"{name}: {digest}")


if __name__ == "__main__":
    main()
