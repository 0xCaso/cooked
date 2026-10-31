#!/usr/bin/env bash
# Builds build/Cooked.app (binary, Info.plist, fonts, icon), signed ad hoc.
# Usage: scripts/bundle.sh [debug|release]   (default: debug)
set -euo pipefail

config="${1:-debug}"
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/build/Cooked.app"

cd "$root"
swift build -c "$config" --product Cooked
bin="$(swift build -c "$config" --show-bin-path)/Cooked"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Cooked"
cp "$root/Resources/Info.plist" "$app/Contents/Info.plist"
cp -R "$root/Resources/Fonts" "$app/Contents/Resources/Fonts"
# The macOS 26 layered icon: Assets.car for Tahoe, AppIcon.icns for older Finder views.
xcrun actool "$root/Resources/AppIcon.icon" --compile "$app/Contents/Resources" --platform macosx \
    --minimum-deployment-target 26.0 --app-icon AppIcon --output-partial-info-plist /dev/null >/dev/null
codesign --force --sign - "$app" >/dev/null
