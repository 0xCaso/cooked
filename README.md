# Usage Island

A macOS app that shows live usage of your Claude and Codex accounts (5-hour window, week, tokens per minute) in an island under the menu bar.


## Build and run

Needs macOS 26 or later and Xcode 26 or later (Swift 6.2+).

```sh
scripts/run.sh            # builds in debug, creates build/UsageIsland.app and launches it
scripts/run.sh release    # the same in release
swift test                # tests
```

`scripts/run.sh` quits the copy that is already open before it launches the new one. The app has no Dock icon: to quit it, right-click the island → "Quit Usage Island", or run `pkill -x UsageIsland`.

## Structure

- `Package.swift`: SwiftPM package, one executable and the tests.
- `Sources/UsageIsland/`: `UsageIslandApp` (launch, no Dock), `IslandPanel` (borderless window under the menu bar), `IslandPlacement` (where the window goes), `IslandPlaceholder` (the temporary island).
- `Resources/Info.plist`: `LSUIElement` for the app with no Dock icon.
