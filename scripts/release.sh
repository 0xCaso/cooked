#!/usr/bin/env bash
# Builds the zip for a GitHub release: build/Cooked-<version>.zip and its SHA-256.
# Signed ad hoc: no Developer ID, so macOS asks for "Open Anyway".
# Usage: scripts/release.sh <version>   (e.g. 0.1.0)
set -euo pipefail

version="${1:?usage: scripts/release.sh <version>}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must be like 0.1.0" >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/build/Cooked.app"
zip="$root/build/Cooked-$version.zip"

cd "$root"
"$root/scripts/bundle.sh" release

# The version goes in the bundle only; the build number counts the commits, so each release is higher.
plist="$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $(git rev-list --count HEAD)" "$plist"
# The plist changed after bundle.sh signed: sign again.
codesign --force --sign - "$app" 2>/dev/null
codesign --verify --deep --strict "$app"

# ditto keeps the signature, links and extended attributes that zip would drop.
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
(cd "$root/build" && shasum -a 256 "$(basename "$zip")" > "$(basename "$zip").sha256")

# Check the zip itself, not the folder it was made from.
check="$(mktemp -d)"
trap 'rm -rf "$check"' EXIT
ditto -x -k "$zip" "$check"
codesign --verify --deep --strict "$check/Cooked.app"
/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$check/Cooked.app/Contents/Info.plist" | grep -qx "$version"

# The copy in build/ would show up in Spotlight next to the installed one.
rm -rf "$app"
echo "Built: $zip"
cat "$zip.sha256"
