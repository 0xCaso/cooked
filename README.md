# Cooked

Know you're cooked before Claude does.

A macOS app that shows live usage of your Claude and Codex accounts (5-hour window, week, tokens per minute) in an island under the menu bar.


## Build and run

Needs macOS 26 or later and Xcode 26 or later (Swift 6.2+).

```sh
scripts/run.sh            # builds in debug, creates build/Cooked.app and launches it
scripts/run.sh release    # the same in release
scripts/install.sh        # release build installed as /Applications/Cooked.app, then launched
swift test                # tests
```

## Use

- Open **Cooked** from Applications, Spotlight or Launchpad. If it already runs, opening it again shows the popover.
- Click the island to open or close the popover; Esc, a click elsewhere or any key closes it. Drag the island to move it: the position is kept per screen.
- Right-click the island: **Data** (Live, Demo: calm, Demo: peak), **Glass** (System, Clear, Darker), **Quit Cooked**.
- Live data, every 5 s, all read-only: the 5h and 7-day percentages, resets and limit status come from the rate-limit events T3 Code logs in `~/.t3/userdata/logs/provider/` (`LimitLog`, which also projects "full at" and the p99 alert threshold). Tokens per minute come from the Claude transcripts (`~/.claude*/projects/**/*.jsonl`, `TranscriptLog`) and, for Codex, from T3 Code's `~/.t3/userdata/statev2.sqlite` (`TokenLog`), counted as input − cached input + output. If no account has limits yet, the menu says so.
- `Cooked --limits` prints every account's limits as the app reads them, one line each.
- `Cooked --snapshots <dir> --live` renders the island and the open popover from the live data to PNG.
- `Cooked --feed demoCalm|demoPeak|live [--open]` starts on that feed (and opens the popover), to check a state on the real glass.

`scripts/run.sh` quits the copy that is already open before it launches the new one. The app has no Dock icon: to quit it, right-click the island → "Quit Cooked", or run `pkill -x Cooked`.

## Structure

- `Package.swift`: SwiftPM package, one executable and the tests.
- `Sources/Cooked/`: `CookedApp` (launch, no Dock), `IslandPanel` (borderless window under the menu bar, click-through, drag), `IslandPlacement` and `PositionStore` (where the window goes), `Views/` (island, popover, surfaces, motion), `Dots/` (the dot-matrix charts), `Model/` (usage snapshot, status.json and database readers, live mapping, demo data), `Style/` (palette, type, contrast).
- `Resources/Info.plist`: `LSUIElement` for the app with no Dock icon.
- `scripts/`: `bundle.sh` (builds the .app), `run.sh` (bundle + launch), `install.sh` (release into /Applications).
