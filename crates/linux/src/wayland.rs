//! Just enough of the Wayland wire protocol to bind a compositor's globals and trade
//! messages, passing a file descriptor where a request takes one: Snappy reads KWin's
//! modifier state and shows its overlay this way, and the live tests inject input. A full
//! client library would be far more than any of them needs.

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `wl_display` is always object 1.
const DISPLAY: u32 = 1;
const DISPLAY_SYNC: u16 = 0;
const DISPLAY_GET_REGISTRY: u16 = 1;
const DISPLAY_ERROR: u16 = 0;
const REGISTRY_BIND: u16 = 0;
const REGISTRY_GLOBAL: u16 = 0;
const CALLBACK_DONE: u16 = 0;
const HEADER: usize = 8;

/// One argument of a request, in the wire protocol's encodings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arg<'a> {
    Uint(u32),
    Int(i32),
    /// Sent as a 24.8 fixed-point number.
    Fixed(f64),
    Str(&'a str),
}

/// An event from the compositor; its arguments are read in order with `Message::reader`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub object: u32,
    pub opcode: u16,
    pub body: Vec<u8>,
}

impl Message {
    pub fn reader(&self) -> ArgReader<'_> {
        ArgReader {
            body: &self.body,
            at: 0,
        }
    }
}

/// Reads an event's arguments in the order the protocol declares them.
pub struct ArgReader<'a> {
    body: &'a [u8],
    at: usize,
}

impl ArgReader<'_> {
    pub fn uint(&mut self) -> Option<u32> {
        let bytes = self.body.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_ne_bytes(bytes.try_into().ok()?))
    }

    /// A string without its terminating zero; `None` past the end or for a null string.
    pub fn string(&mut self) -> Option<String> {
        let length = usize::try_from(self.uint()?).ok()?;
        let bytes = self.body.get(self.at..self.at + length)?;
        self.at += padded(length);
        let text = bytes.strip_suffix(&[0])?;
        Some(String::from_utf8_lossy(text).into_owned())
    }
}

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
    pub fn read(&mut self) -> io::Result<Message> {
        let mut header = [0; HEADER];
        self.stream.read_exact(&mut header)?;
        let object = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let word = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
        let size = (word >> 16) as usize;
        let opcode = (word & 0xffff) as u16;
        let mut body = vec![0; size.saturating_sub(HEADER)];
        self.stream.read_exact(&mut body)?;
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

/// `struct iovec`, `struct msghdr`, and `struct cmsghdr` as glibc lays them out on 64-bit
/// Linux, for passing a file descriptor with `sendmsg`, which the standard library doesn't
/// offer yet.
#[repr(C)]
struct IoVec {
    base: *const c_void,
    len: usize,
}

#[repr(C)]
struct MessageHeader {
    name: *mut c_void,
    name_len: u32,
    iov: *const IoVec,
    iov_len: usize,
    control: *mut c_void,
    control_len: usize,
    flags: i32,
}

#[repr(C)]
struct ControlHeader {
    len: usize,
    level: i32,
    kind: i32,
}

/// One `SCM_RIGHTS` control message carrying one descriptor, padded as `CMSG_SPACE` pads it.
#[repr(C, align(8))]
struct Rights {
    header: ControlHeader,
    fd: i32,
    padding: i32,
}

const _: () = assert!(size_of::<MessageHeader>() == 56 && size_of::<Rights>() == 24);

const SOL_SOCKET: i32 = 1;
const SCM_RIGHTS: i32 = 1;
const MSG_NOSIGNAL: i32 = 0x4000;

unsafe extern "C" {
    fn sendmsg(socket: i32, message: *const MessageHeader, flags: i32) -> isize;
}

fn send_with_rights(stream: &UnixStream, bytes: &[u8], fd: BorrowedFd) -> io::Result<()> {
    let iov = IoVec {
        base: bytes.as_ptr().cast(),
        len: bytes.len(),
    };
    let mut rights = Rights {
        header: ControlHeader {
            len: size_of::<ControlHeader>() + size_of::<i32>(),
            level: SOL_SOCKET,
            kind: SCM_RIGHTS,
        },
        fd: fd.as_raw_fd(),
        padding: 0,
    };
    let header = MessageHeader {
        name: std::ptr::null_mut(),
        name_len: 0,
        iov: &iov,
        iov_len: 1,
        control: (&raw mut rights).cast(),
        control_len: size_of::<Rights>(),
        flags: 0,
    };
    // SAFETY: every pointer in `header` points at a live local or at `bytes` for the length
    // given, the layouts match glibc's (checked above), and `fd` is open for the call.
    let sent = unsafe { sendmsg(stream.as_raw_fd(), &header, MSG_NOSIGNAL) };
    match usize::try_from(sent) {
        Ok(sent) if sent == bytes.len() => Ok(()),
        Ok(_) => Err(io::Error::other("the compositor took part of a request")),
        Err(_) => Err(io::Error::last_os_error()),
    }
}

fn padded(length: usize) -> usize {
    length.div_ceil(4) * 4
}

fn encode(object: u32, opcode: u16, args: &[Arg]) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for arg in args {
        match *arg {
            Arg::Uint(value) => body.extend_from_slice(&value.to_ne_bytes()),
            Arg::Int(value) => body.extend_from_slice(&value.to_ne_bytes()),
            Arg::Fixed(value) => body.extend_from_slice(&((value * 256.0).round() as i32).to_ne_bytes()),
            Arg::Str(text) => {
                let length = text.len() + 1;
                body.extend_from_slice(&u32::try_from(length).map_err(io::Error::other)?.to_ne_bytes());
                body.extend_from_slice(text.as_bytes());
                body.resize(body.len() + padded(length) - text.len(), 0);
            }
        }
    }
    let size = u32::try_from(HEADER + body.len())
        .ok()
        .filter(|size| *size <= u32::from(u16::MAX))
        .ok_or_else(|| io::Error::other("request too large"))?;
    let mut message = Vec::with_capacity(HEADER + body.len());
    message.extend_from_slice(&object.to_ne_bytes());
    message.extend_from_slice(&((size << 16) | u32::from(opcode)).to_ne_bytes());
    message.extend_from_slice(&body);
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_ne_bytes(*chunk))
            .collect()
    }

    #[test]
    fn requests_carry_their_size_and_opcode_in_the_header() {
        let message = encode(3, 9, &[Arg::Uint(7), Arg::Int(-1)]).unwrap();
        assert_eq!(words(&message), vec![3, (16 << 16) | 9, 7, u32::MAX]);
    }

    #[test]
    fn strings_end_in_a_zero_and_pad_to_four_bytes() {
        let message = encode(2, 0, &[Arg::Str("wl_seat")]).unwrap();
        assert_eq!(message.len(), HEADER + 4 + 8);
        assert_eq!(words(&message)[2], 8, "seven letters and a zero");
        assert_eq!(&message[12..20], b"wl_seat\0");
        let reader_message = Message {
            object: 2,
            opcode: 0,
            body: message[HEADER..].to_vec(),
        };
        assert_eq!(reader_message.reader().string().as_deref(), Some("wl_seat"));
    }

    #[test]
    fn fixed_point_numbers_have_eight_fraction_bits() {
        let message = encode(5, 1, &[Arg::Fixed(1.5), Arg::Fixed(-2.0)]).unwrap();
        assert_eq!(words(&message)[2], 384);
        assert_eq!(words(&message)[3] as i32, -512);
    }

    #[test]
    fn events_are_read_in_order_and_errors_are_reported() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut connection = Connection {
            stream: client,
            registry: 2,
            next_id: 3,
            globals: Vec::new(),
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
    fn a_file_descriptor_travels_beside_its_request() {
        use std::os::fd::AsFd;
        let (client, server) = UnixStream::pair().unwrap();
        let file = std::fs::File::open("/proc/self/exe").unwrap();
        let message = encode(5, 0, &[Arg::Uint(9), Arg::Int(4096)]).unwrap();
        send_with_rights(&client, &message, file.as_fd()).unwrap();
        let mut received = vec![0; message.len()];
        (&server).read_exact(&mut received).unwrap();
        assert_eq!(received, message);
    }

    #[test]
    fn readers_stop_at_the_end_of_the_body() {
        let message = Message {
            object: 1,
            opcode: 0,
            body: vec![1, 0],
        };
        assert_eq!(message.reader().uint(), None);
    }
}
