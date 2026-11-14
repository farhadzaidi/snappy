//! Caps Lock as the Snappy key through XKB, the keyboard description every Linux desktop
//! builds its keymap from.
//!
//! On Wayland the compositor builds the keymap from XKB rules with libxkbcommon, which looks
//! in the user's XKB folder (`$XDG_CONFIG_HOME/xkb`) before the system's. Snappy adds an
//! option of its own there, `snappy:caps`: its symbols make Caps Lock set the Snappy key's
//! modifiers while held, without repeating, and keep a real Caps Lock on Shift + Caps Lock,
//! which is also the key Snappy presses to turn Caps Lock on or off after a tap. The rules
//! file that names the option includes the system's rules, so every other option and layout
//! works as before. GNOME Shell and KWin each add the option to the keymap themselves.
//!
//! Every adapter, X11's included, refuses when the user's own options already remap Caps
//! Lock (`caps_lock_options`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use snappy_core::keys::{Modifier, Mods};

/// Snappy's option, as desktops list XKB options.
pub const OPTION: &str = "snappy:caps";
/// The symbols file and section the option adds.
const SYMBOLS_FILE: &str = "snappy";
const SYMBOLS_SECTION: &str = "caps";
/// The lines Snappy adds to a rules file sit between these, so it can take them out again
/// and leave the user's own lines as they were.
const BLOCK_START: &str = "// Snappy: Caps Lock as the Snappy key. Removed when it is turned off.";
const BLOCK_END: &str = "// Snappy: end";

/// What Caps Lock reports as its key while held. The modifiers it sets are the same either
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsLockSymbol {
    /// No key at all, so nothing that reacts to a modifier key alone, such as GNOME's
    /// overview on Super, sees one.
    Nothing,
    /// One of the Snappy key's own modifier keys, so that letting go of Caps Lock alone
    /// looks like letting go of those modifiers alone, which KDE's shortcut service
    /// reports as a modifier-only shortcut.
    OneOfTheModifiers,
}

/// The options among `options`, a comma-separated list as XKB writes it, that change what
/// the Caps Lock key does: the `caps` group, such as `caps:escape`, and the others that
/// name Caps Lock as the key they change, such as `ctrl:nocaps`, `ctrl:swapcaps`, and
/// `grp:caps_toggle`. Options that only make other keys toggle Caps Lock, such as
/// `shift:both_capslock`, leave the key alone. Snappy's own option isn't counted.
pub fn caps_lock_options(options: &str) -> Vec<&str> {
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
    group == "caps" || (group != "shift" && name.contains("caps"))
}

/// The XKB real modifiers for `modifiers`: Super is Mod4 and Alt Mod1 in every keymap
/// xkeyboard-config builds.
fn real_modifiers(modifiers: Mods) -> String {
    modifiers
        .in_display_order()
        .map(|modifier| match modifier {
            Modifier::Shift => "Shift",
            Modifier::Ctrl => "Control",
            Modifier::Alt => "Mod1",
            Modifier::Meta => "Mod4",
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The keysym of one of `modifiers`' own keys, Super first, as KDE's shortcut service
/// treats a Super key as Meta.
fn modifier_keysym(modifiers: Mods) -> &'static str {
    [
        (Modifier::Meta, "Super_R"),
        (Modifier::Ctrl, "Control_R"),
        (Modifier::Alt, "Alt_R"),
        (Modifier::Shift, "Shift_R"),
    ]
    .into_iter()
    .find(|(modifier, _)| modifiers.contains(*modifier))
    .map_or("VoidSymbol", |(_, keysym)| keysym)
}

/// The symbols file for `modifiers`. Caps Lock's first level sets them for as long as it
/// is held; its second, with Shift, is a real Caps Lock.
pub fn symbols(modifiers: Mods, symbol: CapsLockSymbol) -> String {
    let first = match symbol {
        CapsLockSymbol::Nothing => "VoidSymbol",
        CapsLockSymbol::OneOfTheModifiers => modifier_keysym(modifiers),
    };
    let held = real_modifiers(modifiers);
    format!(
        "// Caps Lock as the Snappy key, written by Snappy.\n\
         partial modifier_keys\n\
         xkb_symbols \"{SYMBOLS_SECTION}\" {{\n\
         \x20   key <CAPS> {{\n\
         \x20       type[Group1] = \"TWO_LEVEL\",\n\
         \x20       repeat = No,\n\
         \x20       symbols[Group1] = [ {first}, Caps_Lock ],\n\
         \x20       actions[Group1] = [ SetMods(modifiers = {held}), LockMods(modifiers = Lock) ]\n\
         \x20   }};\n\
         }};\n"
    )
}

/// Snappy's lines for a rules file: its option, after the system's rules when the file
/// had none of its own.
fn block(with_system_rules: bool) -> String {
    let include = if with_system_rules {
        "! include %S/evdev\n"
    } else {
        ""
    };
    format!(
        "{BLOCK_START}\n{include}! option = symbols\n  {OPTION} = +{SYMBOLS_FILE}({SYMBOLS_SECTION})\n{BLOCK_END}\n"
    )
}

/// The rules file with Snappy's option: `existing`, the user's own file if any, without an
/// earlier copy of Snappy's lines and with them at the end. A file of Snappy's own
/// includes the system's rules first.
pub fn rules_with_option(existing: Option<&str>) -> String {
    match existing.and_then(rules_without_option) {
        Some(own) => {
            let separator = if own.ends_with('\n') { "" } else { "\n" };
            format!("{own}{separator}{}", block(false))
        }
        None => block(true),
    }
}

/// `existing` without Snappy's lines, or `None` when nothing of the user's is left.
pub fn rules_without_option(existing: &str) -> Option<String> {
    let mut kept = Vec::new();
    let mut inside = false;
    for line in existing.lines() {
        match line.trim() {
            BLOCK_START => inside = true,
            BLOCK_END if inside => inside = false,
            _ if !inside => kept.push(line),
            _ => {}
        }
    }
    let own = kept.join("\n");
    (!own.trim().is_empty()).then(|| format!("{own}\n"))
}

/// The user's XKB folder, which libxkbcommon searches first.
pub fn user_folder() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .map(|config| config.join("xkb"))
}

/// Write Snappy's option into the XKB folder `folder`, Caps Lock holding `modifiers`.
pub fn install(folder: &Path, modifiers: Mods, symbol: CapsLockSymbol) -> io::Result<()> {
    write(
        &folder.join("symbols").join(SYMBOLS_FILE),
        &symbols(modifiers, symbol),
    )?;
    let rules = folder.join("rules").join("evdev");
    let existing = read_if_present(&rules)?;
    write(&rules, &rules_with_option(existing.as_deref()))
}

/// Take Snappy's option out of the XKB folder `folder`, keeping the user's own rules.
pub fn uninstall(folder: &Path) -> io::Result<()> {
    let rules = folder.join("rules").join("evdev");
    if let Some(existing) = read_if_present(&rules)? {
        match rules_without_option(&existing) {
            Some(own) if own != existing => write(&rules, &own)?,
            Some(_) => {}
            None => remove_if_present(&rules)?,
        }
    }
    remove_if_present(&folder.join("symbols").join(SYMBOLS_FILE))
}

fn read_if_present(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Write through a temporary file in the same folder, so a compositor reading the file
/// never sees half of it.
fn write(path: &Path, contents: &str) -> io::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no folder"))?;
    fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(
        ".{}.snappy-tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("xkb")
    ));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snappy_core::keys::SnappyKey;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("snappy-xkb-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn options_that_change_the_caps_lock_key_are_found_and_others_are_not() {
        let options = "grp:alt_shift_toggle,caps:escape, ctrl:nocaps,shift:both_capslock,ctrl:swapcaps,\
                       compose:caps,lv3:ralt_switch,snappy:caps,grp:caps_toggle,shift:breaks_caps";
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
    fn caps_lock_sets_the_snappy_key_and_shift_with_it_is_a_real_caps_lock() {
        let file = symbols(SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::Nothing);
        assert!(file.contains("xkb_symbols \"caps\""), "{file}");
        assert!(file.contains("repeat = No"), "{file}");
        assert!(
            file.contains("symbols[Group1] = [ VoidSymbol, Caps_Lock ]"),
            "{file}"
        );
        assert!(
            file.contains(
                "actions[Group1] = [ SetMods(modifiers = Control+Mod1+Mod4), LockMods(modifiers = Lock) ]"
            ),
            "{file}"
        );
    }

    #[test]
    fn caps_lock_can_report_one_of_its_own_modifiers() {
        let default = symbols(SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::OneOfTheModifiers);
        assert!(default.contains("[ Super_R, Caps_Lock ]"), "{default}");
        let control_shift = Mods::from_iter([Modifier::Ctrl, Modifier::Shift]);
        let file = symbols(control_shift, CapsLockSymbol::OneOfTheModifiers);
        assert!(file.contains("[ Control_R, Caps_Lock ]"), "{file}");
        assert!(file.contains("SetMods(modifiers = Control+Shift)"), "{file}");
    }

    #[test]
    fn a_rules_file_of_snappys_own_includes_the_system_rules_first() {
        let rules = rules_with_option(None);
        let include = rules.find("! include %S/evdev").unwrap();
        let option = rules.find("snappy:caps = +snappy(caps)").unwrap();
        assert!(include < option, "{rules}");
        assert_eq!(rules_without_option(&rules), None);
    }

    #[test]
    fn the_users_own_rules_are_kept_around_snappys_lines() {
        let own = "! include %S/evdev\n! option = symbols\n  custom:foo = +custom(foo)\n";
        let with = rules_with_option(Some(own));
        assert!(with.starts_with(own), "{with}");
        assert_eq!(with.matches("! include").count(), 1, "{with}");
        assert_eq!(rules_with_option(Some(&with)), with, "adding twice adds once");
        assert_eq!(rules_without_option(&with).as_deref(), Some(own));
    }

    #[test]
    fn installing_and_uninstalling_leave_the_folder_as_it_was() {
        let folder = scratch("fresh");
        install(&folder, SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::Nothing).unwrap();
        assert!(folder.join("symbols/snappy").exists());
        assert!(
            fs::read_to_string(folder.join("rules/evdev"))
                .unwrap()
                .contains(OPTION)
        );
        uninstall(&folder).unwrap();
        assert!(!folder.join("symbols/snappy").exists());
        assert!(!folder.join("rules/evdev").exists());

        let folder = scratch("own-rules");
        let own = "! include %S/evdev\n// mine\n";
        fs::create_dir_all(folder.join("rules")).unwrap();
        fs::write(folder.join("rules/evdev"), own).unwrap();
        install(&folder, SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::Nothing).unwrap();
        uninstall(&folder).unwrap();
        assert_eq!(fs::read_to_string(folder.join("rules/evdev")).unwrap(), own);
        uninstall(&folder).unwrap();
        let _ = fs::remove_dir_all(&folder);
    }
}
