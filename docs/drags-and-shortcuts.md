# Drags and shortcuts

The two ways people move windows with Snappy, followed from the adapter through the engine to the queue. Focus, the selection, and window outlines ride along with shortcuts, so they're here too.

## A drag

1. **Press.** The adapter reports a press. If live control is on, the engine (`drag.rs`) cancels any operation in flight, asks the platform for the window under the pointer, starts a `DragIntent` (`core::placement::drag`) with its frame, and turns drag monitoring on.
2. **Move.** Each pointer move or modifier change re-reads the frame. The intent decides whether this is a real move rather than a click or a resize: the window must travel a little, and a size change outside the title-bar strip cancels it until the next press.
3. **Show zones.** While the drag key is held and the window has moved, the engine finds the layout for the display under the pointer, hit-tests its zones, and asks the adapter to show an `OverlayScene`. Holding the swap key too reverses "When the zone is taken" for this drop, and the hovered zone is captioned "Swap" or "Stack".
4. **Drop.** On release, the engine checks the intent and the modifier again and finds the zone.
   - A drop on an empty zone, or one that stacks, becomes a snap.
   - One that swaps becomes a swap, which looks up the windows filling both zones and plans with `core::placement::swap`.

Either goes through [the queue and the settle loop](window-operations.md). The outcome is recorded in `drops.json` and the debug log ([Files and diagnostics](files-and-diagnostics.md)).

## A shortcut

1. **Register.** Whenever settings, displays, or live control change, the engine works out the shortcuts the current state needs (`core::shortcuts::desired_hotkeys`) and registers them if the set differs. Each comes with its `Combo`, the concrete key and modifiers with the Snappy key already expanded, so adapters never know the Snappy key exists.
2. **Press.** The adapter reports a press by ID, and the engine maps it back to a `HotKeyAction` (`actions.rs`). Shortcuts do nothing while the app is recording one.
3. **Act.** What happens next depends on the shortcut, as below.

### Zones and arrows

Zone and arrow shortcuts find the destination zone (`core::placement::navigate` for arrows) and submit a swap or a snap, as "When the zone is taken" says.

Arrow presses that arrive while the last move is still settling continue from where it was headed. So three presses move three zones, even in a slow app.

### The nearest empty zone

The nearest-empty-zone shortcut picks a zone with `core::placement::empty_zone`. With a selection, it places every selected window into empty zones at once, pairing windows with zones so the total distance is least, as one undo step.

### Focus

Focus shortcuts (`focus.rs`) ask `core::placement::focus` for the next window in a direction. They move between zones that hold a window when any does, and by window centers otherwise, and outline the focused window briefly.

### The selection

Selection shortcuts (`selection.rs`) keep an in-memory, ordered list of windows. Sends and workspace switches take the selection along.

## Outlines

Selected and just-focused windows are outlined on a layer of their own, apart from the zones (`marks.rs`, `Overlay::mark`). Outlines are redrawn only on displays whose outlines changed.
