#!/usr/bin/env bash
# Build an installable Tidepool client artifact.
#
# The suite ships one shape per platform:
#
#   macos    Tidepool.app inside Tidepool-<version>-macos.zip
#            (drag the app into Applications; LSUIElement keeps it menu-bar only)
#   windows  Tidepool/ inside Tidepool-<version>-windows.zip
#            (tidepool-client.exe, the icon and a README)
#   linux    Tidepool-<version>-linux.tar.gz
#            (usr/bin, a .desktop entry and hicolor icons under a prefix)
#
# Usage:
#   scripts/package.sh [macos|windows|linux|all] [options]
#
# Options:
#   --profile release|debug   cargo profile to build (default: release)
#   --binary PATH             package an existing executable instead of building
#   --out DIR                 artifact directory (default: dist)
#   --version VERSION         override the version read from Cargo.toml
#
# Every run ends with a smoke check of the staged files and the archive, so a
# packaging change that drops the executable, the desktop entry or an icon fails
# here rather than on a person's machine.
set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM=""
PROFILE="release"
BINARY=""
OUT="$CLIENT_DIR/dist"
VERSION=""

usage() {
    sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        macos|windows|linux|all) PLATFORM="$1"; shift ;;
        --profile) PROFILE="${2:?--profile needs a value}"; shift 2 ;;
        --binary) BINARY="${2:?--binary needs a path}"; shift 2 ;;
        --out) OUT="${2:?--out needs a path}"; shift 2 ;;
        --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$PROFILE" in
    release) CARGO_FLAG="--release"; PROFILE_DIR="release" ;;
    debug) CARGO_FLAG=""; PROFILE_DIR="debug" ;;
    *) echo "profile must be release or debug" >&2; exit 2 ;;
esac

if [ -z "$PLATFORM" ]; then
    case "$(uname -s)" in
        Darwin) PLATFORM="macos" ;;
        Linux) PLATFORM="linux" ;;
        MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
        *) echo "cannot infer a platform; pass macos, windows or linux" >&2; exit 2 ;;
    esac
fi

if [ -z "$VERSION" ]; then
    VERSION="$(awk '
        /^\[package\]/ { in_package = 1; next }
        in_package && /^version[[:space:]]*=/ {
            value = $0
            sub(/^[^=]*=[[:space:]]*/, "", value)
            gsub(/["[:space:]]/, "", value)
            print value
            exit
        }
    ' "$CLIENT_DIR/Cargo.toml")"
fi
if [ -z "$VERSION" ]; then
    echo "could not read the package version from Cargo.toml" >&2
    exit 2
fi

if [ -z "$BINARY" ]; then
    (cd "$CLIENT_DIR" && cargo build $CARGO_FLAG)
    BINARY="$CLIENT_DIR/target/$PROFILE_DIR/tidepool-client"
    # Cargo appends `.exe` on Windows, including under Git Bash.
    if [ -f "$BINARY.exe" ]; then
        BINARY="$BINARY.exe"
    fi
fi
if [ ! -f "$BINARY" ]; then
    echo "executable not found: $BINARY" >&2
    exit 2
fi
BINARY="$(cd "$(dirname "$BINARY")" && pwd)/$(basename "$BINARY")"

OUT="$(mkdir -p "$OUT" && cd "$OUT" && pwd)"
ICONS="$OUT/icons"

render_icons() {
    command -v python3 >/dev/null 2>&1 || {
        echo "python3 is required to render the app icons" >&2
        exit 2
    }
    python3 "$CLIENT_DIR/scripts/render_app_icons.py" --out "$ICONS" >/dev/null
}

make_zip() { # source_dir, entry_name, destination
    local source="$1" entry="$2" destination="$3"
    rm -f "$destination"
    if command -v zip >/dev/null 2>&1; then
        (cd "$source" && zip -q -r "$destination" "$entry")
    else
        python3 - "$source" "$entry" "$destination" <<'PY'
import os, sys, zipfile
source, entry, destination = sys.argv[1:4]
with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
    for root, _, files in os.walk(os.path.join(source, entry)):
        for name in files:
            path = os.path.join(root, name)
            archive.write(path, os.path.relpath(path, source))
PY
    fi
}

smoke_macos() { # app_dir, archive
    local app="$1" archive="$2"
    [ -x "$app/Contents/MacOS/tidepool-client" ] || {
        echo "macOS bundle has no executable" >&2; exit 1
    }
    [ -f "$app/Contents/Info.plist" ] || {
        echo "macOS bundle has no Info.plist" >&2; exit 1
    }
    grep -q '<key>CFBundleIdentifier</key>' "$app/Contents/Info.plist" || {
        echo "Info.plist has no bundle identifier" >&2; exit 1
    }
    grep -q '<key>LSUIElement</key>' "$app/Contents/Info.plist" || {
        echo "Info.plist does not keep the app out of the Dock" >&2; exit 1
    }
    if command -v plutil >/dev/null 2>&1; then
        plutil -lint "$app/Contents/Info.plist" >/dev/null
    fi
    if command -v unzip >/dev/null 2>&1; then
        # Read the whole listing: `grep -q` would close the pipe early and
        # `pipefail` would report unzip's SIGPIPE as a packaging failure.
        unzip -l "$archive" | grep 'Tidepool.app/Contents/MacOS/tidepool-client' >/dev/null || {
            echo "macOS archive does not contain the app executable" >&2; exit 1
        }
    fi
    echo "macOS smoke OK: $archive"
}

smoke_windows() { # stage_dir, archive
    local stage="$1" archive="$2"
    [ -f "$stage/tidepool-client.exe" ] || {
        echo "Windows artifact has no executable" >&2; exit 1
    }
    [ -f "$stage/tidepool.ico" ] || {
        echo "Windows artifact has no icon" >&2; exit 1
    }
    if command -v unzip >/dev/null 2>&1; then
        unzip -l "$archive" | grep 'Tidepool/tidepool-client.exe' >/dev/null || {
            echo "Windows archive does not contain the executable" >&2; exit 1
        }
    fi
    echo "Windows smoke OK: $archive"
}

smoke_linux() { # prefix_dir, archive
    local prefix="$1" archive="$2"
    [ -x "$prefix/usr/bin/tidepool-client" ] || {
        echo "Linux artifact has no executable" >&2; exit 1
    }
    local desktop="$prefix/usr/share/applications/tidepool.desktop"
    [ -f "$desktop" ] || {
        echo "Linux artifact has no desktop entry" >&2; exit 1
    }
    grep -q '^\[Desktop Entry\]$' "$desktop" || {
        echo "desktop entry has no [Desktop Entry] group" >&2; exit 1
    }
    grep -q '^Exec=tidepool-client$' "$desktop" || {
        echo "desktop entry does not launch the client" >&2; exit 1
    }
    for size in 16 32 48 128 256 512; do
        [ -f "$prefix/usr/share/icons/hicolor/${size}x${size}/apps/tidepool.png" ] || {
            echo "Linux artifact has no ${size}x${size} icon" >&2; exit 1
        }
    done
    if command -v tar >/dev/null 2>&1; then
        tar -tzf "$archive" | grep 'usr/bin/tidepool-client' >/dev/null || {
            echo "Linux archive does not contain the executable" >&2; exit 1
        }
    fi
    echo "Linux smoke OK: $archive"
}

package_macos() {
    render_icons
    local root="$OUT/macos"
    local app="$root/Tidepool.app"
    local archive="$OUT/Tidepool-$VERSION-macos.zip"
    rm -rf "$root"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

    cp "$BINARY" "$app/Contents/MacOS/tidepool-client"
    chmod 755 "$app/Contents/MacOS/tidepool-client"
    cp "$ICONS/Tidepool.icns" "$app/Contents/Resources/Tidepool.icns"

    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Tidepool</string>
    <key>CFBundleExecutable</key>
    <string>tidepool-client</string>
    <key>CFBundleIconFile</key>
    <string>Tidepool</string>
    <key>CFBundleIdentifier</key>
    <string>com.clawffice.Tidepool</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Tidepool</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

    cat > "$app/Contents/Resources/README.txt" <<'README'
Tidepool
========

Drag Tidepool.app into Applications. The app lives in the menu bar; it has no
Dock icon (LSUIElement). Its settings and credentials are stored in the
per-user configuration directory, never inside the app bundle.

The bundle is unsigned. On first launch macOS may ask you to confirm opening an
app from an unidentified developer (System Settings > Privacy & Security).
README

    make_zip "$root" "Tidepool.app" "$archive"
    smoke_macos "$app" "$archive"
    echo "artifact: $archive"
}

package_windows() {
    local root="$OUT/windows"
    local stage="$root/Tidepool"
    local archive="$OUT/Tidepool-$VERSION-windows.zip"
    rm -rf "$root"
    mkdir -p "$stage"

    cp "$BINARY" "$stage/tidepool-client.exe"
    cp "$CLIENT_DIR/assets/tray-icon.ico" "$stage/tidepool.ico"

    cat > "$stage/README.txt" <<'README'
Tidepool
========

Run tidepool-client.exe. The app lives in the notification area; it has no
taskbar button at launch. Its settings and credentials are stored in the
per-user configuration directory (%APPDATA%\clawffice\Tidepool), never next to
the executable.

The executable is unsigned. Windows SmartScreen may ask you to confirm the first
run (More info > Run anyway).
README

    make_zip "$root" "Tidepool" "$archive"
    smoke_windows "$stage" "$archive"
    echo "artifact: $archive"
}

package_linux() {
    render_icons
    local root="$OUT/linux"
    local prefix="$root/Tidepool-$VERSION"
    local archive="$OUT/Tidepool-$VERSION-linux.tar.gz"
    rm -rf "$root"
    mkdir -p \
        "$prefix/usr/bin" \
        "$prefix/usr/share/applications" \
        "$prefix/usr/share/icons/hicolor"

    cp "$BINARY" "$prefix/usr/bin/tidepool-client"
    chmod 755 "$prefix/usr/bin/tidepool-client"

    for size in 16 32 48 128 256 512; do
        local dir="$prefix/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        cp "$ICONS/tidepool-$size.png" "$dir/tidepool.png"
    done

    cat > "$prefix/usr/share/applications/tidepool.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Tidepool
GenericName=Clawffice client
Comment=The Tidepool pool client
Exec=tidepool-client
Icon=tidepool
Terminal=false
Categories=Network;Utility;
Keywords=tidepool;clawffice;pool;agent;
StartupNotify=false
StartupWMClass=Tidepool
DESKTOP

    cat > "$prefix/README.txt" <<'README'
Tidepool
========

Install into /usr/local (or another prefix):

    sudo tar -xzf Tidepool-<version>-linux.tar.gz -C /usr/local

The tarball carries usr/bin/tidepool-client, a .desktop entry and hicolor icons,
so the app appears in the launcher after the desktop's menu cache refreshes.
Settings and credentials live in the per-user configuration directory
(~/.config/clawffice/Tidepool), never under the install prefix.
README

    tar -czf "$archive" -C "$root" "Tidepool-$VERSION"
    smoke_linux "$prefix" "$archive"
    echo "artifact: $archive"
}

case "$PLATFORM" in
    macos) package_macos ;;
    windows) package_windows ;;
    linux) package_linux ;;
    all) package_macos; package_windows; package_linux ;;
esac
