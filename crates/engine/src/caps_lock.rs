//! Caps Lock as the Snappy key: bringing Caps Lock to what the settings ask, keeping what
//! undoes the change in a state file while it is in place, so the next run can undo it
//! after a crash, and deciding what each press of Caps Lock was.

use serde::{Deserialize, Serialize};
use snappy_core::keys::Mods;
use snappy_core::keys::caps_lock::{CapsLockPress, CapsLockRelease, CapsLockState, CapsLockStep, steps};
use snappy_platform::{CapsLockInput, CapsLockRefusal, CapsLockUndo, Platform};

use crate::diagnostics::millis;
use crate::log_event::LogEvent;
use crate::state::EngineState;
use crate::store::{StateFile, remove_state, write_state};

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

impl StateFile for Saved {
    const VERSION: u32 = 1;

    /// Before state files had a version, an adapter's record was kept as JSON text; it is
    /// read as the value it holds.
    fn unversioned(value: serde_json::Value) -> Option<Self> {
        let mut saved: Saved = serde_json::from_value(value).ok()?;
        if let serde_json::Value::String(text) = &saved.undo.0
            && let Ok(record) = serde_json::from_str(text)
        {
            saved.undo.0 = record;
        }
        Some(saved)
    }
}

/// Note a change an earlier run left in place, from its state file, so that the first
/// `configure` undoes it.
pub(crate) fn start<P: Platform>(core: &mut EngineState<P>) {
    let path = core.paths.caps_lock();
    if let Some(saved) = core.read_state::<Saved>(&path).read() {
        core.caps_lock.state = CapsLockState::LeftOver;
        core.caps_lock.left_over = Some(saved.undo);
    }
}

/// Caps Lock stands for the Snappy key while the setting asks for it and Snappy can
/// control the desktop, which on macOS needs the permission that taps keys. In a build that
/// can't hold it, a setting that asks anyway, as one imported from another build does, is
/// treated as off without trying: the app already shows why.
fn wanted<P: Platform>(core: &EngineState<P>) -> Option<Mods> {
    (core.settings.caps_lock_as_snappy_key
        && core.capabilities.caps_lock_unavailable.is_none()
        && core.has_permission()
        && core.live_control.is_available())
    .then(|| core.settings.snappy_key.modifiers())
}

/// Bring Caps Lock to what the settings and the platform's state ask, announcing a
/// refusal once.
pub(crate) fn configure<P: Platform>(core: &mut EngineState<P>) {
    let wanted = wanted(core);
    if wanted.is_none() {
        core.caps_lock.refused = None;
        core.caps_lock.problem = None;
    } else if wanted == core.caps_lock.refused {
        return;
    }
    if let Err(refusal) = bring_to(core, wanted) {
        note_refusal(core, wanted, &refusal);
    }
}

/// Keep why Caps Lock isn't the Snappy key, shown beside the setting until it is. A refusal
/// is announced; one that only waits for the next login is the setting's own notice.
fn note_refusal<P: Platform>(core: &mut EngineState<P>, wanted: Option<Mods>, refusal: &CapsLockRefusal) {
    core.caps_lock.refused = wanted;
    core.caps_lock.problem = Some(refusal.message().to_owned());
    match refusal {
        CapsLockRefusal::Refused(reason) => core.announce_error(reason),
        CapsLockRefusal::AfterLogin(_) => core.publish_state(),
    }
}

/// Take the steps from the current state to `wanted`. A refused hold leaves Caps Lock
/// behaving as usual and returns the platform's reason.
fn bring_to<P: Platform>(core: &mut EngineState<P>, wanted: Option<Mods>) -> Result<(), CapsLockRefusal> {
    for step in steps(core.caps_lock.state, wanted) {
        match step {
            CapsLockStep::UndoLeftOver => {
                if let Some(undo) = core.caps_lock.left_over.take() {
                    core.platform.undo_caps_lock(&undo);
                }
                forget(core);
                core.trace(LogEvent::CapsLockLeftOverUndone, || serde_json::json!({}));
            }
            CapsLockStep::Hold(modifiers) => hold(core, modifiers)?,
            CapsLockStep::Restore => {
                core.platform.restore_caps_lock();
                forget(core);
                core.trace(LogEvent::CapsLockRestored, || serde_json::json!({}));
            }
        }
    }
    Ok(())
}

fn hold<P: Platform>(core: &mut EngineState<P>, modifiers: Mods) -> Result<(), CapsLockRefusal> {
    match core.platform.hold_caps_lock_as(modifiers) {
        Ok(undo) => {
            // Kept before anything else can go wrong, so a crash from here on is undone.
            if write_state(&core.paths.caps_lock(), &Saved { undo }).is_err() {
                core.flag(LogEvent::CapsLockUnsaved, || serde_json::json!({}));
            }
            core.caps_lock.state = CapsLockState::Held(modifiers);
            core.caps_lock.refused = None;
            core.caps_lock.problem = None;
            core.trace(
                LogEvent::CapsLockHeld,
                || serde_json::json!({ "modifiers": modifiers }),
            );
            Ok(())
        }
        Err(refusal) => {
            forget(core);
            let after_login = matches!(refusal, CapsLockRefusal::AfterLogin(_));
            core.flag(LogEvent::CapsLockRefused, || {
                serde_json::json!({
                    "modifiers": modifiers,
                    "reason": refusal.message(),
                    "afterLogin": after_login,
                })
            });
            Err(refusal)
        }
    }
}

/// Caps Lock behaves as usual again, so there is nothing to undo.
fn forget<P: Platform>(core: &mut EngineState<P>) {
    core.caps_lock.state = CapsLockState::Normal;
    core.caps_lock.press = None;
    let _ = remove_state(&core.paths.caps_lock());
}

/// Turn Caps Lock as the Snappy key on or off. Turning it on while Snappy can apply it
/// tries first, so a refusal leaves the setting off and says why. Where the build can't hold
/// Caps Lock at all, turning it on is refused with the platform's reason.
pub(crate) fn set_enabled<P: Platform>(core: &mut EngineState<P>, enabled: bool) -> Result<(), String> {
    if enabled == core.settings.caps_lock_as_snappy_key {
        return Ok(());
    }
    if enabled && let Some(reason) = core.capabilities.caps_lock_unavailable {
        return Err(reason.to_owned());
    }
    let modifiers = core.settings.snappy_key.modifiers();
    let mut waits_for_login = None;
    if enabled && core.has_permission() && core.live_control.is_available() {
        match bring_to(core, Some(modifiers)) {
            Ok(()) => {}
            // It can be, from the next login: the setting goes on and says so until then.
            Err(refusal @ CapsLockRefusal::AfterLogin(_)) => waits_for_login = Some(refusal),
            Err(CapsLockRefusal::Refused(reason)) => return Err(reason),
        }
    }
    let changed = crate::commands::change(core, |settings| settings.caps_lock_as_snappy_key = enabled);
    if let Some(refusal) = &waits_for_login {
        note_refusal(core, Some(modifiers), refusal);
    }
    // A save that failed after Caps Lock changed puts it back as the settings say.
    configure(core);
    changed?;
    if waits_for_login.is_some() {
        return Ok(());
    }
    if enabled {
        core.announce("Hold Caps Lock for the Snappy key. Tap it to turn Caps Lock on or off.");
    } else {
        core.announce("Caps Lock works as usual again.");
    }
    Ok(())
}

/// Caps Lock went down or up, or something else was used meanwhile. A press that comes up
/// as a tap turns Caps Lock on or off.
pub(crate) fn input<P: Platform>(core: &mut EngineState<P>, input: CapsLockInput) {
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
            let held_ms = millis(press.held_for(now)).round();
            let other_input = press.other_input();
            match press.release(now) {
                CapsLockRelease::Tap => match core.platform.toggle_caps_lock() {
                    Ok(()) => core.trace(
                        LogEvent::CapsLockTapped,
                        || serde_json::json!({ "heldMs": held_ms }),
                    ),
                    Err(reason) => core.flag(
                        LogEvent::CapsLockTapFailed,
                        || serde_json::json!({ "heldMs": held_ms, "reason": reason }),
                    ),
                },
                CapsLockRelease::Hold => core.trace(
                    LogEvent::CapsLockReleased,
                    || serde_json::json!({ "heldMs": held_ms, "otherInput": other_input }),
                ),
            }
        }
    }
}

/// The platform let go of Caps Lock because something else changed the keyboard settings.
/// Nothing of Snappy's is left to undo. The settings still ask for it, so Caps Lock is held
/// again, quietly, as each layout switch that reloads the keymap needs; when the new
/// keyboard settings remap Caps Lock themselves, theirs win, and the refusal is announced
/// and shown with the setting as any refusal is.
pub(crate) fn lost<P: Platform>(core: &mut EngineState<P>) {
    if !matches!(core.caps_lock.state, CapsLockState::Held(_)) {
        return;
    }
    core.flag(LogEvent::CapsLockLost, || serde_json::json!({}));
    forget(core);
    configure(core);
    core.publish_state();
}

/// A Snappy shortcut fired, so a press of Caps Lock in progress isn't a tap.
pub(crate) fn note_other_input<P: Platform>(core: &mut EngineState<P>) {
    if let Some(press) = core.caps_lock.press.as_mut() {
        press.note_other_input();
    }
}

/// Give Caps Lock back its usual behavior as Snappy stops.
pub(crate) fn stop<P: Platform>(core: &mut EngineState<P>) {
    if matches!(core.caps_lock.state, CapsLockState::Held(_)) {
        core.platform.restore_caps_lock();
        forget(core);
    }
}
