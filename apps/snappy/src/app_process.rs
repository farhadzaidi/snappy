//! Launching the app and talking to it over its standard input and output.

use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};

use snappy_engine::Input;
use snappy_ipc::{PROTOCOL_VERSION, ToApp, ToDaemon, read_message, write_message};
use tokio::sync::mpsc::UnboundedSender;

/// The app's process, while it runs.
pub struct AppProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    engine: UnboundedSender<Input>,
}

impl AppProcess {
    pub fn new(engine: UnboundedSender<Input>) -> Self {
        Self {
            child: None,
            stdin: None,
            engine,
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    /// Start the app unless it's already running.
    pub fn open(&mut self) -> std::io::Result<()> {
        if self.is_running() {
            return Ok(());
        }
        let mut child = Command::new(executable()?)
            .arg(snappy_ipc::DAEMON_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no stdout"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("no stdin"))?;
        write_message(
            &mut stdin,
            &ToApp::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .map_err(std::io::Error::other)?;

        let pid = child.id();
        self.stdin = Some(stdin);
        self.child = Some(child);
        let _ = self.engine.send(Input::AppStarted { pid });
        self.listen(stdout);
        Ok(())
    }

    /// Forward the app's commands to the engine.
    fn listen(&self, stdout: std::process::ChildStdout) {
        let engine = self.engine.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = read_message::<ToDaemon>(&mut reader) {
                match message {
                    ToDaemon::Hello { protocol_version } if protocol_version != PROTOCOL_VERSION => break,
                    ToDaemon::Hello { .. } => {}
                    ToDaemon::Command { id, command } => {
                        let _ = engine.send(Input::Command {
                            id,
                            command: *command,
                        });
                    }
                }
            }
            let _ = engine.send(Input::AppExited);
        });
    }

    pub fn send(&mut self, messages: Vec<ToApp>) {
        let Some(pipe) = self.stdin.as_mut() else { return };
        for message in messages {
            if write_message(pipe, &message).is_err() {
                self.stdin = None;
                return;
            }
        }
    }
}

/// The app's binary: inside Snappy.app when installed, next to the daemon in
/// development builds.
fn executable() -> std::io::Result<PathBuf> {
    let daemon = std::env::current_exe()?;
    let folder = daemon
        .parent()
        .ok_or_else(|| std::io::Error::other("no executable folder"))?;
    let bundled = folder.join("../Helpers/Snappy App.app/Contents/MacOS/snappy-app");
    let name = if cfg!(windows) {
        "snappy-app.exe"
    } else {
        "snappy-app"
    };
    let candidates = [bundled, folder.join(name)];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "snappy-app not found"))
}
