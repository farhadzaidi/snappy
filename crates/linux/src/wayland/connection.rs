//! A connection to the compositor: binding its globals, sending requests, and reading
//! events with the descriptors that travel beside them.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::message::{Arg, HEADER, Message, encode};
use super::rights::{receive_with_rights, send_with_rights};

/// `wl_display` is always object 1.
const DISPLAY: u32 = 1;
const DISPLAY_SYNC: u16 = 0;
const DISPLAY_GET_REGISTRY: u16 = 1;
const DISPLAY_ERROR: u16 = 0;
const REGISTRY_BIND: u16 = 0;
const REGISTRY_GLOBAL: u16 = 0;
const CALLBACK_DONE: u16 = 0;

/// A global the compositor advertises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Global {
    pub name: u32,
    pub interface: String,
    pub version: u32,
}

/// A connection to the compositor.
pub struct Connection {
    stream: UnixStream,
    registry: u32,
    next_id: u32,
    globals: Vec<Global>,
    /// Descriptors received beside events, in the order they came, until taken.
    fds: VecDeque<OwnedFd>,
    /// A read stopped partway through a message, as a timeout can, so what follows on the
    /// stream no longer starts at a message; every read fails from then on.
    out_of_step: bool,
}

impl Connection {
    /// Connects to the session's compositor and lists its globals.
    pub fn open() -> io::Result<Connection> {
        let path =
            socket_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no Wayland display"))?;
        Connection::on(UnixStream::connect(path)?)
    }

    /// Talks to the compositor at the other end of `stream`, listing its globals.
    pub fn on(stream: UnixStream) -> io::Result<Connection> {
        let mut connection = Connection {
            stream,
            registry: 0,
            next_id: DISPLAY + 1,
            globals: Vec::new(),
            fds: VecDeque::new(),
            out_of_step: false,
        };
        connection.registry = connection.new_id();
        connection.send(DISPLAY, DISPLAY_GET_REGISTRY, &[Arg::Uint(connection.registry)])?;
        for message in connection.roundtrip()? {
            if message.object == connection.registry && message.opcode == REGISTRY_GLOBAL {
                let mut reader = message.reader();
                if let (Some(name), Some(interface), Some(version)) =
                    (reader.uint(), reader.string(), reader.uint())
                {
                    connection.globals.push(Global {
                        name,
                        interface,
                        version,
                    });
                }
            }
        }
        Ok(connection)
    }

    /// A second handle on the socket, to shut it down from another thread.
    pub fn shutdown_handle(&self) -> io::Result<UnixStream> {
        self.stream.try_clone()
    }

    /// The globals the compositor advertised when the connection opened.
    pub fn globals(&self) -> &[Global] {
        &self.globals
    }

    /// Binds the global offering `interface`, at its version or `version`, whichever is lower.
    /// Returns the new object and the version bound.
    pub fn bind(&mut self, interface: &str, version: u32) -> io::Result<(u32, u32)> {
        let global = self
            .globals
            .iter()
            .find(|global| global.interface == interface)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, format!("no {interface}")))?;
        self.bind_global(&global, version)
    }

    /// Binds one particular global, as when the compositor offers several of a kind.
    pub fn bind_global(&mut self, global: &Global, version: u32) -> io::Result<(u32, u32)> {
        let interface = global.interface.as_str();
        let version = version.min(global.version);
        let id = self.new_id();
        self.send(
            self.registry,
            REGISTRY_BIND,
            &[
                Arg::Uint(global.name),
                Arg::Str(interface),
                Arg::Uint(version),
                Arg::Uint(id),
            ],
        )?;
        Ok((id, version))
    }

    pub fn send(&mut self, object: u32, opcode: u16, args: &[Arg]) -> io::Result<()> {
        let message = encode(object, opcode, args)?;
        self.stream.write_all(&message)
    }

    /// Sends a request with one file descriptor argument, which travels beside the message
    /// rather than in it.
    pub fn send_with_fd(&mut self, object: u32, opcode: u16, args: &[Arg], fd: BorrowedFd) -> io::Result<()> {
        let message = encode(object, opcode, args)?;
        send_with_rights(&self.stream, &message, fd)
    }

    /// The next event, waiting for it. A protocol error the compositor reports is an error.
    /// Descriptors an event carries travel beside it; `take_fd` hands them out.
    pub fn read(&mut self) -> io::Result<Message> {
        if self.out_of_step {
            return Err(io::Error::other(
                "the connection to the compositor is out of step",
            ));
        }
        let mut header = [0; HEADER];
        self.receive(&mut header)?;
        let object = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let word = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
        let size = (word >> 16) as usize;
        let opcode = (word & 0xffff) as u16;
        let mut body = vec![0; size.saturating_sub(HEADER)];
        if let Err(error) = self.receive(&mut body) {
            self.out_of_step = true;
            return Err(error);
        }
        let message = Message { object, opcode, body };
        if object == DISPLAY && opcode == DISPLAY_ERROR {
            let mut reader = message.reader();
            let (_, code, text) = (reader.uint(), reader.uint(), reader.string());
            return Err(io::Error::other(format!(
                "Wayland protocol error {}: {}",
                code.unwrap_or_default(),
                text.unwrap_or_default()
            )));
        }
        Ok(message)
    }

    /// Reads events until one `awaited` accepts arrives, for answers a compositor sends later
    /// than a roundtrip, such as a layer surface's first configure. Returns every event read,
    /// the awaited one last, or a timeout error after `timeout`.
    pub fn read_until(
        &mut self,
        timeout: Duration,
        awaited: impl Fn(&Message) -> bool,
    ) -> io::Result<Vec<Message>> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        let result = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the compositor didn't answer",
                ));
            }
            if let Err(error) = self.stream.set_read_timeout(Some(left)) {
                break Err(error);
            }
            match self.read() {
                Ok(message) => {
                    let found = awaited(&message);
                    events.push(message);
                    if found {
                        break Ok(());
                    }
                }
                Err(error) => break Err(error),
            }
        };
        self.stream.set_read_timeout(None)?;
        result.map(|()| events)
    }

    /// Waits until the compositor has handled every request sent so far, and returns the
    /// events it sent meanwhile.
    pub fn roundtrip(&mut self) -> io::Result<Vec<Message>> {
        let callback = self.new_id();
        self.send(DISPLAY, DISPLAY_SYNC, &[Arg::Uint(callback)])?;
        let mut events = Vec::new();
        loop {
            let message = self.read()?;
            if message.object == callback && message.opcode == CALLBACK_DONE {
                return Ok(events);
            }
            events.push(message);
        }
    }

    /// The oldest descriptor an event read so far carried and nothing took yet.
    pub fn take_fd(&mut self) -> Option<OwnedFd> {
        self.fds.pop_front()
    }

    /// Fills `buffer` from the socket, keeping the descriptors that arrive beside the bytes.
    fn receive(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        let mut filled = 0;
        while filled < buffer.len() {
            match receive_with_rights(&self.stream, &mut buffer[filled..], &mut self.fds) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    self.out_of_step |= filled > 0;
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// An ID for a new object. Client IDs count up from 2, and nothing Snappy does creates
    /// objects often enough to run out.
    pub fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// `$WAYLAND_DISPLAY`, relative to `$XDG_RUNTIME_DIR` unless it is a path; `wayland-0` when unset.
fn socket_path() -> Option<PathBuf> {
    let display = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    let display = PathBuf::from(display);
    if display.is_absolute() {
        return Some(display);
    }
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(display))
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::super::keymap::{KEYBOARD_KEYMAP, KEYMAP_XKB_V1, read_keymap};
    use super::*;

    #[test]
    fn events_are_read_in_order_and_errors_are_reported() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut connection = Connection {
            stream: client,
            registry: 2,
            next_id: 3,
            globals: Vec::new(),
            fds: VecDeque::new(),
            out_of_step: false,
        };
        server
            .write_all(&encode(4, 0, &[Arg::Uint(6), Arg::Uint(3)]).unwrap())
            .unwrap();
        server
            .write_all(
                &encode(
                    DISPLAY,
                    DISPLAY_ERROR,
                    &[Arg::Uint(4), Arg::Uint(1), Arg::Str("bad")],
                )
                .unwrap(),
            )
            .unwrap();
        let event = connection.read().unwrap();
        let mut reader = event.reader();
        assert_eq!(
            (event.object, reader.uint(), reader.uint()),
            (4, Some(6), Some(3))
        );
        let error = connection.read().unwrap_err();
        assert!(error.to_string().contains("bad"));
    }

    #[test]
    fn a_read_that_stops_inside_a_message_fails_every_read_after_it() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut connection = Connection {
            stream: client,
            registry: 2,
            next_id: 3,
            globals: Vec::new(),
            fds: VecDeque::new(),
            out_of_step: false,
        };
        let event = encode(4, 0, &[Arg::Uint(6), Arg::Uint(3)]).unwrap();
        server.write_all(&event[..HEADER + 2]).unwrap();
        let cut = connection.read_until(Duration::from_millis(50), |_| true);
        assert!(cut.is_err(), "half a message is no message");

        server.write_all(&event[HEADER + 2..]).unwrap();
        server.write_all(&event).unwrap();
        assert!(
            connection.read().is_err(),
            "the rest of the cut message would read as a header"
        );
    }

    #[test]
    fn a_file_descriptor_beside_an_event_is_kept_until_taken() {
        use std::os::fd::AsFd;
        let (client, server) = UnixStream::pair().unwrap();
        let mut connection = Connection {
            stream: client,
            registry: 2,
            next_id: 3,
            globals: Vec::new(),
            fds: VecDeque::new(),
            out_of_step: false,
        };
        let path = std::env::temp_dir().join(format!("snappy-keymap-{}", std::process::id()));
        std::fs::write(&path, b"xkb_keymap {\n};\n\0").unwrap();
        let file = File::open(&path).unwrap();
        let event = encode(4, KEYBOARD_KEYMAP, &[Arg::Uint(KEYMAP_XKB_V1), Arg::Uint(17)]).unwrap();
        send_with_rights(&server, &event, file.as_fd()).unwrap();
        (&server).write_all(&encode(4, 1, &[]).unwrap()).unwrap();

        let message = connection.read().unwrap();
        assert_eq!((message.object, message.opcode), (4, KEYBOARD_KEYMAP));
        assert_eq!(
            connection.read().unwrap().opcode,
            1,
            "the events after it still read"
        );
        let fd = connection.take_fd().unwrap();
        assert!(connection.take_fd().is_none());
        assert_eq!(read_keymap(&File::from(fd), 17).unwrap(), "xkb_keymap {\n};\n");
        std::fs::remove_file(&path).unwrap();
    }
}
