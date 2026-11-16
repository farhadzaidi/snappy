//! Requests and events as the wire protocol lays them out: a header with the object,
//! opcode, and size, then the arguments, each padded to four bytes.

use std::io;

pub(super) const HEADER: usize = 8;

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

fn padded(length: usize) -> usize {
    length.div_ceil(4) * 4
}

pub(super) fn encode(object: u32, opcode: u16, args: &[Arg]) -> io::Result<Vec<u8>> {
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
    fn readers_stop_at_the_end_of_the_body() {
        let message = Message {
            object: 1,
            opcode: 0,
            body: vec![1, 0],
        };
        assert_eq!(message.reader().uint(), None);
    }
}
