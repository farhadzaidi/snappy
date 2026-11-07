//! The runner's side of the fixture app.
//!
//! On GNOME on Wayland, windows can't place or read their own frames, so the fixture only
//! opens and closes them and the test helper in GNOME Shell places and reads them, finding
//! each by its title.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use snappy_core::geometry::{Rect, Size};

use crate::protocol::{Reply, Request};

/// Set to `x11` to open the fixture's windows through Xwayland in a Wayland session.
pub const DISPLAY_SERVER: &str = "SNAPPY_LIVE_FIXTURE_DISPLAY";
/// How long a new window may take to appear and take its place.
#[cfg(target_os = "linux")]
const APPEAR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Where frames come from: the fixture's own windows, or GNOME Shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frames {
    Own,
    #[cfg(target_os = "linux")]
    Gnome,
}

pub struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    frames: Frames,
    /// The windows open through Wayland, which has no size increments.
    wayland: bool,
}

impl Fixture {
    pub fn launch(program: &Path) -> Result<Self, String> {
        let mut command = Command::new(program);
        let (frames, wayland) = Self::display_server(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", program.display()))?;
        let input = child.stdin.take().ok_or("the fixture has no input pipe")?;
        let output = child.stdout.take().ok_or("the fixture has no output pipe")?;
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            frames,
            wayland,
        })
    }

    /// Where frames come from, and whether windows open through Wayland.
    #[cfg(target_os = "linux")]
    fn display_server(command: &mut Command) -> (Frames, bool) {
        if !crate::gnome::is_session() {
            return (Frames::Own, false);
        }
        if std::env::var(DISPLAY_SERVER).is_ok_and(|server| server == "x11") {
            command.env_remove("WAYLAND_DISPLAY");
            (Frames::Gnome, false)
        } else {
            (Frames::Gnome, true)
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn display_server(_command: &mut Command) -> (Frames, bool) {
        (Frames::Own, false)
    }

    /// Whether windows can size in steps, which Wayland has no way to ask for.
    pub fn supports_size_steps(&self) -> bool {
        !self.wayland
    }

    /// The fixture's windows are titled after their IDs.
    fn title(id: &str) -> String {
        format!("Fixture {id}")
    }

    /// The fixture app's process, which owns its windows.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn request(&mut self, request: &Request) -> Result<Reply, String> {
        let line = serde_json::to_string(request).map_err(|error| error.to_string())?;
        writeln!(self.input, "{line}")
            .and_then(|()| self.input.flush())
            .map_err(|error| format!("the fixture stopped listening: {error}"))?;
        let mut answer = String::new();
        self.output
            .read_line(&mut answer)
            .map_err(|error| format!("the fixture stopped answering: {error}"))?;
        let reply: Reply =
            serde_json::from_str(&answer).map_err(|error| format!("bad reply {answer:?}: {error}"))?;
        match reply.error {
            Some(error) => Err(error),
            None => Ok(reply),
        }
    }

    fn framed(&mut self, request: &Request) -> Result<Rect, String> {
        self.request(request)?
            .frame
            .ok_or_else(|| "the fixture sent no frame".to_owned())
    }

    /// Open a window whose content is `content` in size, at `content`'s origin.
    pub fn open(
        &mut self,
        id: &str,
        content: Rect,
        min_size: Option<Size>,
        grid: Option<Size>,
    ) -> Result<Rect, String> {
        let opened = self.framed(&Request::Open {
            id: id.to_owned(),
            frame: content,
            min_size,
            grid,
        })?;
        match self.frames {
            Frames::Own => Ok(opened),
            #[cfg(target_os = "linux")]
            Frames::Gnome => Self::place(&Self::title(id), content),
        }
    }

    /// Waits for the window to appear in GNOME Shell, then moves its frame to `content`'s
    /// origin and waits until it is there.
    #[cfg(target_os = "linux")]
    fn place(title: &str, content: Rect) -> Result<Rect, String> {
        use crate::input::pause;
        let started = std::time::Instant::now();
        while crate::gnome::window_frame(title)?.is_none_or(|frame| frame.width <= 1.0) {
            if started.elapsed() > APPEAR_TIMEOUT {
                return Err(format!("{title} never appeared"));
            }
            pause(20);
        }
        // GNOME Shell places a new window itself when it first shows it, so placing is
        // repeated until the window stays where it was put.
        let origin = content.origin();
        let at_origin = |frame: Rect| (frame.x - origin.x).abs() < 1.0 && (frame.y - origin.y).abs() < 1.0;
        let mut settled = 0;
        loop {
            let frame = crate::gnome::window_frame(title)?.ok_or_else(|| format!("{title} closed"))?;
            if at_origin(frame) {
                settled += 1;
                if settled >= 5 {
                    return Ok(frame);
                }
            } else {
                settled = 0;
                crate::gnome::place_window(title, origin)?;
            }
            if started.elapsed() > APPEAR_TIMEOUT {
                return Err(format!("{title} stayed at ({:.0}, {:.0})", frame.x, frame.y));
            }
            pause(20);
        }
    }

    pub fn frame(&mut self, id: &str) -> Result<Rect, String> {
        match self.frames {
            Frames::Own => self.framed(&Request::Frame { id: id.to_owned() }),
            #[cfg(target_os = "linux")]
            Frames::Gnome => {
                crate::gnome::window_frame(&Self::title(id))?.ok_or_else(|| format!("no window {id}"))
            }
        }
    }

    pub fn focus(&mut self, id: &str) -> Result<(), String> {
        self.request(&Request::Focus { id: id.to_owned() })?;
        match self.frames {
            Frames::Own => Ok(()),
            #[cfg(target_os = "linux")]
            Frames::Gnome => crate::gnome::focus_window(&Self::title(id)),
        }
    }

    pub fn full_screen(&mut self, id: &str, on: bool) -> Result<(), String> {
        self.request(&Request::FullScreen {
            id: id.to_owned(),
            on,
        })
        .map(drop)
    }

    pub fn close(&mut self, id: &str) -> Result<(), String> {
        self.request(&Request::Close { id: id.to_owned() }).map(drop)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.request(&Request::Quit);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
