#!/usr/bin/env bash
# Builds UsageIsland.app in build/ and launches it (closing any copy already running).
# Usage: scripts/run.sh [debug|release]   (default: debug)
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

pkill -x UsageIsland 2>/dev/null || true
open "$app"
echo "Launched: $app"
