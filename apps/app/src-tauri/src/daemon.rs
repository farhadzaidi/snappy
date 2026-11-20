//! The link to the daemon over inherited pipes: a reader thread for `ToApp` frames on
//! standard input and a writer thread for `ToDaemon` frames on standard output.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread;

use snappy_ipc::{Command, DAEMON_ARGUMENT, PROTOCOL_VERSION, ToApp, ToDaemon, read_message, write_message};
use tauri::{AppHandle, Emitter, Manager};

use crate::visibility::Visibility;

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
    pending: Vec<ToApp>,
}

impl Inbox {
    fn deliver(&self, app: &AppHandle, message: ToApp) {
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

fn emit(app: &AppHandle, message: &ToApp) {
    if let Err(error) = app.emit(MESSAGE_EVENT, message) {
        eprintln!("snappy-app: couldn't relay a daemon message: {error}");
    }
}

/// The pipes to the daemon, opened before the window exists and started once it does.
pub struct Link {
    receiver: Receiver<ToDaemon>,
}

/// Open the link when the daemon started the app. Run any other way, the app shows its
/// window against no daemon.
pub fn open() -> (Outbox, Option<Link>) {
    if !std::env::args()
        .skip(1)
        .any(|argument| argument == DAEMON_ARGUMENT)
    {
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
            eprintln!("snappy-app: couldn't write to the daemon: {error}");
            return;
        }
    }
}

/// Read until the daemon closes the pipe, then quit: the app never outlives it.
fn read_loop(app: &AppHandle) {
    let mut input = io::stdin().lock();
    loop {
        match read_message::<ToApp>(&mut input) {
            Ok(Some(message)) => handle(app, message),
            Ok(None) => return app.exit(0),
            Err(error) => {
                eprintln!("snappy-app: couldn't read from the daemon: {error}");
                return app.exit(1);
            }
        }
    }
}

fn handle(app: &AppHandle, message: ToApp) {
    match message {
        ToApp::Hello { protocol_version } if protocol_version != PROTOCOL_VERSION => {
            eprintln!("snappy-app: the daemon speaks protocol {protocol_version}, not {PROTOCOL_VERSION}");
            app.exit(1);
        }
        ToApp::Hello { .. } => {}
        ToApp::Focus => app.state::<Visibility>().bring_forward(app),
        other => app.state::<Inbox>().deliver(app, other),
    }
}
