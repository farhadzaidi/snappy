//! The numbers of KDE's Wayland protocols Snappy speaks, as KWin's protocol files define them.
//! The live-test harness speaks them too.

/// `org_kde_kwin_fake_input`, a device whose input KWin handles like hardware's.
pub mod fake_input {
    pub const INTERFACE: &str = "org_kde_kwin_fake_input";
    /// Version 4 added keyboard keys.
    pub const VERSION: u32 = 4;
    pub const AUTHENTICATE: u16 = 0;
    pub const BUTTON: u16 = 2;
    pub const POINTER_MOTION_ABSOLUTE: u16 = 9;
    pub const KEYBOARD_KEY: u16 = 10;
    pub const PRESSED: u32 = 1;
    pub const RELEASED: u32 = 0;
}

/// `org_kde_kwin_keystate`, which reports the lock and modifier keys' states.
pub mod keystate {
    pub const INTERFACE: &str = "org_kde_kwin_keystate";
    /// Version 5 added the modifiers; earlier ones report only the locks.
    pub const VERSION: u32 = 5;
    pub const FETCH_STATES: u16 = 0;
    pub const STATE_CHANGED: u16 = 0;
    /// The protocol's key numbers.
    pub const KEY_CAPS_LOCK: u32 = 0;
    pub const KEY_NUM_LOCK: u32 = 1;
    pub const KEY_SCROLL_LOCK: u32 = 2;
    pub const KEY_ALT: u32 = 3;
    pub const KEY_CONTROL: u32 = 4;
    pub const KEY_SHIFT: u32 = 5;
    pub const KEY_META: u32 = 6;
    pub const KEY_ALT_GR: u32 = 7;
    /// A key's state: up and neither latched nor locked, latched, locked, or held down.
    pub const UNLOCKED: u32 = 0;
    pub const LATCHED: u32 = 1;
    pub const LOCKED: u32 = 2;
    pub const PRESSED: u32 = 3;
}
