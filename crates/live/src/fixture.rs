//! The runner's side of the fixture app.
//!
//! On Wayland, windows can't place or read their own frames, so the fixture only opens and
//! closes them and the desktop shell places and reads them (`desktop::wayland`), finding each by
//! its title.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use snappy_core::geometry::{Point, Rect, Size};

use crate::input::pause;
use crate::protocol::{Reply, Request};

/// Set to `x11` to open the fixture's windows through Xwayland in a Wayland session.
pub const DISPLAY_SERVER: &str = "SNAPPY_LIVE_FIXTURE_DISPLAY";
/// How long a new window may take to appear and take its place.
#[cfg(target_os = "linux")]
const APPEAR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long macOS takes to bring a window following the user to the desktop showing.
const SPACE_MOVE_MS: u64 = 500;

/// Where frames come from: the fixture's own windows, or the Wayland desktop shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frames {
    Own,
    #[cfg(target_os = "linux")]
    Shell(crate::desktop::wayland::Shell),
}

pub struct Fixture {
    /// The program, which `another` starts again.
    program: PathBuf,
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
            .map_err(|error| format!("couldn't start {}: {error}", program.display()))?;
        let input = child.stdin.take().ok_or("the fixture has no input pipe")?;
        let output = child.stdout.take().ok_or("the fixture has no output pipe")?;
        Ok(Self {
            program: program.to_owned(),
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
        let Some(shell) = crate::desktop::wayland::Shell::current() else {
            return (Frames::Own, false);
        };
        if std::env::var(DISPLAY_SERVER).is_ok_and(|server| server == "x11") {
            command.env_remove("WAYLAND_DISPLAY");
            (Frames::Shell(shell), false)
        } else {
            (Frames::Shell(shell), true)
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn display_server(_command: &mut Command) -> (Frames, bool) {
        (Frames::Own, false)
    }

    /// Another fixture app, in a process of its own, for windows of a second app.
    pub fn another(&self) -> Result<Self, String> {
        Self::launch(&self.program)
    }

    /// Whether windows can size in steps, which Wayland has no way to ask for.
    pub fn supports_size_steps(&self) -> bool {
        !self.wayland
    }

    /// The fixture's windows are titled after their IDs.
    #[cfg(target_os = "linux")]
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
            Frames::Shell(shell) => Self::place(shell, &Self::title(id), content),
        }
    }

    /// Waits for the window to appear, then moves its frame to `content`'s origin and waits
    /// until it is there.
    #[cfg(target_os = "linux")]
    fn place(shell: crate::desktop::wayland::Shell, title: &str, content: Rect) -> Result<Rect, String> {
        let started = std::time::Instant::now();
        while shell.window_frame(title)?.is_none_or(|frame| frame.width <= 1.0) {
            if started.elapsed() > APPEAR_TIMEOUT {
                return Err(format!("{title} never appeared"));
            }
            pause(20);
        }
        // The shell places a new window itself when it first shows it, so placing is repeated
        // until the window stays where it was put.
        let origin = content.origin();
        let at_origin = |frame: Rect| (frame.x - origin.x).abs() < 1.0 && (frame.y - origin.y).abs() < 1.0;
        let mut settled = 0;
        loop {
            let frame = shell
                .window_frame(title)?
                .ok_or_else(|| format!("{title} closed"))?;
            if at_origin(frame) {
                settled += 1;
                if settled >= 5 {
                    return Ok(frame);
                }
            } else {
                settled = 0;
                shell.place_window(title, origin)?;
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
            Frames::Shell(shell) => shell
                .window_frame(&Self::title(id))?
                .ok_or_else(|| format!("no window {id}")),
        }
    }

    pub fn focus(&mut self, id: &str) -> Result<(), String> {
        self.request(&Request::Focus { id: id.to_owned() })?;
        match self.frames {
            Frames::Own => Ok(()),
            #[cfg(target_os = "linux")]
            Frames::Shell(shell) => shell.focus_window(&Self::title(id)),
        }
    }

    /// The fixture's window with keyboard focus, if any.
    pub fn focused(&mut self) -> Result<Option<String>, String> {
        Ok(self.request(&Request::Focused)?.focused)
    }

    pub fn full_screen(&mut self, id: &str, on: bool) -> Result<(), String> {
        self.request(&Request::FullScreen {
            id: id.to_owned(),
            on,
        })
        .map(drop)
    }

    /// Open `id` as a new tab of `into`; returns the group's frame.
    pub fn tab(&mut self, id: &str, into: &str) -> Result<Rect, String> {
        self.framed(&Request::Tab {
            id: id.to_owned(),
            into: into.to_owned(),
        })
    }

    /// Move tab `id` into a window of its own at `at`; returns its new frame.
    pub fn tear_out(&mut self, id: &str, at: Point) -> Result<Rect, String> {
        self.framed(&Request::TearOut {
            id: id.to_owned(),
            at,
        })
    }

    /// Move `id` to the native desktop showing now, where it then stays; returns its frame.
    pub fn move_to_active_space(&mut self, id: &str) -> Result<Rect, String> {
        let follow = |on| Request::FollowActiveSpace {
            id: id.to_owned(),
            on,
        };
        self.request(&follow(true))?;
        pause(SPACE_MOVE_MS);
        self.framed(&follow(false))
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
