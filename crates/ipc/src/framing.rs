//! Framing on the pipes: a 4-byte little-endian length followed by UTF-8 JSON.

use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

/// The argument the daemon starts the app with, telling it that its standard input
/// and output are the pipe. The app doesn't guess from standard input: a Windows release build
/// started on its own has a null standard input, which reads as no terminal and ends at once.
pub const DAEMON_ARGUMENT: &str = "--daemon";

/// Messages larger than this are refused; settings files are capped well below it.
pub const MAX_MESSAGE_BYTES: u32 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("pipe error: {0}")]
    Io(#[from] io::Error),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message of {0} bytes is too large")]
    TooLarge(u32),
}

pub fn write_message<T: Serialize>(writer: &mut impl Write, message: &T) -> Result<(), FrameError> {
    let body = serde_json::to_vec(message)?;
    let length = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Read one message, or `None` when the other side closed the pipe.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FrameError> {
    let mut header = [0_u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_le_bytes(header);
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    let mut body = vec![0_u8; length as usize];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::messages::{PROTOCOL_VERSION, ToDaemon};

    #[test]
    fn messages_round_trip_through_the_framing() {
        let mut pipe = Vec::new();
        let command = ToDaemon::Command {
            id: 7,
            command: Box::new(Command::ApplyLayout {
                display: "d".into(),
                layout: "halves".into(),
            }),
        };
        let hello = ToDaemon::Hello {
            protocol_version: PROTOCOL_VERSION,
        };
        write_message(&mut pipe, &command).unwrap();
        write_message(&mut pipe, &hello).unwrap();
        let mut reader = pipe.as_slice();
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), Some(command));
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), Some(hello));
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), None);
    }

    #[test]
    fn oversized_messages_are_refused() {
        let mut reader: &[u8] = &(MAX_MESSAGE_BYTES + 1).to_le_bytes();
        assert!(matches!(
            read_message::<ToDaemon>(&mut reader),
            Err(FrameError::TooLarge(_))
        ));
    }
}
