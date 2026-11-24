#!/usr/bin/env bash
# Installs a perf build as "/Applications/Cooked.app" (Spotlight, Launchpad, Finder) and launches it.
# The settings stay: the installed app reads the same `com.0xcaso.Cooked` defaults as the copy it replaces.
# Usage: scripts/install.sh [--no-launch]
#   COOKED_INSTALL_DIR=<dir>   installs into <dir> instead of /Applications; only a Cooked running from
#                              <dir>/Cooked.app is quit, and nothing in /Applications is touched
#   --no-launch                installs without opening the app
set -euo pipefail

launch=1
case "${1:-}" in
    "") ;;
    --no-launch) launch=0 ;;
    *) echo "usage: scripts/install.sh [--no-launch]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${COOKED_INSTALL_DIR:-/Applications}"
dir="${dir%/}"
dest="$dir/Cooked.app"
"$root/scripts/bundle.sh"

if [[ "$dir" == "/Applications" ]]; then
    # UsageIsland: the executable before the rename to Cooked.
    pkill -x Cooked 2>/dev/null || true
    pkill -x UsageIsland 2>/dev/null || true
    # open fails (-600) if the old copy is still quitting
    while pgrep -x Cooked >/dev/null || pgrep -x UsageIsland >/dev/null; do sleep 0.1; done
    # The app was called Usage Island before it became Cooked.
    rm -rf "/Applications/Usage Island.app"
else
    # Another folder: only the copies running from it, by their exact executable path.
    exe="$dest/Contents/MacOS/Cooked"
    running() { ps -axo pid=,comm= | awk -v exe="$exe" '{ pid = $1; $1 = ""; sub(/^ /, "") } $0 == exe { print pid }'; }
    pids="$(running)"
    if [[ -n "$pids" ]]; then
        # shellcheck disable=SC2086 # one PID per word
        kill $pids 2>/dev/null || true
        while [[ -n "$(running)" ]]; do sleep 0.1; done
    fi
    mkdir -p "$dir"
fi
rm -rf "$dest"
cp -R "$root/build/Cooked.app" "$dest"
# The copy in build/ would show up in Spotlight next to the installed one.
rm -rf "$root/build/Cooked.app"
if [[ "$launch" == 1 ]]; then
    open "$dest"
fi
echo "Installed: $dest"
