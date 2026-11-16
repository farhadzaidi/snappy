//! The adapter running against the session's KWin, the fixture app's windows, and how
//! long tests wait for either.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_kwin::{App, KWinPlatform, Stopper};
use snappy_platform::{LiveControl, Platform, PlatformEvent, WindowInfo, WindowSystem};

use crate::input::input;

pub const WAIT: Duration = Duration::from_secs(5);
/// Where a title bar is grabbed, below the window's top edge.
pub const TITLE_BAR_GRIP: f64 = 14.0;

/// The adapter against the session's KWin, or, outside the test session, a note that the
/// test was skipped and a return from it.
macro_rules! kwin {
    () => {
        match $crate::support::Session::start() {
            Some(session) => session,
            None => {
                println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
                return;
            }
        }
    };
}
pub(crate) use kwin;

/// The adapter running against the session's KWin, with the events it reports.
pub struct Session {
    pub platform: KWinPlatform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    run: Option<JoinHandle<()>>,
}

impl Session {
    pub fn start() -> Option<Session> {
        if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
            return None;
        }
        drop(input());
        let (sender, events) = mpsc::channel();
        let app = App::new(sender).unwrap();
        let platform = app.platform();
        let stopper = app.stopper();
        let run = std::thread::spawn(move || app.run().unwrap());
        let mut session = Session {
            platform,
            events,
            stopper,
            run: Some(run),
        };
        session.wait_until("the script answers", |session| {
            session.platform.live_control() == LiveControl::Available
        });
        Some(session)
    }

    pub fn wait_until(&mut self, what: &str, mut done: impl FnMut(&mut Session) -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting until {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The next event, skipping ones the test doesn't look at.
    #[track_caller]
    pub fn next(&self, wanted: impl Fn(&PlatformEvent) -> bool) -> PlatformEvent {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = self.events.recv_timeout(left).expect("an event");
            if wanted(&event) {
                return event;
            }
            println!("passed over {event:?}");
        }
    }

    pub fn drain(&self) {
        while self.events.try_recv().is_ok() {}
    }

    /// The fixture's window, once KWin manages it.
    pub fn window_of(&mut self, fixture: &Fixture) -> WindowInfo {
        let pid = fixture.pid();
        let mut found = None;
        self.wait_until("the fixture's window appears", |session| {
            found = session
                .platform
                .all_windows()
                .into_iter()
                .find(|window| window.pid == pid);
            found.is_some()
        });
        found.unwrap()
    }

    pub fn place(&mut self, window: &WindowInfo, origin: Point) -> Rect {
        self.platform.set_position(window.id, origin).unwrap();
        let mut frame = Rect::default();
        self.wait_until("the window moves", |session| {
            frame = session.platform.frame(window.id).unwrap();
            (frame.x - origin.x).abs() < 1.0 && (frame.y - origin.y).abs() < 1.0
        });
        frame
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stopper.stop();
        if let Some(run) = self.run.take() {
            let _ = run.join();
        }
    }
}

/// The fixture app, with one window open.
pub struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Fixture {
    pub fn open(id: &str, size: Size) -> Fixture {
        let program =
            std::env::var("SNAPPY_KWIN_FIXTURE").expect("SNAPPY_KWIN_FIXTURE names the fixture app");
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut fixture = Fixture {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
        };
        let request = serde_json::json!({
            "op": "open", "id": id,
            "frame": {"x": 0.0, "y": 0.0, "width": size.width, "height": size.height},
            "minSize": null, "grid": null,
        });
        writeln!(fixture.input, "{request}").unwrap();
        fixture.input.flush().unwrap();
        let mut reply = String::new();
        fixture.output.read_line(&mut reply).unwrap();
        fixture
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = writeln!(self.input, r#"{{"op":"quit"}}"#);
        let _ = self.input.flush();
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
