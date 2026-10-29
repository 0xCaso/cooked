#!/usr/bin/env bash
# Installs a release build as "/Applications/Usage Island.app" (Spotlight, Launchpad, Finder) and launches it.
# Usage: scripts/install.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="/Applications/Usage Island.app"
"$root/scripts/bundle.sh" release

pkill -x UsageIsland 2>/dev/null || true
# open fails (-600) if the old copy is still quitting
while pgrep -x UsageIsland >/dev/null; do sleep 0.1; done
rm -rf "$dest"
cp -R "$root/build/UsageIsland.app" "$dest"
# The copy in build/ would show up in Spotlight next to the installed one.
rm -rf "$root/build/UsageIsland.app"
open "$dest"
echo "Installed: $dest"
