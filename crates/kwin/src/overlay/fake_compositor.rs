//! Just enough of a compositor to watch what the overlay asks of it, for the session's tests.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use super::protocol::{
    ALPHA_MODIFIER, BUFFER_RELEASE, COMPOSITOR, LAYER_CONFIGURE, LAYER_SHELL, OUTPUT, OUTPUT_NAME, SHM,
};

/// Offers the globals Snappy binds, names one output, configures layer surfaces when they
/// commit a new size, releases a buffer when the next is attached, and records every request
/// by interface and opcode.
pub struct FakeCompositor {
    stream: UnixStream,
    interfaces: HashMap<u32, &'static str>,
    /// Layer surface to its wl_surface, and the layer surfaces with a size to configure.
    layers: HashMap<u32, u32>,
    unconfigured: HashSet<u32>,
    attached: HashMap<u32, u32>,
    requests: Vec<(&'static str, u16)>,
    /// Every alpha multiplier set, in order.
    alphas: Vec<u32>,
    serial: u32,
}

/// Every request by interface and opcode, and every alpha multiplier set, in order.
pub type Recorded = (Vec<(&'static str, u16)>, Vec<u32>);

const GLOBALS: [(u32, &str); 5] = [
    (1, COMPOSITOR),
    (2, SHM),
    (3, LAYER_SHELL),
    (4, ALPHA_MODIFIER),
    (5, OUTPUT),
];

fn event(object: u32, opcode: u16, words: &[u32], text: Option<&str>) -> Vec<u8> {
    let mut body: Vec<u8> = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
    if let Some(text) = text {
        let length = text.len() + 1;
        body.extend_from_slice(&(length as u32).to_ne_bytes());
        body.extend_from_slice(text.as_bytes());
        body.resize(body.len() + length.div_ceil(4) * 4 - text.len(), 0);
    }
    let mut message = object.to_ne_bytes().to_vec();
    message.extend_from_slice(&((((8 + body.len()) as u32) << 16) | u32::from(opcode)).to_ne_bytes());
    message.extend_from_slice(&body);
    message
}

impl FakeCompositor {
    pub fn serve(stream: UnixStream) -> std::thread::JoinHandle<Recorded> {
        std::thread::spawn(move || {
            let mut fake = FakeCompositor {
                stream,
                interfaces: HashMap::from([(1, "wl_display")]),
                layers: HashMap::new(),
                unconfigured: HashSet::new(),
                attached: HashMap::new(),
                requests: Vec::new(),
                alphas: Vec::new(),
                serial: 0,
            };
            while fake.next() {}
            (fake.requests, fake.alphas)
        })
    }

    fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).unwrap();
    }

    /// Handles one request; false once the client hangs up.
    fn next(&mut self) -> bool {
        let mut header = [0; 8];
        if self.stream.read_exact(&mut header).is_err() {
            return false;
        }
        let object = u32::from_ne_bytes(header[0..4].try_into().unwrap());
        let word = u32::from_ne_bytes(header[4..8].try_into().unwrap());
        let opcode = (word & 0xffff) as u16;
        let mut body = vec![0; (word >> 16) as usize - 8];
        self.stream.read_exact(&mut body).unwrap();
        let words: Vec<u32> = body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_ne_bytes(*chunk))
            .collect();
        let interface = self.interfaces.get(&object).copied().unwrap_or("unknown");
        self.requests.push((interface, opcode));
        let new = |kind: &'static str, fake: &mut FakeCompositor, id: u32| {
            fake.interfaces.insert(id, kind);
        };
        match (interface, opcode) {
            ("wl_display", 0) => self.send(&event(words[0], 0, &[0], None)),
            ("wl_display", 1) => {
                new("wl_registry", self, words[0]);
                for (name, interface) in GLOBALS {
                    let mut global = event(words[0], 0, &[name], Some(interface));
                    // The version follows the name; the size in the header grows by it.
                    global.extend_from_slice(&5u32.to_ne_bytes());
                    let size = (global.len() as u32) << 16;
                    global[4..8].copy_from_slice(&size.to_ne_bytes());
                    self.send(&global);
                }
            }
            ("wl_registry", 0) => {
                let name = words[0];
                let id = *words.last().unwrap();
                let kind = match name {
                    1 => "wl_compositor",
                    2 => "wl_shm",
                    3 => "layer_shell",
                    4 => "alpha_modifier",
                    _ => "wl_output",
                };
                new(kind, self, id);
                if kind == "wl_output" {
                    self.send(&event(id, OUTPUT_NAME, &[], Some("Virtual-0")));
                }
            }
            ("wl_compositor", 0) => new("wl_surface", self, words[0]),
            ("wl_compositor", 1) => new("wl_region", self, words[0]),
            ("wl_shm", 0) => new("wl_shm_pool", self, words[0]),
            ("wl_shm_pool", 0) => new("wl_buffer", self, words[0]),
            ("layer_shell", 0) => {
                new("layer_surface", self, words[0]);
                self.layers.insert(words[0], words[1]);
            }
            ("alpha_modifier", 1) => new("alpha_surface", self, words[0]),
            ("alpha_surface", 1) => self.alphas.push(words[0]),
            ("layer_surface", 0) => {
                self.unconfigured.insert(object);
            }
            ("wl_surface", 1) => {
                if let Some(previous) = self.attached.insert(object, words[0]) {
                    self.send(&event(previous, BUFFER_RELEASE, &[], None));
                }
            }
            ("wl_surface", 6) => {
                let layers: Vec<u32> = self
                    .layers
                    .iter()
                    .filter(|(layer, surface)| **surface == object && self.unconfigured.contains(layer))
                    .map(|(layer, _)| *layer)
                    .collect();
                for layer in layers {
                    self.unconfigured.remove(&layer);
                    self.serial += 1;
                    self.send(&event(layer, LAYER_CONFIGURE, &[self.serial, 200, 100], None));
                }
            }
            _ => {}
        }
        true
    }
}

pub fn count(requests: &[(&str, u16)], wanted: (&str, u16)) -> usize {
    requests.iter().filter(|request| **request == wanted).count()
}
