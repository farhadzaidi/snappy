//! Passing file descriptors beside messages with `sendmsg` and `recvmsg`, which the
//! standard library doesn't offer yet.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;

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
const MSG_CMSG_CLOEXEC: i32 = 0x4000_0000;
/// Room for the most descriptors one message may carry, 28 as libwayland sends them, as
/// `CMSG_SPACE` counts it.
const CONTROL_ROOM: usize = size_of::<ControlHeader>() + 28 * size_of::<i32>();

/// A control buffer aligned as `cmsghdr` needs.
#[repr(C, align(8))]
struct Control([u8; CONTROL_ROOM]);

unsafe extern "C" {
    fn sendmsg(socket: i32, message: *const MessageHeader, flags: i32) -> isize;
    fn recvmsg(socket: i32, message: *mut MessageHeader, flags: i32) -> isize;
}

/// Reads what is waiting, up to `buffer`'s length, adding the descriptors that come with it
/// to `fds`, close-on-exec. Returns how many bytes were read.
pub(super) fn receive_with_rights(
    stream: &UnixStream,
    buffer: &mut [u8],
    fds: &mut VecDeque<OwnedFd>,
) -> io::Result<usize> {
    let iov = IoVec {
        base: buffer.as_mut_ptr().cast(),
        len: buffer.len(),
    };
    let mut control = Control([0; CONTROL_ROOM]);
    let mut header = MessageHeader {
        name: std::ptr::null_mut(),
        name_len: 0,
        iov: &iov,
        iov_len: 1,
        control: control.0.as_mut_ptr().cast(),
        control_len: CONTROL_ROOM,
        flags: 0,
    };
    // SAFETY: every pointer in `header` points at a live local or at `buffer` for the length
    // given, and the layouts match glibc's (checked above).
    let read = unsafe { recvmsg(stream.as_raw_fd(), &mut header, MSG_CMSG_CLOEXEC) };
    let read = usize::try_from(read).map_err(|_| io::Error::last_os_error())?;
    let received = control.0.get(..header.control_len).unwrap_or(&control.0);
    for fd in descriptors(received) {
        // SAFETY: the kernel just opened this descriptor for this process, and nothing else
        // owns it.
        fds.push_back(unsafe { OwnedFd::from_raw_fd(fd) });
    }
    Ok(read)
}

/// The descriptors in the `SCM_RIGHTS` messages of a received control buffer: each message
/// is a `cmsghdr` (length, level, kind) followed by its data, padded to eight bytes.
fn descriptors(control: &[u8]) -> Vec<i32> {
    let header = size_of::<ControlHeader>();
    let mut fds = Vec::new();
    let mut at = 0;
    while let Some(bytes) = control.get(at..at + header) {
        let (length, rest) = bytes.split_at(8);
        let (level, kind) = rest.split_at(4);
        let length = usize::from_ne_bytes(length.try_into().unwrap_or_default());
        let level = i32::from_ne_bytes(level.try_into().unwrap_or_default());
        let kind = i32::from_ne_bytes(kind.try_into().unwrap_or_default());
        if length < header {
            break;
        }
        if level == SOL_SOCKET && kind == SCM_RIGHTS {
            let data = control
                .get(at + header..(at + length).min(control.len()))
                .unwrap_or_default();
            fds.extend(
                data.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|chunk| i32::from_ne_bytes(*chunk)),
            );
        }
        at += length.next_multiple_of(8);
    }
    fds
}

pub(super) fn send_with_rights(stream: &UnixStream, bytes: &[u8], fd: BorrowedFd) -> io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::super::message::{Arg, encode};
    use super::*;

    #[test]
    fn a_file_descriptor_travels_beside_its_request() {
        use std::io::Read;
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
    fn descriptors_are_read_from_each_rights_message_and_others_skipped() {
        let message = |level: i32, kind: i32, fds: &[i32]| {
            let mut bytes = Vec::new();
            bytes.extend((16 + 4 * fds.len()).to_ne_bytes());
            bytes.extend(level.to_ne_bytes());
            bytes.extend(kind.to_ne_bytes());
            for fd in fds {
                bytes.extend(fd.to_ne_bytes());
            }
            bytes.resize(bytes.len().next_multiple_of(8), 0);
            bytes
        };
        let mut control = message(SOL_SOCKET, 2, &[99]);
        control.extend(message(SOL_SOCKET, SCM_RIGHTS, &[7, 8, 9]));
        assert_eq!(descriptors(&control), [7, 8, 9]);
        assert!(descriptors(&[]).is_empty());
        assert!(descriptors(&[0; 16]).is_empty(), "a zero length ends the list");
    }
}
