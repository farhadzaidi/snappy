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
//! Lock, in the words of `options_refusal`. On Wayland the keymap the compositor built is
//! read too (`layout_refusal`), so a layout that makes Caps Lock another key is refused as
//! well; X11 reads the key's definition from the server itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use snappy_core::keys::{Modifier, Mods};

use crate::{journal, wayland};

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

/// The refusal when the keyboard layout itself makes Caps Lock another key.
pub const LAYOUT_REMAPS_CAPS_LOCK: &str = "Caps Lock is already remapped by the keyboard layout. Choose a layout that \
     keeps Caps Lock to use Caps Lock as the Snappy key.";

/// Caps Lock's evdev keycode, as XKB numbers keys: the kernel's 58 plus 8.
const CAPS_LOCK_KEYCODE: u32 = 66;
/// What the first level of Snappy's own definition of Caps Lock can report, whichever
/// `CapsLockSymbol` an adapter chose.
const SNAPPYS_FIRST_LEVEL: [&str; 5] = ["VoidSymbol", "Super_R", "Control_R", "Alt_R", "Shift_R"];

/// What a keymap makes of the Caps Lock key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsLockKey {
    /// Caps Lock, in every layout.
    Plain,
    /// Snappy's own option is in the keymap, and every other layout keeps Caps Lock.
    Snappys,
    /// At least one layout makes it another key.
    Remapped,
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

/// What `keymap`, XKB keymap text as compositors hand it to clients, makes of the Caps
/// Lock key; `None` when it doesn't define the key.
pub fn caps_lock_key(keymap: &str) -> Option<CapsLockKey> {
    let names = key_names(keymap, CAPS_LOCK_KEYCODE);
    let symbols = &keymap[keymap.find("xkb_symbols")?..];
    let body = names.iter().find_map(|name| key_body(symbols, name))?;
    let groups = symbol_lists(body);
    let Some((first, rest)) = groups.split_first() else {
        return Some(CapsLockKey::Remapped);
    };
    let others_plain = rest.iter().all(|group| is_plain(group));
    Some(if is_plain(first) && others_plain {
        CapsLockKey::Plain
    } else if is_snappys(first) && others_plain {
        CapsLockKey::Snappys
    } else {
        CapsLockKey::Remapped
    })
}

/// Whether a group's symbols start with a Caps Lock.
fn is_plain(group: &[&str]) -> bool {
    group.first() == Some(&"Caps_Lock")
}

/// Whether a group's symbols are those of Snappy's option: one of the symbols it may report,
/// then a real Caps Lock with Shift.
fn is_snappys(group: &[&str]) -> bool {
    matches!(group, [first, "Caps_Lock"] if SNAPPYS_FIRST_LEVEL.contains(first))
}

/// The names the keymap's `xkb_keycodes` section gives `keycode`, aliases included, such as
/// `CAPS`.
fn key_names(keymap: &str, keycode: u32) -> Vec<&str> {
    let section = keymap
        .find("xkb_keycodes")
        .map_or("", |start| &keymap[start..])
        .split("xkb_types")
        .next()
        .unwrap_or_default();
    let statements = || section.split(';').map(str::trim);
    let mut names: Vec<&str> = statements()
        .filter_map(|statement| {
            let (name, value) = statement.split_once('=')?;
            let name = name.trim().strip_prefix('<')?.strip_suffix('>')?;
            (value.trim().parse::<u32>().ok()? == keycode).then_some(name)
        })
        .collect();
    let aliases: Vec<&str> = statements()
        .filter_map(|statement| {
            let (alias, real) = statement.strip_prefix("alias")?.split_once('=')?;
            let real = real.trim().strip_prefix('<')?.strip_suffix('>')?;
            names
                .contains(&real)
                .then(|| alias.trim().strip_prefix('<')?.strip_suffix('>'))
                .flatten()
        })
        .collect();
    names.extend(aliases);
    names
}

/// The text between the braces of `key <name> { ... }` in a symbols section.
fn key_body<'a>(symbols: &'a str, name: &str) -> Option<&'a str> {
    let declaration = format!("<{name}>");
    let mut rest = symbols;
    loop {
        let at = rest.find("key")?;
        let after = rest[at + 3..].trim_start();
        let is_word = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric() && before != '_');
        if is_word && let Some(definition) = after.strip_prefix(&declaration) {
            let open = definition.find('{')?;
            let close = definition[open..].find('}')? + open;
            return Some(&definition[open + 1..close]);
        }
        rest = &rest[at + 3..];
    }
}

/// The symbol lists of a key's body, one per group: `symbols[Group1]= [ ... ]` entries, or
/// lists written bare, as `{ [ Caps_Lock ] }`. Lists of actions and the like are left out.
fn symbol_lists(body: &str) -> Vec<Vec<&str>> {
    let mut lists = Vec::new();
    let mut at = 0;
    while let Some(open) = body[at..].find('[').map(|offset| at + offset) {
        let Some(close) = body[open..].find(']').map(|offset| open + offset) else {
            break;
        };
        let before = body[..open].trim_end();
        let is_symbols = match before.chars().next_back() {
            Some('{' | ',') | None => true,
            Some('=') => {
                let field = before[..before.len() - 1].trim_end();
                let field = match field.strip_suffix(']') {
                    Some(indexed) => indexed.rsplit_once('[').map_or(indexed, |(name, _)| name),
                    None => field,
                };
                field
                    .rsplit(|character: char| !character.is_alphanumeric() && character != '_')
                    .next()
                    == Some("symbols")
            }
            _ => false,
        };
        if is_symbols {
            lists.push(
                body[open + 1..close]
                    .split(',')
                    .map(str::trim)
                    .filter(|symbol| !symbol.is_empty())
                    .collect(),
            );
        }
        at = close + 1;
    }
    lists
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

/// Take Snappy's option out of the XKB folder `folder`, keeping the user's own rules. The
/// folders that hold nothing more afterwards go too, `folder` included, as `install` may
/// have made them.
pub fn uninstall(folder: &Path) -> io::Result<()> {
    let rules = folder.join("rules").join("evdev");
    if let Some(existing) = read_if_present(&rules)? {
        match rules_without_option(&existing) {
            Some(own) if own != existing => write(&rules, &own)?,
            Some(_) => {}
            None => remove_if_present(&rules)?,
        }
    }
    remove_if_present(&folder.join("symbols").join(SYMBOLS_FILE))?;
    for empty in [folder.join("rules"), folder.join("symbols"), folder.to_owned()] {
        remove_if_empty(&empty)?;
    }
    Ok(())
}

fn read_if_present(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Removes the folder at `path` when it holds nothing; one that holds something, or none at
/// all, stays as it is.
fn remove_if_empty(path: &Path) -> io::Result<()> {
    match fs::remove_dir(path) {
        Err(error)
            if !matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Err(error)
        }
        _ => Ok(()),
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

    /// A keymap as libxkbcommon writes one for clients, with `caps` as Caps Lock's body.
    fn keymap(caps: &str) -> String {
        format!(
            "xkb_keymap {{\n\
             xkb_keycodes \"evdev+aliases(qwerty)\" {{\n\
             \tminimum = 8;\n\tmaximum = 255;\n\t<ESC> = 9;\n\t<CAPS> = 66;\n\t<AC01> = 38;\n\
             \talias <AB00> = <TLDE>;\n\tindicator 1 = \"Caps Lock\";\n}};\n\n\
             xkb_types \"complete\" {{\n\ttype \"TWO_LEVEL\" {{\n\t\tmodifiers= Shift;\n\
             \t\tmap[Shift]= Level2;\n\t\tlevel_name[Level1]= \"Base\";\n\t}};\n}};\n\n\
             xkb_compatibility \"complete\" {{\n\tinterpret Caps_Lock {{\n\
             \t\taction= LockMods(modifiers=Lock);\n\t}};\n}};\n\n\
             xkb_symbols \"pc+us+inet(evdev)\" {{\n\tname[Group1]=\"English (US)\";\n\
             \tkey <ESC>                {{\t[          Escape ] }};\n\
             \tkey <CAPS>               {{{caps}}};\n\
             \tkey <AC01>               {{\t[               a,               A ] }};\n\
             \tmodifier_map Lock {{ <CAPS> }};\n}};\n\n}};\n"
        )
    }

    #[test]
    fn a_plain_caps_lock_in_every_layout_is_plain() {
        assert_eq!(
            caps_lock_key(&keymap("\t[       Caps_Lock ] ")),
            Some(CapsLockKey::Plain)
        );
        assert_eq!(
            caps_lock_key(&keymap(
                "\n\t\tsymbols[1]= [ Caps_Lock ],\n\t\tsymbols[2]= [ Caps_Lock, Caps_Lock ]\n\t"
            )),
            Some(CapsLockKey::Plain)
        );
    }

    #[test]
    fn a_layout_that_makes_caps_lock_another_key_remaps_it_in_any_group() {
        let colemak = "\t[       BackSpace,       BackSpace,       BackSpace,       BackSpace ] ";
        assert_eq!(caps_lock_key(&keymap(colemak)), Some(CapsLockKey::Remapped));
        assert_eq!(
            caps_lock_key(&keymap(
                "\n\t\tsymbols[Group1]= [ Caps_Lock ],\n\t\tsymbols[Group2]= [ Control_L ]\n\t"
            )),
            Some(CapsLockKey::Remapped)
        );
        assert_eq!(
            caps_lock_key(&keymap("\n\t\tactions[1]= [ SetMods(modifiers=Control) ]\n\t")),
            Some(CapsLockKey::Remapped),
            "no symbol at all"
        );
    }

    #[test]
    fn snappys_own_option_in_the_keymap_is_told_apart_from_a_remapping() {
        for first in ["VoidSymbol", "Super_R"] {
            let caps = format!(
                "\n\t\ttype= \"TWO_LEVEL\",\n\t\trepeat= No,\n\
                 \t\tsymbols[Group1]= [ {first}, Caps_Lock ],\n\
                 \t\tactions[Group1]= [ SetMods(modifiers=Control+Mod1+Mod4), LockMods(modifiers=Lock) ]\n\t"
            );
            assert_eq!(
                caps_lock_key(&keymap(&caps)),
                Some(CapsLockKey::Snappys),
                "{first}"
            );
        }
        assert_eq!(
            caps_lock_key(&keymap(
                "\n\t\tsymbols[1]= [ VoidSymbol, Caps_Lock ],\n\t\tsymbols[2]= [ BackSpace ]\n\t"
            )),
            Some(CapsLockKey::Remapped),
            "another layout still remaps it"
        );
    }

    #[test]
    fn caps_lock_is_found_by_its_keycode_or_an_alias_of_it() {
        let renamed = keymap("\t[ Caps_Lock ] ")
            .replace("<CAPS> = 66;", "<CAPS> = 66;\n\talias <LOCK> = <CAPS>;")
            .replace("key <CAPS>", "key <LOCK>");
        assert_eq!(caps_lock_key(&renamed), Some(CapsLockKey::Plain));
        let elsewhere = keymap("\t[ Escape ] ").replace("<CAPS> = 66;", "<CAPS> = 67;");
        assert_eq!(caps_lock_key(&elsewhere), None, "keycode 66 has no key");
        assert_eq!(caps_lock_key(""), None);
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
        assert!(!folder.exists(), "the folders install made go with the files");

        let folder = scratch("own-rules");
        let own = "! include %S/evdev\n// mine\n";
        fs::create_dir_all(folder.join("rules")).unwrap();
        fs::write(folder.join("rules/evdev"), own).unwrap();
        install(&folder, SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::Nothing).unwrap();
        uninstall(&folder).unwrap();
        assert_eq!(fs::read_to_string(folder.join("rules/evdev")).unwrap(), own);
        assert!(!folder.join("symbols").exists());
        uninstall(&folder).unwrap();
        let _ = fs::remove_dir_all(&folder);

        let folder = scratch("own-symbols");
        fs::create_dir_all(folder.join("symbols")).unwrap();
        fs::write(folder.join("symbols/mine"), "").unwrap();
        install(&folder, SnappyKey::DEFAULT.modifiers(), CapsLockSymbol::Nothing).unwrap();
        uninstall(&folder).unwrap();
        assert!(folder.join("symbols/mine").exists());
        assert!(!folder.join("rules").exists());
        let _ = fs::remove_dir_all(&folder);
    }
}
