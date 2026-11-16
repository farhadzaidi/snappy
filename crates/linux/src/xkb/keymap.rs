//! What a keymap, as compositors hand it to clients, makes of the Caps Lock key.

use crate::key_codes::{self, keysym};

/// What the first level of Snappy's own definition of Caps Lock can report, whichever
/// `CapsLockSymbol` an adapter chose.
const SNAPPYS_FIRST_LEVEL: [&str; 5] = ["VoidSymbol", "Super_R", "Control_R", "Alt_R", "Shift_R"];
const CAPS_LOCK: &str = "Caps_Lock";
/// The keysyms the checks look for, with their values. A keymap may write a keysym as its
/// value, `0xffe5` for `Caps_Lock`, as KWin's does.
const KEYSYMS: [(&str, u32); 6] = [
    (CAPS_LOCK, keysym::CAPS_LOCK),
    ("VoidSymbol", keysym::VOID_SYMBOL),
    ("Super_R", keysym::SUPER_R),
    ("Control_R", keysym::CONTROL_R),
    ("Alt_R", keysym::ALT_R),
    ("Shift_R", keysym::SHIFT_R),
];

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

/// What `keymap`, XKB keymap text as compositors hand it to clients, makes of the Caps
/// Lock key; `None` when it doesn't define the key.
pub fn caps_lock_key(keymap: &str) -> Option<CapsLockKey> {
    let names = key_names(keymap, u32::from(key_codes::CAPS_LOCK));
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

/// Whether a keymap's `symbol`, a name or a value, is the keysym `name`.
fn is_keysym(symbol: &str, name: &str) -> bool {
    symbol == name
        || symbol
            .strip_prefix("0x")
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .is_some_and(|value| KEYSYMS.contains(&(name, value)))
}

/// Whether a group's symbols start with a Caps Lock.
fn is_plain(group: &[&str]) -> bool {
    group.first().is_some_and(|first| is_keysym(first, CAPS_LOCK))
}

/// Whether a group's symbols are those of Snappy's option: one of the symbols it may report,
/// then a real Caps Lock with Shift.
fn is_snappys(group: &[&str]) -> bool {
    matches!(group, [first, second]
        if is_keysym(second, CAPS_LOCK) && SNAPPYS_FIRST_LEVEL.iter().any(|name| is_keysym(first, name)))
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn keysyms_written_as_values_read_like_their_names() {
        // KWin writes keysyms as values.
        assert_eq!(caps_lock_key(&keymap("\t[ 0xffe5 ] ")), Some(CapsLockKey::Plain));
        assert_eq!(
            caps_lock_key(&keymap("\t[ 0xffec, 0xffe5 ] ")),
            Some(CapsLockKey::Snappys)
        );
        assert_eq!(
            caps_lock_key(&keymap("\t[ 0xff1b ] ")),
            Some(CapsLockKey::Remapped)
        );
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
}
