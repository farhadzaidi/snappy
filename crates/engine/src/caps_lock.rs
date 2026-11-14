//! Caps Lock as the Snappy key: bringing Caps Lock to what the settings ask, keeping what
//! undoes the change in a state file while it is in place, so the next run can undo it
//! after a crash, and deciding what each press of Caps Lock was.

use serde::{Deserialize, Serialize};
use snappy_core::caps_lock::{CapsLockPress, CapsLockRelease, CapsLockState, CapsLockStep, steps};
use snappy_core::keys::Mods;
use snappy_platform::{CapsLockInput, CapsLockUndo, Platform};

use crate::core::Core;
use crate::store::{read_state, remove_state, write_state};

/// What the engine knows about Caps Lock.
#[derive(Debug)]
pub(crate) struct CapsLockController {
    state: CapsLockState,
    /// What undoes a change an earlier run left behind, while `state` says there is one.
    left_over: Option<CapsLockUndo>,
    /// The press in progress, while Caps Lock is down.
    press: Option<CapsLockPress>,
    /// The modifiers the platform last refused, so a refusal isn't tried again and
    /// announced on every reconfiguration, only once something it depends on changes.
    refused: Option<Mods>,
    /// Why Caps Lock isn't the Snappy key though the setting asks for it.
    problem: Option<String>,
}

impl Default for CapsLockController {
    fn default() -> Self {
        Self {
            state: CapsLockState::Normal,
            left_over: None,
            press: None,
            refused: None,
            problem: None,
        }
    }
}

impl CapsLockController {
    /// Why Caps Lock isn't the Snappy key though the setting asks for it.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }
}

/// The state file's contents.
#[derive(Serialize, Deserialize)]
struct Saved {
    undo: CapsLockUndo,
}

/// Note a change an earlier run left in place, from its state file, so that the first
/// `configure` undoes it.
pub(crate) fn start<P: Platform>(core: &mut Core<P>) {
    if let Some(saved) = read_state::<Saved>(&core.paths.caps_lock()) {
        core.caps_lock.state = CapsLockState::LeftOver;
        core.caps_lock.left_over = Some(saved.undo);
    }
}

/// Caps Lock stands for the Snappy key while the setting asks for it and Snappy can
/// control the desktop, which on macOS needs the permission that taps keys.
fn wanted<P: Platform>(core: &Core<P>) -> Option<Mods> {
    (core.settings.caps_lock_as_snappy_key && core.has_permission() && core.live_control.is_available())
        .then(|| core.settings.snappy_key.modifiers())
}

/// Bring Caps Lock to what the settings and the platform's state ask, announcing a
/// refusal once.
pub(crate) fn configure<P: Platform>(core: &mut Core<P>) {
    let wanted = wanted(core);
    if wanted.is_none() {
        core.caps_lock.refused = None;
        core.caps_lock.problem = None;
    } else if wanted == core.caps_lock.refused {
        return;
    }
    if let Err(reason) = bring_to(core, wanted) {
        core.caps_lock.refused = wanted;
        core.caps_lock.problem = Some(reason.clone());
        core.announce_error(&reason);
    }
}

/// Take the steps from the current state to `wanted`. A refused hold leaves Caps Lock
/// behaving as usual and returns the platform's reason.
fn bring_to<P: Platform>(core: &mut Core<P>, wanted: Option<Mods>) -> Result<(), String> {
    for step in steps(core.caps_lock.state, wanted) {
        match step {
            CapsLockStep::UndoLeftOver => {
                if let Some(undo) = core.caps_lock.left_over.take() {
                    core.platform.undo_caps_lock(&undo);
                }
                forget(core);
                core.trace("capsLockLeftOverUndone", || serde_json::json!({}));
            }
            CapsLockStep::Hold(modifiers) => hold(core, modifiers)?,
            CapsLockStep::Restore => {
                core.platform.restore_caps_lock();
                forget(core);
                core.trace("capsLockRestored", || serde_json::json!({}));
            }
        }
    }
    Ok(())
}

fn hold<P: Platform>(core: &mut Core<P>, modifiers: Mods) -> Result<(), String> {
    match core.platform.hold_caps_lock_as(modifiers) {
        Ok(undo) => {
            // Kept before anything else can go wrong, so a crash from here on is undone.
            if write_state(&core.paths.caps_lock(), &Saved { undo }).is_err() {
                core.flag("capsLockUnsaved", || serde_json::json!({}));
            }
            core.caps_lock.state = CapsLockState::Held(modifiers);
            core.caps_lock.refused = None;
            core.caps_lock.problem = None;
            core.trace("capsLockHeld", || serde_json::json!({ "modifiers": modifiers }));
            Ok(())
        }
        Err(reason) => {
            forget(core);
            core.flag(
                "capsLockRefused",
                || serde_json::json!({ "modifiers": modifiers, "reason": reason }),
            );
            Err(reason)
        }
    }
}

/// Caps Lock behaves as usual again, so there is nothing to undo.
fn forget<P: Platform>(core: &mut Core<P>) {
    core.caps_lock.state = CapsLockState::Normal;
    core.caps_lock.press = None;
    let _ = remove_state(&core.paths.caps_lock());
}

/// Turn Caps Lock as the Snappy key on or off. Turning it on while Snappy can apply it
/// tries first, so a refusal leaves the setting off and says why.
pub(crate) fn set_enabled<P: Platform>(core: &mut Core<P>, enabled: bool) -> Result<(), String> {
    if enabled == core.settings.caps_lock_as_snappy_key {
        return Ok(());
    }
    let modifiers = core.settings.snappy_key.modifiers();
    if enabled && core.has_permission() && core.live_control.is_available() {
        bring_to(core, Some(modifiers))?;
    }
    let changed = crate::commands::change(core, |settings| settings.caps_lock_as_snappy_key = enabled);
    // A save that failed after Caps Lock changed puts it back as the settings say.
    configure(core);
    changed?;
    if enabled {
        core.announce("Hold Caps Lock for the Snappy key. Tap it to turn Caps Lock on or off.");
    } else {
        core.announce("Caps Lock works as usual again.");
    }
    Ok(())
}

/// Caps Lock went down or up, or something else was used meanwhile. A press that comes up
/// as a tap turns Caps Lock on or off.
pub(crate) fn input<P: Platform>(core: &mut Core<P>, input: CapsLockInput) {
    if !matches!(core.caps_lock.state, CapsLockState::Held(_)) {
        return;
    }
    let now = core.platform.now();
    match input {
        CapsLockInput::Pressed => core.caps_lock.press = Some(CapsLockPress::new(now)),
        CapsLockInput::OtherInput => note_other_input(core),
        CapsLockInput::Released => {
            let Some(press) = core.caps_lock.press.take() else {
                return;
            };
            let held_ms = (press.held_for(now) * 1000.0).round();
            let other_input = press.other_input();
            match press.release(now) {
                CapsLockRelease::Tap => {
                    core.platform.toggle_caps_lock();
                    core.trace("capsLockTapped", || serde_json::json!({ "heldMs": held_ms }));
                }
                CapsLockRelease::Hold => core.trace(
                    "capsLockReleased",
                    || serde_json::json!({ "heldMs": held_ms, "otherInput": other_input }),
                ),
            }
        }
    }
}

/// A Snappy shortcut fired, so a press of Caps Lock in progress isn't a tap.
pub(crate) fn note_other_input<P: Platform>(core: &mut Core<P>) {
    if let Some(press) = core.caps_lock.press.as_mut() {
        press.note_other_input();
    }
}

/// Give Caps Lock back its usual behavior as Snappy stops.
pub(crate) fn stop<P: Platform>(core: &mut Core<P>) {
    if matches!(core.caps_lock.state, CapsLockState::Held(_)) {
        core.platform.restore_caps_lock();
        forget(core);
    }
}
