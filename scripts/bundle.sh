#!/usr/bin/env bash
# Builds build/UsageIsland.app (binary, Info.plist, fonts), signed ad hoc.
# Usage: scripts/bundle.sh [debug|release]   (default: debug)
set -euo pipefail

config="${1:-debug}"
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/build/UsageIsland.app"

cd "$root"
swift build -c "$config" --product UsageIsland
bin="$(swift build -c "$config" --show-bin-path)/UsageIsland"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/UsageIsland"
cp "$root/Resources/Info.plist" "$app/Contents/Info.plist"
cp -R "$root/Resources/Fonts" "$app/Contents/Resources/Fonts"
codesign --force --sign - "$app" >/dev/null
