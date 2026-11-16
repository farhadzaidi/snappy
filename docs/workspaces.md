# Workspaces

A workspace is a group of windows, and one group shows at a time. Switching hides the others and brings the chosen group back exactly where it was.

## Sessions

`core::workspace::Session` keeps each window's workspace, its frame, and where it waits while hidden. It makes every change all or nothing:

1. Write the journal of windows about to be parked first.
2. Bring incoming windows back before parking outgoing ones, so the screen is never empty mid-switch.
3. Check each move, and on a refusal reverse the moves already made. A window that closes meanwhile just leaves the session.

## A switch

The engine (`workspaces/switching.rs`) refuses a switch while workspaces are paused or the target is already showing, and holds it while displays settle. Otherwise it:

- enrolls the focused app's windows,
- takes the selection along,
- focuses the target workspace's last focused window,
- saves the active workspace and then membership, and
- refreshes shortcuts and overlays.

## Parking

How a window is hidden is the platform's `ParkingPolicy`:

- **macOS**: at an exposed lower corner, with a sliver left on screen, since macOS pulls fully off-screen windows back.
- **Windows**: entirely off screen.
- **Linux**: on a native desktop of Snappy's own, frames untouched.

The platform pages have the details.

## Containment

When an app or the system brings a parked window back, the engine (`workspaces/containment.rs`) parks it again at once, tries once more a second later, and only then reports it, naming its workspace.

## Display changes

Displays change in bursts, as when a laptop wakes. The engine (`workspaces/events.rs`) waits until 350 ms have passed without a change. Then it re-places every window for the final displays in one session change. If that fails, it retries once after 750 ms, and otherwise hides inactive workspaces' windows one by one.

## Other desktops and lost control

Workspaces belong to the native desktop (Space, virtual desktop) they started on, which each adapter tracks with its `DesktopGuard`. They pause on any other.

When live control is lost, workspaces are suspended without moving anything, and resume when it returns.

## Restarts

Quitting returns every parked window. Stopping for a restart (`SIGUSR2`, or `snappy --stop-for-restart` on Windows), a crash, or a panic leaves the journal and membership files as they are. The next start puts each window back in its workspace without moving it (`workspaces/recovery.rs`).

Windows are recognized across processes by a `RecoveryKey`: the process ID, the process's start time, and a per-window token from the adapter. So a reused process ID never matches. `core::workspace::relaunch` decides what each journaled window becomes.

## Tabs

On macOS, each native tab is a window of its own. The session treats a tab group as one window, acting on the shown tab and carrying the hidden ones along. [macOS](platforms/macos.md#native-tabs) explains how Snappy learns which tabs belong together.
