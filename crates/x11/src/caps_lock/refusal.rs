//! Whether the session's keymap leaves Caps Lock free to be the Snappy key.

use x11rb::protocol::xproto::AtomEnum;

use super::key_definition::KeyDefinition;
use crate::messages;
use crate::shared::Shared;
use crate::xserver;

/// Why Caps Lock can't be the Snappy key: one of `options` remaps it, or `caps` isn't a
/// plain Caps Lock, as after `xmodmap`.
pub(super) fn refusal(options: &str, caps: Option<&KeyDefinition>) -> Option<String> {
    if let Some(reason) = snappy_linux::xkb::options_refusal(options) {
        return Some(reason);
    }
    (!caps.is_some_and(KeyDefinition::is_plain_caps_lock))
        .then(|| messages::CAPS_LOCK_OTHER_SETTING.to_owned())
}

/// The XKB options the session's keymap was built with.
pub(super) fn options(shared: &Shared) -> String {
    xserver::bytes(
        &shared.conn,
        shared.screen.root,
        shared.atoms._XKB_RULES_NAMES,
        AtomEnum::STRING,
    )
    .map(|names| {
        // Rules, model, layouts, variants, options, each ended by a zero byte.
        names
            .split(|byte| *byte == 0)
            .nth(4)
            .map(|options| String::from_utf8_lossy(options).into_owned())
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps_lock::key_definition::plain_caps_lock;

    #[test]
    fn a_caps_lock_option_or_another_remapping_is_refused_naming_it() {
        let caps = plain_caps_lock();
        assert_eq!(refusal("grp:alt_shift_toggle", Some(&caps)), None);
        assert_eq!(
            refusal("ctrl:nocaps,compose:caps", Some(&caps)),
            snappy_linux::xkb::options_refusal("ctrl:nocaps,compose:caps"),
            "the options are named first, even for a key that is still plain"
        );
        let mut xmodmapped = caps.clone();
        xmodmapped.syms = vec![0xffe3];
        assert_eq!(
            refusal("", Some(&xmodmapped)).as_deref(),
            Some(messages::CAPS_LOCK_OTHER_SETTING)
        );
        let mut not_a_lock = caps;
        not_a_lock.modmap = 0;
        assert!(refusal("", Some(&not_a_lock)).is_some());
        assert!(refusal("", None).is_some(), "no Caps Lock key at all");
    }
}
