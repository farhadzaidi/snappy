//! KDE's keyboard settings file, `kxkbrc`, where KWin reads the XKB options it builds the
//! keymap with: the `Options` entry of the `[Layout]` group, and only while
//! `ResetOldOptions` is true; otherwise KWin takes the session's `XKB_DEFAULT_OPTIONS`.
//!
//! Snappy changes only those two entries, in place, and keeps every other line as written,
//! so writing back the entries it read gives back the file it read. The file is KConfig's
//! format: `[Group]` headers and `Key=Value` lines.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const GROUP: &str = "[Layout]";
const RESET_OLD_OPTIONS: &str = "ResetOldOptions";
const OPTIONS: &str = "Options";
/// The keys KWin's notification names, so it reloads the group.
pub const CHANGED_KEYS: [&str; 2] = [RESET_OLD_OPTIONS, OPTIONS];
/// The group name as KConfig's change notification gives it.
pub const CHANGED_GROUP: &str = "Layout";

/// The two entries of `[Layout]` that decide the options, each as written, or absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionEntries {
    pub reset_old_options: Option<String>,
    pub options: Option<String>,
}

impl OptionEntries {
    /// The entries that make KWin use `options`.
    pub fn using(options: &str) -> OptionEntries {
        OptionEntries {
            reset_old_options: Some("true".to_owned()),
            options: Some(options.to_owned()),
        }
    }

    /// Whether KWin reads `Options`: KConfig reads a boolean from these words.
    pub fn resets_old_options(&self) -> bool {
        self.reset_old_options.as_deref().is_some_and(|value| {
            ["true", "on", "yes", "1"]
                .iter()
                .any(|word| value.trim().eq_ignore_ascii_case(word))
        })
    }

    /// The options KWin builds the keymap with, given the session's `XKB_DEFAULT_OPTIONS`.
    pub fn effective_options(&self, environment: Option<&str>) -> String {
        if self.resets_old_options() {
            self.options.clone().unwrap_or_default()
        } else {
            environment.unwrap_or_default().to_owned()
        }
    }
}

/// The options list with `option` added once at the end, empty items dropped.
pub fn with_option(options: &str, option: &str) -> String {
    let mut list: Vec<&str> = options
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != option)
        .collect();
    list.push(option);
    list.join(",")
}

/// The options list without `option`.
pub fn without_option(options: &str, option: &str) -> String {
    options
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != option)
        .collect::<Vec<_>>()
        .join(",")
}

/// Whether the options list holds `option`.
pub fn has_option(options: &str, option: &str) -> bool {
    options.split(',').any(|item| item.trim() == option)
}

fn is_header(line: &str) -> bool {
    line.trim_start().starts_with('[')
}

/// The key of a `Key=Value` line, without KConfig's `[$i]`-style flags, and its value.
fn entry(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim_end();
    let key = key.split_once('[').map_or(key, |(name, _)| name);
    Some((key, value.trim()))
}

/// The entries as `text` has them; a later `[Layout]` group overrides an earlier one, as
/// KConfig reads it.
pub fn read(text: &str) -> OptionEntries {
    let mut entries = OptionEntries::default();
    let mut inside = false;
    for line in text.lines() {
        if is_header(line) {
            inside = line.trim() == GROUP;
            continue;
        }
        match entry(line) {
            Some((RESET_OLD_OPTIONS, value)) if inside => entries.reset_old_options = Some(value.to_owned()),
            Some((OPTIONS, value)) if inside => entries.options = Some(value.to_owned()),
            _ => {}
        }
    }
    entries
}

/// `text` with the two entries set as `entries` says, every other line kept. An entry already
/// there keeps its line, its key as written, flags such as `[$i]` included, and the spaces
/// around its value; one that is `None` loses its line. A new one goes at the top of the first
/// `[Layout]` group, which is added at the end when there is none, and a group left with
/// nothing in it goes.
pub fn write(text: &str, entries: &OptionEntries) -> String {
    let wanted = [
        (RESET_OLD_OPTIONS, entries.reset_old_options.as_deref()),
        (OPTIONS, entries.options.as_deref()),
    ];
    let mut present = [false; 2];
    let mut lines: Vec<String> = Vec::new();
    let mut inside = false;
    let mut group_header = None;
    for line in text.lines() {
        if is_header(line) {
            inside = line.trim() == GROUP;
            if inside && group_header.is_none() {
                group_header = Some(lines.len());
            }
            lines.push(line.to_owned());
            continue;
        }
        let ours = inside
            .then(|| entry(line))
            .flatten()
            .and_then(|(key, _)| wanted.iter().position(|(name, _)| *name == key));
        match ours {
            Some(index) => {
                present[index] = true;
                if let Some(value) = wanted[index].1 {
                    lines.push(with_value(line, value));
                }
            }
            None => lines.push(line.to_owned()),
        }
    }
    let missing: Vec<String> = wanted
        .iter()
        .zip(present)
        .filter(|(_, present)| !present)
        .filter_map(|((key, value), _)| value.map(|value| format!("{key}={value}")))
        .collect();
    match group_header {
        Some(header) => {
            lines.splice(header + 1..header + 1, missing);
        }
        None if !missing.is_empty() => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(GROUP.to_owned());
            lines.extend(missing);
        }
        None => {}
    }
    drop_empty_group(&mut lines);
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

/// An entry's `line` with `value` in place of its own, keeping the key as written and the
/// spaces around the value.
fn with_value(line: &str, value: &str) -> String {
    let Some((key, old)) = line.split_once('=') else {
        return line.to_owned();
    };
    let before = &old[..old.len() - old.trim_start().len()];
    let rest = &old[before.len()..];
    let after = &rest[rest.trim_end().len()..];
    format!("{key}={before}{value}{after}")
}

/// Removes a `[Layout]` header with no entries before the next group.
fn drop_empty_group(lines: &mut Vec<String>) {
    let Some(header) = lines.iter().position(|line| line.trim() == GROUP) else {
        return;
    };
    let end = lines[header + 1..]
        .iter()
        .position(|line| is_header(line))
        .map_or(lines.len(), |offset| header + 1 + offset);
    if lines[header + 1..end].iter().all(|line| line.trim().is_empty()) {
        lines.drain(header..end);
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
    }
}

/// `$XDG_CONFIG_HOME/kxkbrc`, defaulting to `~/.config`, where KConfig keeps it.
pub fn path() -> Option<PathBuf> {
    snappy_linux::xdg::config_home().map(|config| config.join("kxkbrc"))
}

/// The file's text; a missing file reads as empty.
pub fn load(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        other => other,
    }
}

/// Writes `text` through a temporary file in the same folder, as KConfig does, so KWin
/// never reads half of it. An empty text removes the file.
pub fn store(path: &Path, text: &str) -> io::Result<()> {
    if text.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no folder"))?;
    fs::create_dir_all(folder)?;
    let temporary = folder.join(".kxkbrc.snappy-tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLASMA: &str = "[$Version]\nupdate_info=kxkb.upd:remove-empty-lists\n\n[Layout]\nDisplayNames=,\nLayoutList=us,de\nOptions=grp:alt_shift_toggle\nResetOldOptions=true\nUse=true\nVariantList=,\n";

    #[test]
    fn the_entries_are_read_from_the_layout_group_only() {
        let entries = read(PLASMA);
        assert_eq!(entries.options.as_deref(), Some("grp:alt_shift_toggle"));
        assert!(entries.resets_old_options());
        assert_eq!(read("[Other]\nOptions=caps:escape\n"), OptionEntries::default());
        assert_eq!(read(""), OptionEntries::default());
    }

    #[test]
    fn options_count_only_while_reset_old_options_is_true() {
        let ignored = OptionEntries {
            reset_old_options: Some("false".to_owned()),
            options: Some("caps:escape".to_owned()),
        };
        assert_eq!(ignored.effective_options(Some("ctrl:nocaps")), "ctrl:nocaps");
        assert_eq!(ignored.effective_options(None), "");
        let read_entries = OptionEntries {
            reset_old_options: Some("Yes".to_owned()),
            options: Some("caps:escape".to_owned()),
        };
        assert_eq!(read_entries.effective_options(Some("ctrl:nocaps")), "caps:escape");
    }

    #[test]
    fn writing_changes_only_the_two_entries_in_place_and_writing_back_gives_the_file_back() {
        let written = write(PLASMA, &OptionEntries::using("grp:alt_shift_toggle,snappy:caps"));
        assert_eq!(
            written,
            PLASMA.replace(
                "Options=grp:alt_shift_toggle\n",
                "Options=grp:alt_shift_toggle,snappy:caps\n"
            )
        );
        assert_eq!(write(&written, &read(PLASMA)), PLASMA);
    }

    #[test]
    fn flags_spacing_and_missing_entries_come_back_exactly() {
        let own = "[Layout]\nLayoutList=us\nOptions[$i] = caps:none \nUse=true\n";
        let written = write(own, &OptionEntries::using("snappy:caps"));
        assert_eq!(
            written,
            "[Layout]\nResetOldOptions=true\nLayoutList=us\nOptions[$i] = snappy:caps \nUse=true\n"
        );
        assert_eq!(write(&written, &read(own)), own);

        let without = "[Layout]\nLayoutList=us\n";
        let written = write(without, &OptionEntries::using("snappy:caps"));
        assert_eq!(
            written,
            "[Layout]\nResetOldOptions=true\nOptions=snappy:caps\nLayoutList=us\n"
        );
        assert_eq!(write(&written, &read(without)), without);
    }

    #[test]
    fn a_file_without_the_group_gets_one_and_loses_it_again() {
        let own = "[Other]\nKey=value\n";
        let written = write(own, &OptionEntries::using("snappy:caps"));
        assert_eq!(
            written,
            "[Other]\nKey=value\n\n[Layout]\nResetOldOptions=true\nOptions=snappy:caps\n"
        );
        assert_eq!(write(&written, &OptionEntries::default()), own);
        assert_eq!(
            write("", &OptionEntries::using("snappy:caps")),
            "[Layout]\nResetOldOptions=true\nOptions=snappy:caps\n"
        );
        assert_eq!(
            write("[Layout]\nOptions=snappy:caps\n", &OptionEntries::default()),
            ""
        );
    }

    #[test]
    fn options_lists_gain_and_lose_snappys_option_once() {
        assert_eq!(with_option("", "snappy:caps"), "snappy:caps");
        assert_eq!(
            with_option("grp:alt_shift_toggle, snappy:caps,", "snappy:caps"),
            "grp:alt_shift_toggle,snappy:caps"
        );
        assert_eq!(
            without_option("grp:alt_shift_toggle,snappy:caps", "snappy:caps"),
            "grp:alt_shift_toggle"
        );
        assert!(has_option("a:b, snappy:caps", "snappy:caps"));
        assert!(!has_option("snappy:capsx", "snappy:caps"));
    }

    #[test]
    fn flagged_and_commented_lines_are_read_as_kconfig_reads_them() {
        let text = "[Layout]\n# Options=caps:escape\nOptions[$i]=ctrl:nocaps\n";
        assert_eq!(read(text).options.as_deref(), Some("ctrl:nocaps"));
    }

    #[test]
    fn storing_writes_through_a_temporary_file_and_an_empty_text_removes_the_file() {
        let folder = std::env::temp_dir().join(format!("snappy-kxkbrc-{}", std::process::id()));
        let path = folder.join("kxkbrc");
        store(&path, PLASMA).unwrap();
        assert_eq!(load(&path).unwrap(), PLASMA);
        store(&path, "").unwrap();
        assert!(!path.exists());
        assert_eq!(load(&path).unwrap(), "");
        fs::remove_dir_all(&folder).unwrap();
    }
}
