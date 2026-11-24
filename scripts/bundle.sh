#!/usr/bin/env bash
# Builds build/Cooked.app (the Rust binary, Info.plist, fonts, icon), signed ad hoc.
# Always the `perf` Cargo profile: optimised, non-incremental, no debug info (never the default `release` profile).
# Usage: scripts/bundle.sh
#   COOKED_FEATURES=tooling scripts/bundle.sh   also builds Rune's agent tooling (--snap, --tree) in, for checks
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/build/Cooked.app"

cd "$root"
features=()
[[ -n "${COOKED_FEATURES:-}" ]] && features=(--features "$COOKED_FEATURES")
cargo build --profile perf -p cooked "${features[@]+"${features[@]}"}"
bin="$root/target/perf/cooked"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# Info.plist names the executable `Cooked`.
cp "$bin" "$app/Contents/MacOS/Cooked"
# Local symbols only cost size (about 4 MB); panics keep their messages.
strip -x "$app/Contents/MacOS/Cooked"
cp "$root/Resources/Info.plist" "$app/Contents/Info.plist"
# The binary embeds its TTF fonts; the bundle keeps the folder Info.plist's ATSApplicationFontsPath names, and the
# fonts' licence.
cp -R "$root/Resources/Fonts" "$app/Contents/Resources/Fonts"
# The macOS 26 layered icon: Assets.car for Tahoe, AppIcon.icns for older Finder views.
xcrun actool "$root/Resources/AppIcon.icon" --compile "$app/Contents/Resources" --platform macosx \
    --minimum-deployment-target 26.0 --app-icon AppIcon --output-partial-info-plist /dev/null >/dev/null
codesign --force --sign - "$app" >/dev/null
echo "Bundled: $app ($(du -sh "$app" | cut -f1))"
