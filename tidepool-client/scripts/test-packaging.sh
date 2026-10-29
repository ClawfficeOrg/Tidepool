#!/usr/bin/env bash
# Smoke-test every platform artifact without cross-compiling.
#
# The packaging script accepts an existing executable, so this test feeds it a
# tiny stub and checks the *shape* of each artifact: the macOS bundle and zip,
# the Windows zip and the Linux tarball with its desktop entry and icons. The
# real executable is only different in bytes.
#
# This runs on any Unix runner with bash, tar and python3; it never starts the
# app, needs no display and installs nothing.
set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tidepool-package-smoke.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

STUB="$WORK/tidepool-client"
cat > "$STUB" <<'STUB'
#!/bin/sh
echo "Tidepool packaging stub"
STUB
chmod 755 "$STUB"

"$CLIENT_DIR/scripts/package.sh" all \
    --binary "$STUB" \
    --out "$WORK/dist" \
    --version 0.0.0-smoke

python3 - "$WORK/dist" <<'PY'
import os
import sys
import tarfile
import zipfile

dist = sys.argv[1]


def need(path):
    if not os.path.isfile(path):
        raise SystemExit(f"missing artifact: {path}")
    return path


macos = need(os.path.join(dist, "Tidepool-0.0.0-smoke-macos.zip"))
windows = need(os.path.join(dist, "Tidepool-0.0.0-smoke-windows.zip"))
linux = need(os.path.join(dist, "Tidepool-0.0.0-smoke-linux.tar.gz"))

with zipfile.ZipFile(macos) as archive:
    names = set(archive.namelist())
    for required in (
        "Tidepool.app/Contents/Info.plist",
        "Tidepool.app/Contents/MacOS/tidepool-client",
        "Tidepool.app/Contents/Resources/Tidepool.icns",
    ):
        if required not in names:
            raise SystemExit(f"macOS archive missing {required}")
    plist = archive.read("Tidepool.app/Contents/Info.plist").decode()
    if "<key>LSUIElement</key>" not in plist:
        raise SystemExit("macOS Info.plist does not hide the Dock icon")
    if "com.clawffice.Tidepool" not in plist:
        raise SystemExit("macOS Info.plist has the wrong bundle identifier")
    mode = archive.getinfo("Tidepool.app/Contents/MacOS/tidepool-client").external_attr >> 16
    if not mode & 0o111:
        raise SystemExit("macOS executable is not executable")

with zipfile.ZipFile(windows) as archive:
    names = set(archive.namelist())
    for required in (
        "Tidepool/tidepool-client.exe",
        "Tidepool/tidepool.ico",
        "Tidepool/README.txt",
    ):
        if required not in names:
            raise SystemExit(f"Windows archive missing {required}")

with tarfile.open(linux, "r:gz") as archive:
    members = {member.name: member for member in archive.getmembers()}
    required = [
        "Tidepool-0.0.0-smoke/usr/bin/tidepool-client",
        "Tidepool-0.0.0-smoke/usr/share/applications/tidepool.desktop",
    ] + [
        f"Tidepool-0.0.0-smoke/usr/share/icons/hicolor/{size}x{size}/apps/tidepool.png"
        for size in (16, 32, 48, 128, 256, 512)
    ]
    for name in required:
        if name not in members:
            raise SystemExit(f"Linux archive missing {name}")
    binary = members["Tidepool-0.0.0-smoke/usr/bin/tidepool-client"]
    if not binary.mode & 0o111:
        raise SystemExit("Linux executable is not executable")
    desktop = archive.extractfile(
        "Tidepool-0.0.0-smoke/usr/share/applications/tidepool.desktop"
    ).read().decode()
    if "[Desktop Entry]" not in desktop or "Exec=tidepool-client" not in desktop:
        raise SystemExit("Linux desktop entry does not launch the client")

print("packaging smoke OK: macOS bundle zip, Windows zip, Linux tarball")
PY

echo "test-packaging: PASS"
