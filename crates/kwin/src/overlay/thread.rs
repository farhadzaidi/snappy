//! The overlay's own thread and the commands the engine's side sends it, so the engine
//! never waits for KWin or for drawing.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};

use snappy_core::geometry::Point;
use snappy_linux::journal;
use snappy_platform::{OverlayMarks, OverlayScene};

use super::session::Session;

/// Where a display's overlay goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// The output's connector name, such as `HDMI-A-1`.
    pub output: String,
    /// The output's top-left corner in KWin's coordinates.
    pub origin: Point,
    /// The output's scale; buffers are drawn at the next whole multiple.
    pub scale: f64,
}

pub enum Command {
    /// Makes the display's surface, invisibly, with the scene drawn, ahead of a drag.
    Prepare {
        display: String,
        target: Target,
        scene: OverlayScene,
    },
    Show {
        display: String,
        target: Target,
        scene: OverlayScene,
    },
    Hide,
    /// Outlines windows on the display's marks surface; no frames hides it.
    Mark {
        display: String,
        target: Target,
        marks: OverlayMarks,
    },
    /// Drops what is kept for displays that are gone.
    Retain(Vec<String>),
    Stop,
}

/// The overlay thread's handle.
pub struct Overlays {
    commands: Sender<Command>,
}

impl Overlays {
    pub fn start() -> io::Result<Overlays> {
        let (commands, received) = mpsc::channel();
        std::thread::Builder::new()
            .name("snappy-kwin-overlay".into())
            .spawn(move || run(&received))?;
        Ok(Overlays { commands })
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            journal::warn("the zone overlay's thread has stopped");
        }
    }
}

fn run(commands: &Receiver<Command>) {
    let mut session: Option<Session> = None;
    // The last failure, so one that repeats on every pointer move is logged once.
    let mut last_failure: Option<String> = None;
    for command in commands {
        let result = match command {
            Command::Stop => return,
            // A connection that failed, as when KWin restarted, gets one fresh try.
            Command::Prepare {
                display,
                target,
                scene,
            } => retried(&mut session, |open| open.prepare(&display, &target, &scene)),
            Command::Show {
                display,
                target,
                scene,
            } => retried(&mut session, |open| open.show(&display, &target, &scene)),
            Command::Hide => session.as_mut().map_or(Ok(()), Session::hide),
            Command::Mark {
                display,
                target,
                marks,
            } => retried(&mut session, |open| open.mark(&display, &target, &marks)),
            Command::Retain(displays) => session.as_mut().map_or(Ok(()), |open| open.retain(&displays)),
        };
        match result {
            Ok(()) => last_failure = None,
            Err(error) => {
                let failure = error.to_string();
                if last_failure.as_ref() != Some(&failure) {
                    journal::warn(format_args!("the zone overlay can't be shown: {failure}"));
                    last_failure = Some(failure);
                }
                // Start over with a fresh connection next time; KWin may have restarted.
                session = None;
            }
        }
    }
}

/// Runs `action` on the session, connecting first if needed, and once more on a fresh
/// connection if it fails.
fn retried(session: &mut Option<Session>, action: impl Fn(&mut Session) -> io::Result<()>) -> io::Result<()> {
    connected(session).and_then(&action).or_else(|_| {
        *session = None;
        connected(session).and_then(&action)
    })
}

fn connected(session: &mut Option<Session>) -> io::Result<&mut Session> {
    if session.is_none() {
        *session = Some(Session::open()?);
    }
    session
        .as_mut()
        .ok_or_else(|| io::Error::other("no connection to KWin"))
}
