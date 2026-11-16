//! Caps Lock as the Snappy key.

use snappy_platform::{CapsLock, CapsLockRefusal};

use crate::support::kwin;

#[test]
fn caps_lock_waits_for_a_new_login_when_kwin_started_without_the_xkb_folder() {
    let mut kwin = kwin!();
    let config = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".config");
    let folder = config.join("xkb");
    assert!(
        folder.is_dir(),
        "Snappy makes the folder as it starts, after KWin did, so the next login finds it"
    );

    let refused = kwin
        .platform
        .hold_caps_lock_as(snappy_core::keys::SnappyKey::DEFAULT.modifiers());
    assert!(
        matches!(&refused, Err(CapsLockRefusal::AfterLogin(reason)) if reason.contains("log out and back in")),
        "{refused:?}"
    );
    let kxkbrc = std::fs::read_to_string(config.join("kxkbrc")).unwrap_or_default();
    assert!(
        !kxkbrc.contains(snappy_linux::xkb::OPTION),
        "taken back out: {kxkbrc}"
    );
    assert!(
        folder.is_dir(),
        "the folder stays for KWin to search after the next login"
    );
    assert!(!folder.join("symbols").exists() && !folder.join("rules").exists());
    let _ = std::fs::remove_dir(&folder);
}
