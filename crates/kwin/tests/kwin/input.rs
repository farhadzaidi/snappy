//! KWin's fake input device, which the tests drive the pointer and keyboard with.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use snappy_core::geometry::Point;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_kwin::wire::fake_input::{
    AUTHENTICATE, BUTTON, INTERFACE, KEYBOARD_KEY, POINTER_MOTION_ABSOLUTE, PRESSED, RELEASED, VERSION,
};
use snappy_linux::key_codes;
use snappy_linux::wayland::{Arg, Connection};

/// The session's one fake input device. KWin gives the seat a pointer and a keyboard when it
/// appears, and apps that were already running when that happens can trip over it, so it is
/// created before any fixture window and kept for the whole run.
pub fn input() -> MutexGuard<'static, FakeInput> {
    static INPUT: OnceLock<Mutex<FakeInput>> = OnceLock::new();
    INPUT
        .get_or_init(|| Mutex::new(FakeInput::open()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// KWin's fake input device.
pub struct FakeInput {
    connection: Connection,
    device: u32,
}

/// The primary button's Linux input code.
pub const BUTTON_LEFT: u32 = 0x110;

impl FakeInput {
    pub fn open() -> FakeInput {
        let mut connection = Connection::open().unwrap();
        let (device, version) = connection.bind(INTERFACE, VERSION).unwrap();
        assert!(version >= VERSION, "KWin's fake input can't press keys");
        connection
            .send(
                device,
                AUTHENTICATE,
                &[Arg::Str("Snappy tests"), Arg::Str("Drive the test session")],
            )
            .unwrap();
        let mut input = FakeInput { connection, device };
        input.sync();
        input
    }

    /// Waits until KWin has handled everything sent so far.
    pub fn sync(&mut self) {
        self.connection.roundtrip().unwrap();
    }

    pub fn move_to(&mut self, point: Point) {
        self.connection
            .send(
                self.device,
                POINTER_MOTION_ABSOLUTE,
                &[Arg::Fixed(point.x), Arg::Fixed(point.y)],
            )
            .unwrap();
        self.sync();
    }

    pub fn button(&mut self, down: bool) {
        let state = if down { PRESSED } else { RELEASED };
        self.connection
            .send(self.device, BUTTON, &[Arg::Uint(BUTTON_LEFT), Arg::Uint(state)])
            .unwrap();
        self.sync();
    }

    /// A key by its evdev keycode.
    pub fn key(&mut self, keycode: u8, down: bool) {
        let code = u32::from(keycode - key_codes::EVDEV_OFFSET);
        let state = if down { PRESSED } else { RELEASED };
        self.connection
            .send(self.device, KEYBOARD_KEY, &[Arg::Uint(code), Arg::Uint(state)])
            .unwrap();
        self.sync();
    }

    pub fn modifier(&mut self, modifier: Modifier, down: bool) {
        self.key(key_codes::modifier_keycode(modifier), down);
    }

    pub fn press_chord(&mut self, chord: &KeyChord) {
        let combo = chord.combo(snappy_core::keys::SnappyKey::DEFAULT);
        let modifiers: Vec<Modifier> = combo.mods.iter().collect();
        for modifier in &modifiers {
            self.modifier(*modifier, true);
        }
        let key = key_codes::evdev_keycode(combo.code.as_str()).unwrap();
        self.key(key, true);
        self.key(key, false);
        for modifier in modifiers.iter().rev() {
            self.modifier(*modifier, false);
        }
    }
}
