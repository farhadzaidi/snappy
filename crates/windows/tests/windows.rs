//! The adapter against a real Windows desktop: real windows in another process, real
//! hotkeys, hooks, and input. These tests run only when `SNAPPY_WINDOWS_TEST=1`, which
//! `tools/live/windows/test.sh` sets inside the test VM's desktop session. Anywhere else
//! they print "skipped" and pass, so `cargo test` never touches a desktop.
//!
//! Windows to act on come from this test binary run again as a helper process, because
//! the adapter's hooks skip Snappy's own process the way they would in the daemon.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, KeyChord, Modifier, Mods, Platform};
use snappy_core::settings::{Settings, Workspace};
use snappy_core::workspace::{ParkingPolicy, parking_origin};
use snappy_platform::{
    Caption, DesktopGuard, Display, HotKeys, Input, LoginItem, Overlay, OverlayScene, OverlayZone,
    PlatformEvent, WindowSystem,
};
use snappy_windows::reference;
use snappy_windows::{App, Claim, Stopper, WindowsPlatform};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOD_ALT, MOD_CONTROL, MOD_WIN, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, RegisterHotKey, SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_LSHIFT,
    VK_LWIN,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, GetForegroundWindow,
    GetMessageW, GetSystemMetrics, IsWindow, MSG, PostMessageW, PostThreadMessageW, RegisterClassExW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWNORMAL,
    SetForegroundWindow, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_QUIT, WNDCLASSEXW,
    WS_CAPTION, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW, WS_SYSMENU,
};
use windows::core::{PCWSTR, w};

const ENABLED: &str = "SNAPPY_WINDOWS_TEST";
const HELPER_PREFIX: &str = "snappy-host:";
const EVENT_TIMEOUT: Duration = Duration::from_secs(3);

fn enabled() -> bool {
    std::env::var(ENABLED).is_ok_and(|value| value == "1")
}

macro_rules! require_desktop {
    () => {
        if !enabled() {
            eprintln!("skipped: set {ENABLED}=1 on the test VM's desktop");
            return;
        }
    };
}

fn pause(milliseconds: u64) {
    std::thread::sleep(Duration::from_millis(milliseconds));
}

// MARK: The adapter on its own main thread

struct Session {
    platform: WindowsPlatform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Session {
    fn start() -> Session {
        reference::set_dpi_aware();
        let (events, received) = mpsc::channel();
        let (handles, made) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let app = App::new(events).unwrap();
            handles.send((app.platform(), app.stopper())).unwrap();
            app.run();
        });
        let (platform, stopper) = made.recv().unwrap();
        pause(200);
        Session {
            platform,
            events: received,
            stopper,
            thread: Some(thread),
        }
    }

    fn drain(&self) {
        while self.events.try_recv().is_ok() {}
    }

    /// The first event `wanted` accepts, within the timeout.
    fn expect(&self, what: &str, wanted: impl Fn(&PlatformEvent) -> bool) -> PlatformEvent {
        let deadline = Instant::now() + EVENT_TIMEOUT;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(event) if wanted(&event) => return event,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("no {what} event arrived");
    }

    /// Every event within `window`.
    fn collect(&self, window: Duration) -> Vec<PlatformEvent> {
        let deadline = Instant::now() + window;
        let mut events = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        events
    }

    fn primary(&mut self) -> Display {
        self.platform.displays().into_iter().next().unwrap()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stopper.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// MARK: Helper windows in another process

#[derive(Clone, Copy)]
enum Kind {
    Normal,
    Tool,
    Fixed,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Normal => "normal",
            Kind::Tool => "tool",
            Kind::Fixed => "fixed",
        }
    }
}

struct Host {
    child: Option<Child>,
    window: isize,
}

impl Host {
    /// A window of `kind` whose window rectangle is `rect`, in a helper process.
    fn open(kind: Kind, rect: Rect) -> Host {
        let spec = spec(kind, rect, 60, None);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "helper_window_host",
                "--nocapture",
                "--test-threads=1",
                &spec,
            ])
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let window = output
            .lines()
            .map_while(Result::ok)
            // The test harness prints the helper's own line ahead of it, unterminated.
            .find_map(|line| {
                line.split("hwnd ")
                    .nth(1)
                    .map(|value| value.trim().parse::<isize>().unwrap())
            })
            .expect("the helper printed no window");
        pause(400);
        Host {
            child: Some(child),
            window,
        }
    }

    /// A normal window in a helper running as administrator, which UAC in the VM
    /// elevates without asking. It closes itself after a while, since Snappy's process
    /// may not end it.
    fn open_elevated(rect: Rect) -> Host {
        let file = std::env::temp_dir().join(format!("snappy-elevated-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let spec = spec(Kind::Normal, rect, 5, Some(&file));
        let parameters = wide(&format!(
            "--exact helper_window_host --nocapture --test-threads=1 \"{spec}\""
        ));
        let program = wide(&std::env::current_exe().unwrap().display().to_string());
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(program.as_ptr()),
            lpParameters: PCWSTR(parameters.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        unsafe { ShellExecuteExW(&mut info) }.unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let window = loop {
            if let Ok(text) = std::fs::read_to_string(&file)
                && let Ok(window) = text.trim().parse::<isize>()
            {
                break window;
            }
            assert!(
                Instant::now() < deadline,
                "the elevated helper never opened its window"
            );
            pause(100);
        };
        pause(400);
        Host { child: None, window }
    }

    fn hwnd(&self) -> HWND {
        HWND(self.window as *mut _)
    }

    fn frame(&self) -> Rect {
        reference::visible_frame(self.window).unwrap()
    }
}

impl Drop for Host {
    /// Ends the helper. An elevated one can't be ended from here, and while its window is
    /// in front, Windows drops the input later tests inject, so wait for it to close.
    fn drop(&mut self) {
        match self.child.take() {
            Some(mut child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            None => {
                let deadline = Instant::now() + Duration::from_secs(15);
                while unsafe { IsWindow(Some(self.hwnd())) }.as_bool() && Instant::now() < deadline {
                    pause(100);
                }
            }
        }
    }
}

fn spec(kind: Kind, rect: Rect, seconds: u32, file: Option<&std::path::Path>) -> String {
    let file = file.map(|path| path.display().to_string()).unwrap_or_default();
    format!(
        "{HELPER_PREFIX}{}:{}:{}:{}:{}:{seconds}:{file}",
        kind.name(),
        rect.x,
        rect.y,
        rect.width,
        rect.height
    )
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn host_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Not a test: the helper process's body. It opens one window and keeps it alive.
#[test]
fn helper_window_host() {
    let Some(spec) = std::env::args().find_map(|arg| arg.strip_prefix(HELPER_PREFIX).map(str::to_owned))
    else {
        return;
    };
    let parts: Vec<&str> = spec.splitn(7, ':').collect();
    let number = |index: usize| parts[index].parse::<f64>().unwrap() as i32;
    let (style, extended) = match parts[0] {
        "tool" => (WS_OVERLAPPEDWINDOW, WS_EX_TOOLWINDOW),
        "fixed" => (WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0), WINDOW_EX_STYLE(0)),
        _ => (WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE(0)),
    };
    reference::set_dpi_aware();
    let window = unsafe {
        let module = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(host_procedure),
            hInstance: module.into(),
            lpszClassName: w!("SnappyTestHost"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        let (x, y, width, height) = (number(1), number(2), number(3), number(4));
        let window = CreateWindowExW(
            extended,
            w!("SnappyTestHost"),
            w!("Snappy test window"),
            style,
            if width > 0 { x } else { CW_USEDEFAULT },
            y,
            width,
            height,
            None,
            None,
            Some(module.into()),
            None,
        )
        .unwrap();
        let _ = ShowWindow(window, SW_SHOWNORMAL);
        window
    };
    let handle = window.0 as isize;
    if let Some(file) = parts.get(6).filter(|file| !file.is_empty()) {
        std::fs::write(file, handle.to_string()).unwrap();
    }
    println!("hwnd {handle}");
    let seconds: u64 = parts[5].parse().unwrap();
    let thread = unsafe { GetCurrentThreadId() };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        let _ = unsafe { PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)) };
    });
    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            DispatchMessageW(&message);
        }
    }
}

// MARK: Synthetic input, delivered like hardware input

fn send(inputs: &[INPUT]) {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    assert_eq!(sent as usize, inputs.len(), "input was blocked");
}

fn mouse(at: Point, flags: MOUSE_EVENT_FLAGS) {
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let normalize = |value: f64, start: i32, extent: i32| {
        ((value - f64::from(start)) * 65535.0 / f64::from((extent - 1).max(1))).round() as i32
    };
    send(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalize(at.x, left, width),
                dy: normalize(at.y, top, height),
                dwFlags: flags | MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                ..Default::default()
            },
        },
    }]);
}

fn key_input(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn virtual_key(vk: VIRTUAL_KEY, down: bool) {
    let mut flags = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    if vk == VK_LWIN {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    send(&[key_input(vk, 0, flags)]);
    pause(15);
}

fn modifiers(mods: Mods, down: bool) {
    let mut order = vec![Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];
    if !down {
        order.reverse();
    }
    for modifier in order.into_iter().filter(|modifier| mods.contains(*modifier)) {
        virtual_key(VIRTUAL_KEY(reference::modifier_key(modifier)), down);
    }
}

fn press(combo: &Combo) {
    modifiers(combo.mods, true);
    let (scan, extended) = reference::scan_code(&combo.code).unwrap();
    for down in [true, false] {
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        send(&[key_input(VIRTUAL_KEY(0), scan, flags)]);
        pause(20);
    }
    modifiers(combo.mods, false);
}

/// Drags from `from` to `to` with the primary button.
fn drag(from: Point, to: Point, steps: u32) {
    mouse(from, MOUSE_EVENT_FLAGS(0));
    pause(50);
    mouse(from, MOUSEEVENTF_LEFTDOWN);
    pause(100);
    for step in 1..=steps {
        let t = f64::from(step) / f64::from(steps);
        mouse(
            Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t),
            MOUSE_EVENT_FLAGS(0),
        );
        pause(20);
    }
    mouse(to, MOUSEEVENTF_LEFTUP);
    pause(200);
}

fn snappy(code: &str) -> Combo {
    KeyChord::snappy(code, []).combo()
}

fn near(a: Rect, b: Rect, tolerance: f64) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.width, b.width), (a.height, b.height)]
        .iter()
        .all(|(x, y)| (x - y).abs() <= tolerance)
}

fn message(result: Result<snappy_platform::WindowInfo, WindowError>) -> String {
    result.err().map(|error| error.to_string()).unwrap_or_default()
}

// MARK: Displays and windows

#[test]
fn displays_report_frames_work_areas_and_scale() {
    require_desktop!();
    let mut session = Session::start();
    let display = session.primary();
    assert!(!display.id.is_empty());
    assert!(!display.name.is_empty());
    assert_eq!(display.frame, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    assert!(display.frame.contains_rect(display.work_area));
    assert!(
        display.work_area.height < display.frame.height,
        "the taskbar isn't excluded"
    );
    assert_eq!(display.scale, 1.0);
}

#[test]
fn a_normal_window_is_managed_with_its_visible_frame() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(200.0, 150.0, 800.0, 600.0));
    let frame = host.frame();
    let info = session.platform.window_at(frame.center()).unwrap();
    assert_eq!(info.frame, frame);
    assert!(
        frame.width < 800.0,
        "the visible frame includes invisible borders"
    );
    assert_eq!(session.platform.frame(info.id).unwrap(), frame);
    assert!(info.app.ends_with(".exe"));
    assert!(info.recovery_key.is_some());
    assert!(
        session
            .platform
            .visible_windows(None)
            .iter()
            .any(|window| window.id == info.id)
    );
    assert!(!session.platform.is_closed(info.id));
    drop(host);
    pause(300);
    assert!(session.platform.is_closed(info.id));
}

#[test]
fn tool_and_fixed_size_windows_are_refused() {
    require_desktop!();
    let mut session = Session::start();
    let tool = Host::open(Kind::Tool, Rect::new(100.0, 100.0, 500.0, 400.0));
    assert_eq!(
        message(session.platform.window_at(tool.frame().center())),
        "Choose a standard application window."
    );
    drop(tool);
    let fixed = Host::open(Kind::Fixed, Rect::new(150.0, 150.0, 500.0, 400.0));
    assert_eq!(
        message(session.platform.window_at(fixed.frame().center())),
        "This window does not allow moving and resizing."
    );
}

#[test]
fn windows_of_elevated_apps_are_refused() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open_elevated(Rect::new(300.0, 200.0, 600.0, 400.0));
    assert_eq!(
        message(session.platform.window_at(host.frame().center())),
        "Snappy can't move windows of apps running as administrator."
    );
}

#[test]
fn writes_land_on_the_visible_frame() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(300.0, 200.0, 700.0, 500.0));
    let id = session.platform.window_at(host.frame().center()).unwrap().id;
    let target = Rect::new(12.0, 12.0, 940.0, 900.0);
    session.platform.set_size(id, target.size()).unwrap();
    session.platform.set_position(id, target.origin()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !near(host.frame(), target, 1.0) {
        assert!(Instant::now() < deadline, "the window is at {:?}", host.frame());
        pause(10);
    }
    assert_eq!(session.platform.frame(id).unwrap(), host.frame());
}

#[test]
fn parked_windows_go_fully_off_screen_unclamped_and_come_back() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(400.0, 200.0, 700.0, 500.0));
    let id = session.platform.window_at(host.frame().center()).unwrap().id;
    let before = host.frame();
    let frames: Vec<Rect> = session
        .platform
        .displays()
        .iter()
        .map(|display| display.frame)
        .collect();
    let parked = parking_origin(ParkingPolicy::OffScreen, before, &frames).unwrap();

    session.platform.hide_for_workspace(id, parked).unwrap();
    let after = host.frame();
    assert!(
        near(after, Rect::from_origin_size(parked, before.size()), 1.0),
        "parked at {after:?}, not {parked:?}"
    );
    assert!(frames.iter().all(|display| display.intersection(after).is_none()));
    pause(1000);
    assert!(
        near(host.frame(), after, 1.0),
        "Windows moved the parked window back"
    );

    session.platform.show_for_workspace(id, before.origin()).unwrap();
    assert!(near(host.frame(), before, 1.0));
}

// MARK: Shortcuts

#[test]
fn registered_shortcuts_fire_on_their_keys() {
    require_desktop!();
    let mut session = Session::start();
    let chords = [(7, snappy("Digit1")), (8, snappy("ArrowRight"))];
    assert!(session.platform.register(&chords).is_empty());
    session.drain();
    press(&chords[0].1);
    session.expect("zone shortcut", |event| *event == PlatformEvent::HotKey(7));
    press(&chords[1].1);
    session.expect("arrow shortcut", |event| *event == PlatformEvent::HotKey(8));
    session.platform.unregister_all();
}

/// The reserved-chord audit: every default chord, moving between zones, to the nearest
/// empty zone, the workspace steps, taking windows along, and switching to workspaces 1
/// to 9, is free on a stock Windows 11, so Windows and the shell leave them to Snappy.
#[test]
fn the_windows_default_shortcuts_are_free() {
    require_desktop!();
    let mut session = Session::start();
    let settings = Settings::defaults(Platform::Windows);
    let mut chords: Vec<KeyChord> = settings
        .navigation
        .chords()
        .map(|(_, chord)| chord.clone())
        .collect();
    chords.extend(settings.navigation.nearest_empty.clone());
    chords.extend(settings.workspaces.steps().map(|(_, chord)| chord.clone()));
    chords.extend((1..=9).filter_map(|n| Workspace::numbered(format!("workspace-{n}"), n).switch_shortcut));
    assert_eq!(chords.len(), 5 + 5 + 9);
    let taken: Vec<String> = chords
        .iter()
        .filter(|chord| !session.platform.is_available(&chord.combo()))
        .map(|chord| chord.long_label(Platform::Windows))
        .collect();
    assert!(taken.is_empty(), "Windows holds {taken:?}");
}

#[test]
fn chords_held_by_another_app_are_unavailable() {
    require_desktop!();
    let mut session = Session::start();
    let chord = snappy("Digit9");
    assert!(session.platform.is_available(&chord));
    // A thread hotkey of this test thread stands in for another app's registration.
    let flags = MOD_CONTROL | MOD_ALT | MOD_WIN;
    unsafe { RegisterHotKey(None, 1, flags, u32::from(b'9')) }.unwrap();
    assert!(!session.platform.is_available(&chord));
    unsafe { UnregisterHotKey(None, 1) }.unwrap();
    assert!(session.platform.is_available(&chord));

    assert!(session.platform.register(&[(3, chord.clone())]).is_empty());
    assert!(
        session.platform.is_available(&chord),
        "Snappy's own shortcut counts as free"
    );
    session.platform.unregister_all();
}

// MARK: Drags and keys

#[test]
fn moving_a_window_reports_its_press_drag_and_release() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(300.0, 200.0, 700.0, 500.0));
    let id = session.platform.window_at(host.frame().center()).unwrap().id;
    session.platform.set_press_monitoring(true);
    session.platform.set_drag_monitoring(true);
    session.drain();
    let grip = Point::new(host.frame().mid_x(), host.frame().y + 14.0);
    drag(grip, Point::new(grip.x - 150.0, grip.y + 100.0), 10);

    let events = session.collect(Duration::from_millis(500));
    let down = events.iter().position(
        |event| matches!(event, PlatformEvent::PointerDown { window: Some(window), .. } if *window == id),
    );
    let dragged = events
        .iter()
        .position(|event| matches!(event, PlatformEvent::PointerDragged { .. }));
    let up = events
        .iter()
        .position(|event| matches!(event, PlatformEvent::PointerUp { .. }));
    assert!(
        matches!((down, dragged, up), (Some(d), Some(m), Some(u)) if d < m && m < u),
        "{events:?}"
    );
    session.platform.set_drag_monitoring(false);
    session.platform.set_press_monitoring(false);
}

#[test]
fn presses_are_unreported_while_monitoring_is_off() {
    require_desktop!();
    let session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(300.0, 200.0, 700.0, 500.0));
    session.drain();
    let grip = Point::new(host.frame().mid_x(), host.frame().y + 14.0);
    drag(grip, Point::new(grip.x + 80.0, grip.y + 40.0), 5);
    let events = session.collect(Duration::from_millis(400));
    assert!(
        !events.iter().any(|event| matches!(
            event,
            PlatformEvent::PointerDown { .. } | PlatformEvent::PointerUp { .. }
        )),
        "{events:?}"
    );
}

#[test]
fn modifier_changes_are_reported_only_while_dragging() {
    require_desktop!();
    let mut session = Session::start();
    session.platform.set_drag_monitoring(true);
    pause(200);
    session.drain();
    virtual_key(VK_LSHIFT, true);
    session.expect("shift", |event| {
        matches!(event, PlatformEvent::ModifiersChanged { mods, .. } if mods.contains(Modifier::Shift))
    });
    virtual_key(VK_LSHIFT, false);

    session.platform.set_drag_monitoring(false);
    pause(200);
    session.drain();
    virtual_key(VK_LSHIFT, true);
    virtual_key(VK_LSHIFT, false);
    let events = session.collect(Duration::from_millis(400));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PlatformEvent::ModifiersChanged { .. })),
        "keys were observed outside a drag: {events:?}"
    );
}

// MARK: Overlay, tray, desktops, lifecycle

#[test]
fn overlays_float_above_windows_without_taking_focus() {
    require_desktop!();
    let mut session = Session::start();
    let host = Host::open(Kind::Normal, Rect::new(300.0, 200.0, 700.0, 500.0));
    unsafe {
        let _ = SetForegroundWindow(host.hwnd());
    }
    pause(200);
    let display = session.primary();
    let area = display.work_area;
    let half = Size::new(area.width / 2.0, area.height);
    let zone = |x: f64, number: usize, highlighted: bool| OverlayZone {
        frame: Rect::from_origin_size(Point::new(x, area.y), half),
        number,
        highlighted,
        caption: highlighted.then_some(Caption::Stack),
        swap_source: false,
    };
    let scene = OverlayScene {
        work_area: area,
        zones: vec![zone(area.x, 1, false), zone(area.x + half.width, 2, true)],
    };
    let foreground = unsafe { GetForegroundWindow() };
    session.platform.prepare(&display.id, &scene);
    session.platform.show(&display.id, &scene);
    pause(300);
    let own = std::process::id();
    let shown = reference::on_screen_windows();
    assert!(
        shown
            .iter()
            .any(|window| window.pid == own && window.topmost && window.frame.contains_rect(area)),
        "{shown:?}"
    );
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    session.platform.hide();
    pause(300);
    assert!(
        !reference::on_screen_windows()
            .iter()
            .any(|window| window.pid == own && window.frame.contains_rect(area))
    );
}

#[test]
fn the_notification_area_icon_is_added() {
    require_desktop!();
    let _session = Session::start();
    pause(300);
    assert!(reference::has_tray_icon(std::process::id()));
}

#[test]
fn another_virtual_desktop_pauses_workspaces() {
    require_desktop!();
    let mut session = Session::start();
    let _member = Host::open(Kind::Normal, Rect::new(300.0, 200.0, 700.0, 500.0));
    let displays = session.platform.displays();
    session.platform.bind(&displays);
    pause(300);
    assert!(session.platform.on_managed_desktop());
    session.drain();
    // Win+Ctrl+D opens a new desktop and Win+Ctrl+F4 closes it; each animates for a
    // moment before the next keys are taken.
    let windows_ctrl = Mods::from_iter([Modifier::Ctrl, Modifier::Meta]);
    press(&Combo::new("KeyD", windows_ctrl.iter()));
    pause(2000);
    let away = session.platform.on_managed_desktop();
    let changed = session
        .collect(Duration::from_millis(200))
        .contains(&PlatformEvent::NativeDesktopChanged);
    press(&Combo::new("F4", windows_ctrl.iter()));
    pause(2000);
    let back = session.platform.on_managed_desktop();
    session.platform.clear();
    assert!(!away, "the new virtual desktop counted as the managed one");
    assert!(changed, "switching desktops wasn't reported");
    assert!(back, "closing the new desktop didn't return to the managed one");
}

#[test]
fn open_at_login_writes_and_removes_the_run_value() {
    require_desktop!();
    let mut session = Session::start();
    session.platform.set_enabled(true).unwrap();
    assert!(session.platform.is_enabled());
    session.platform.set_enabled(false).unwrap();
    assert!(!session.platform.is_enabled());
}

#[test]
fn a_second_launch_asks_the_first_to_open_settings() {
    require_desktop!();
    let Claim::Primary(mut first) = snappy_windows::claim().unwrap() else {
        panic!("another daemon is running");
    };
    let (events, received) = mpsc::channel();
    first.publish(events).unwrap();
    assert!(matches!(snappy_windows::claim().unwrap(), Claim::Secondary));
    assert_eq!(
        received.recv_timeout(EVENT_TIMEOUT).unwrap(),
        PlatformEvent::OpenSettingsRequested
    );
    drop(first);
    assert!(matches!(snappy_windows::claim().unwrap(), Claim::Primary(_)));
}

#[test]
fn a_close_request_asks_the_engine_to_quit() {
    require_desktop!();
    let session = Session::start();
    let own = std::process::id();
    let mut after = None;
    let window = loop {
        let window = unsafe { FindWindowExW(None, after, w!("Snappy.Daemon"), None) }.unwrap();
        if pid_of(window) == own {
            break window;
        }
        after = Some(window);
    };
    session.drain();
    unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
    session.expect("quit request", |event| *event == PlatformEvent::QuitRequested);
}

fn pid_of(window: HWND) -> u32 {
    let mut pid = 0;
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(window, Some(&mut pid)) };
    pid
}
