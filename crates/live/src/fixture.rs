//! The runner's side of the fixture app.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use snappy_core::geometry::{Rect, Size};

use crate::protocol::{Reply, Request};

pub struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Fixture {
    pub fn launch(program: &Path) -> Result<Self, String> {
        let mut child = Command::new(program)
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
        })
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
        self.framed(&Request::Open {
            id: id.to_owned(),
            frame: content,
            min_size,
            grid,
        })
    }

    pub fn frame(&mut self, id: &str) -> Result<Rect, String> {
        self.framed(&Request::Frame { id: id.to_owned() })
    }

    pub fn focus(&mut self, id: &str) -> Result<(), String> {
        self.request(&Request::Focus { id: id.to_owned() }).map(drop)
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
