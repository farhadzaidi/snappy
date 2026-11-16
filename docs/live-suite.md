# The live suite

`crates/live` runs the real daemon against real windows with real input, and checks where the windows actually end up. It's the closest thing to a person using Snappy, and it only ever runs in a [sandbox](sandboxes.md).

## Two programs

- **`snappy-fixture`** opens plain windows on request, over its standard input and output: ordinary ones, ones with a minimum size, ones that size in steps like a terminal, and, on macOS, native tabs. Each window reports its own frame, which is the ground truth.
- **`snappy-live`** writes scratch settings and starts the daemon with `SNAPPY_HOME` pointing at a scratch folder and the debug log on. It posts pointer and keyboard input the way hardware does, runs the scenarios, and writes `results.json` with a screenshot of each failure.

`snappy-live` kills any running Snappy and drives the pointer and keyboard, so it refuses to start unless a sandbox script has set `SNAPPY_LIVE_SANDBOX=1`.

## The scenarios

The scenarios (`crates/live/src/scenarios/`) are grouped by feature: moves, swaps, the overlay and menu bar item, workspaces (switching, stepping, sending, restarting, quitting, display changes), the selection, tabs, focus, the Snappy key, and Caps Lock. How each desktop is driven, its input, reading frames, and screenshots, is in `crates/live/src/desktop/`.

- **Shortcuts from settings.** Scenarios take every shortcut from the run's settings with the Snappy key expanded. So one scenario presses Control+Option+Command+1 on macOS and Ctrl+Alt+Super+1 on Linux.
- **What Snappy measured.** Scenarios read the daemon's debug log for what Snappy measured and decided, such as settle times and overlay captions.
- **Fast hands.** Drags let go of the modifier right after the button, as fast as input allows. So an adapter that reads modifiers late loses the drop.

The sandbox scripts take a scenario name prefix to run only some scenarios ([CONTRIBUTING.md](../CONTRIBUTING.md#testing)).

## Skipped scenarios

A scenario that can't apply to a desktop skips with the reason, which counts as a pass:

- tabs and display-size changes run only on macOS, and so does the check that the menu opens every time;
- snapping to a cell grid is skipped for Wayland windows, which can't ask to size in steps.

## What posted input can't show

Posted input doesn't pass every point a real keyboard's does. On macOS it joins the event stream after the HID remap and after the window server drops Control-Option-Command-Delete. So the Caps Lock scenario presses F18 there, and a passing scenario doesn't prove a chord reaches apps ([macOS](platforms/macos.md#limitations)).
