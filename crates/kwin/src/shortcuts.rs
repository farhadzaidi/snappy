//! Snappy's shortcuts with KDE's shortcut service, as a component of its own, so System
//! Settings lists them under Snappy. A key another component holds is refused, and the same
//! question is the availability probe; Snappy's own component never counts, whatever an
//! earlier run left registered. A press arrives as a signal on the component.

use crate::bus::{Bus, CallError};
use crate::shared::Grab;

/// The component's name for the service, and as System Settings shows it.
pub const COMPONENT: &str = "snappy";
const COMPONENT_NAME: &str = "Snappy";
/// Where the service reports the component's shortcuts being pressed.
pub const COMPONENT_PATH: &str = "/component/snappy";

/// KGlobalAccel's flags: make the action present, and use these keys rather than ones
/// the user saved for it earlier, since Snappy's settings are where shortcuts are changed.
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;

const ACTION_PREFIX: &str = "shortcut-";

/// The shortcut ID an action stands for.
pub fn shortcut_id(action: &str) -> Option<u32> {
    action.strip_prefix(ACTION_PREFIX)?.parse().ok()
}

fn action_id(id: u32) -> [String; 4] {
    [
        COMPONENT.to_owned(),
        format!("{ACTION_PREFIX}{id}"),
        COMPONENT_NAME.to_owned(),
        format!("Snappy shortcut {id}"),
    ]
}

/// Holds `wanted`, `(id, key)` pairs, dropping whatever `previous` held that isn't wanted.
/// Returns what is held now, and the keys the service refused.
pub fn register(bus: &Bus, previous: &[Grab], wanted: &[(u32, i32)]) -> (Vec<Grab>, Vec<i32>) {
    let stale: Vec<Grab> = previous
        .iter()
        .filter(|grab| !wanted.contains(&(grab.id, grab.key)))
        .cloned()
        .collect();
    unregister(bus, &stale);

    let mut held = Vec::new();
    let mut refused = Vec::new();
    for &(id, key) in wanted {
        match grab(bus, id, key) {
            Ok(Some(grab)) => held.push(grab),
            Ok(None) => refused.push(key),
            Err(error) => {
                eprintln!("snappy: KDE's shortcut service didn't answer: {error:?}");
                refused.push(key);
            }
        }
    }
    (held, refused)
}

/// Assigns the key to the shortcut's action; `None` when another component holds it. The
/// service would give a held key to a second action too, so it is asked who holds it first.
/// Its own `globalShortcutAvailable` counts Snappy's own actions as taken, so it isn't used.
fn grab(bus: &Bus, id: u32, key: i32) -> Result<Option<Grab>, CallError> {
    let action = action_id(id);
    let holders = bus.key_holders(vec![key])?;
    if held_elsewhere(&holders) {
        return Ok(None);
    }
    // The key moved from another of Snappy's shortcuts, or an earlier run left it behind.
    for (_, holder) in holders.into_iter().filter(|(_, holder)| *holder != action[1]) {
        bus.unregister_action(COMPONENT, holder)?;
    }
    bus.register_action(action.clone())?;
    let given = bus.set_action_keys(action.clone(), vec![key], SET_PRESENT | NO_AUTOLOADING)?;
    if given.contains(&key) {
        return Ok(Some(Grab {
            id,
            key,
            action: action[1].clone(),
        }));
    }
    let _ = bus.unregister_action(COMPONENT, action[1].clone());
    Ok(None)
}

/// Removes the actions entirely, so none is left behind in System Settings.
pub fn unregister(bus: &Bus, grabs: &[Grab]) {
    for grab in grabs {
        let _ = bus.unregister_action(COMPONENT, grab.action.clone());
    }
}

/// Whether no other component holds the key; Snappy's own actions count as free.
pub fn is_free(bus: &Bus, key: i32) -> bool {
    bus.key_holders(vec![key])
        .is_ok_and(|holders| !held_elsewhere(&holders))
}

/// Whether any `(component, action)` holding a key is another component's.
fn held_elsewhere(holders: &[(String, String)]) -> bool {
    holders.iter().any(|(component, _)| component != COMPONENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_another_components_actions_hold_a_key() {
        let snappy = |action: &str| (COMPONENT.to_owned(), action.to_owned());
        assert!(!held_elsewhere(&[]));
        assert!(!held_elsewhere(&[snappy("shortcut-3"), snappy("shortcut-9")]));
        assert!(held_elsewhere(&[
            snappy("shortcut-3"),
            ("kwin".to_owned(), "Window Close".to_owned())
        ]));
    }

    #[test]
    fn actions_name_the_shortcut_they_stand_for() {
        let action = action_id(12);
        assert_eq!(action[0], COMPONENT);
        assert_eq!(shortcut_id(&action[1]), Some(12));
        assert_eq!(shortcut_id("something-else"), None);
        assert_eq!(COMPONENT_PATH, format!("/component/{COMPONENT}"));
    }
}
