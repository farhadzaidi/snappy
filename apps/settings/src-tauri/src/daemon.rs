//! The link to the daemon over inherited pipes: a reader thread for `ToSettings` frames on
//! standard input and a writer thread for `ToDaemon` frames on standard output.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread;

use snappy_ipc::{Command, PROTOCOL_VERSION, ToDaemon, ToSettings, read_message, write_message};
use tauri::{AppHandle, Emitter, Manager};

use crate::window;

/// The event that carries daemon messages to the web UI.
pub const MESSAGE_EVENT: &str = "snappy://message";

/// Sends messages to the daemon without blocking the caller. Disconnected when the app
/// runs without a daemon, and then messages are dropped.
#[derive(Clone)]
pub struct Outbox {
    sender: Option<Sender<ToDaemon>>,
}

impl Outbox {
    pub fn send(&self, message: ToDaemon) {
        if let Some(sender) = &self.sender {
            // A closed channel means the writer stopped because the daemon went away.
            let _ = sender.send(message);
        }
    }

    pub fn send_command(&self, id: u64, command: Command) {
        self.send(ToDaemon::Command {
            id,
            command: Box::new(command),
        });
    }
}

/// Daemon messages wait here until the web UI says it is listening.
#[derive(Default)]
pub struct Inbox {
    state: Mutex<InboxState>,
}

#[derive(Default)]
struct InboxState {
    ui_listening: bool,
    pending: Vec<ToSettings>,
}

impl Inbox {
    fn deliver(&self, app: &AppHandle, message: ToSettings) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.ui_listening {
            emit(app, &message);
        } else {
            state.pending.push(message);
        }
    }

    /// Mark the web UI as listening and hand it everything that arrived before.
    pub fn open(&self, app: &AppHandle) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.ui_listening = true;
        for message in state.pending.drain(..) {
            emit(app, &message);
        }
    }
}

fn emit(app: &AppHandle, message: &ToSettings) {
    if let Err(error) = app.emit(MESSAGE_EVENT, message) {
        eprintln!("snappy-settings: could not relay a daemon message: {error}");
    }
}

/// The pipes to the daemon, opened before the window exists and started once it does.
pub struct Link {
    receiver: Receiver<ToDaemon>,
}

/// Open the link when standard input is the daemon's pipe. Run any other way, the app
/// shows its window against no daemon.
pub fn open() -> (Outbox, Option<Link>) {
    if !stdin_is_pipe() {
        return (Outbox { sender: None }, None);
    }
    let (sender, receiver) = mpsc::channel();
    let outbox = Outbox { sender: Some(sender) };
    outbox.send(ToDaemon::Hello {
        protocol_version: PROTOCOL_VERSION,
    });
    (outbox, Some(Link { receiver }))
}

impl Link {
    pub fn start(self, app: &AppHandle) {
        let receiver = self.receiver;
        thread::spawn(move || write_loop(receiver));
        let app = app.clone();
        thread::spawn(move || read_loop(&app));
    }
}

fn write_loop(receiver: Receiver<ToDaemon>) {
    let mut output = io::stdout().lock();
    for message in receiver {
        if let Err(error) = write_message(&mut output, &message) {
            eprintln!("snappy-settings: could not write to the daemon: {error}");
            return;
        }
    }
}

/// Read until the daemon closes the pipe, then quit: the settings app never outlives it.
fn read_loop(app: &AppHandle) {
    let mut input = io::stdin().lock();
    loop {
        match read_message::<ToSettings>(&mut input) {
            Ok(Some(message)) => handle(app, message),
            Ok(None) => return app.exit(0),
            Err(error) => {
                eprintln!("snappy-settings: could not read from the daemon: {error}");
                return app.exit(1);
            }
        }
    }
}

fn handle(app: &AppHandle, message: ToSettings) {
    match message {
        ToSettings::Hello { protocol_version } if protocol_version != PROTOCOL_VERSION => {
            eprintln!(
                "snappy-settings: the daemon speaks protocol {protocol_version}, not {PROTOCOL_VERSION}"
            );
            app.exit(1);
        }
        ToSettings::Hello { .. } => {}
        ToSettings::Focus => window::focus(app),
        other => app.state::<Inbox>().deliver(app, other),
    }
}

#[cfg(unix)]
fn stdin_is_pipe() -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::FileTypeExt;

    io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|file| file.metadata())
        .is_ok_and(|metadata| metadata.file_type().is_fifo())
}

#[cfg(not(unix))]
fn stdin_is_pipe() -> bool {
    use std::io::IsTerminal;

    !io::stdin().is_terminal()
}
