#!/usr/bin/env bash
# Builds UsageIsland.app in build/ and launches it (closing any copy already running).
# Usage: scripts/run.sh [debug|release]   (default: debug)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/build/UsageIsland.app"
"$root/scripts/bundle.sh" "${1:-debug}"

pkill -x UsageIsland 2>/dev/null || true
# open fails (-600) if the old copy is still quitting
while pgrep -x UsageIsland >/dev/null; do sleep 0.1; done
open "$app"
echo "Launched: $app"
