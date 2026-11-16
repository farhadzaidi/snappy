//! Why Caps Lock can't be the Snappy key: keyboard options or a layout that already remap it.

use super::keymap::{CapsLockKey, caps_lock_key};
use super::option::OPTION;
use crate::messages::LAYOUT_REMAPS_CAPS_LOCK;
use crate::{journal, wayland};

/// Why Caps Lock can't be the Snappy key when `options`, a comma-separated list as XKB
/// writes it, already remap it: a sentence for the user naming each option that does.
/// `None` when none does.
pub fn options_refusal(options: &str) -> Option<String> {
    let named = match caps_lock_options(options).as_slice() {
        [] => return None,
        [only] => format!("the keyboard option {only}. Remove it"),
        [first @ .., last] => format!(
            "the keyboard options {} and {last}. Remove them",
            first.join(", ")
        ),
    };
    Some(format!(
        "Caps Lock is already remapped by {named} to use Caps Lock as the Snappy key."
    ))
}

/// Why Caps Lock can't be the Snappy key in this Wayland session: the keymap its compositor
/// built makes Caps Lock another key, as some layouts do, such as Colemak. A keymap that
/// can't be read is taken as no reason, with a warning.
pub fn layout_refusal() -> Option<&'static str> {
    match wayland::keymap() {
        Ok(keymap) => {
            (caps_lock_key(&keymap) == Some(CapsLockKey::Remapped)).then_some(LAYOUT_REMAPS_CAPS_LOCK)
        }
        Err(error) => {
            journal::warn(format_args!(
                "can't read the keymap to check the keyboard layout keeps Caps Lock: {error}"
            ));
            None
        }
    }
}

/// The options among `options`, a comma-separated list as XKB writes it, that change what
/// the Caps Lock key does: the `caps` group, such as `caps:escape`, and the others that
/// name Caps Lock as the key they change, such as `ctrl:nocaps`, `ctrl:swapcaps`, and
/// `grp:caps_toggle`. Options that only make other keys toggle Caps Lock, such as
/// `shift:both_capslock`, or only light Caps Lock's light, such as `grp_led:caps`, leave the
/// key alone. Snappy's own option isn't counted.
fn caps_lock_options(options: &str) -> Vec<&str> {
    options
        .split(',')
        .map(str::trim)
        .filter(|option| *option != OPTION && changes_the_caps_lock_key(option))
        .collect()
}

fn changes_the_caps_lock_key(option: &str) -> bool {
    let Some((group, name)) = option.split_once(':') else {
        return false;
    };
    group == "caps" || (!matches!(group, "shift" | "grp_led") && name.contains("caps"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_that_change_the_caps_lock_key_are_found_and_others_are_not() {
        let options = "grp:alt_shift_toggle,caps:escape, ctrl:nocaps,shift:both_capslock,ctrl:swapcaps,\
                       compose:caps,lv3:ralt_switch,snappy:caps,grp:caps_toggle,shift:breaks_caps,\
                       grp_led:caps";
        assert_eq!(
            caps_lock_options(options),
            [
                "caps:escape",
                "ctrl:nocaps",
                "ctrl:swapcaps",
                "compose:caps",
                "grp:caps_toggle"
            ]
        );
        assert!(caps_lock_options("").is_empty());
        assert!(caps_lock_options("terminate:ctrl_alt_bksp").is_empty());
    }

    #[test]
    fn a_refusal_names_each_option_that_remaps_caps_lock() {
        assert_eq!(options_refusal("grp:alt_shift_toggle,snappy:caps"), None);
        assert_eq!(
            options_refusal("caps:escape").as_deref(),
            Some(
                "Caps Lock is already remapped by the keyboard option caps:escape. Remove it to use Caps Lock as the Snappy key."
            )
        );
        assert_eq!(
            options_refusal("ctrl:nocaps,lv3:ralt_switch,caps:escape").as_deref(),
            Some(
                "Caps Lock is already remapped by the keyboard options ctrl:nocaps and caps:escape. Remove them to use Caps Lock as the Snappy key."
            )
        );
        assert!(
            options_refusal("ctrl:nocaps,caps:escape,grp:caps_toggle")
                .unwrap()
                .contains("the keyboard options ctrl:nocaps, caps:escape and grp:caps_toggle. Remove them")
        );
    }
}
