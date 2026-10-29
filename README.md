# Usage Island

A macOS app that shows live usage of your Claude and Codex accounts (5-hour window, week, tokens per minute) in an island under the menu bar.


## Build and run

Needs macOS 26 or later and Xcode 26 or later (Swift 6.2+).

```sh
scripts/run.sh            # builds in debug, creates build/UsageIsland.app and launches it
scripts/run.sh release    # the same in release
scripts/install.sh        # release build installed as /Applications/Usage Island.app, then launched
swift test                # tests
```

## Use

- Open **Usage Island** from Applications, Spotlight or Launchpad. If it already runs, opening it again shows the popover.
- Click the island to open or close the popover; Esc, a click elsewhere or any key closes it. Drag the island to move it: the position is kept per screen.
- Right-click the island: **Demo: peak / Demo: calm**, **Glass** (System, Clear, Darker), **Quit Usage Island**.
- The data is demo data for now: reading the watcher's `status.json` is the next step.

`scripts/run.sh` quits the copy that is already open before it launches the new one. The app has no Dock icon: to quit it, right-click the island → "Quit Usage Island", or run `pkill -x UsageIsland`.

## Structure

- `Package.swift`: SwiftPM package, one executable and the tests.
- `Sources/UsageIsland/`: `UsageIslandApp` (launch, no Dock), `IslandPanel` (borderless window under the menu bar, click-through, drag), `IslandPlacement` and `PositionStore` (where the window goes), `Views/` (island, popover, surfaces, motion), `Dots/` (the dot-matrix charts), `Model/` (usage snapshot, demo data), `Style/` (palette, type, contrast).
- `Resources/Info.plist`: `LSUIElement` for the app with no Dock icon.
- `scripts/`: `bundle.sh` (builds the .app), `run.sh` (bundle + launch), `install.sh` (release into /Applications).
