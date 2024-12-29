use std::sync::{Arc, Mutex as SyncMutex};

use serde::{Deserialize, Serialize};

pub type Serial = u64;

#[derive(Clone, Default)]
pub struct SerialGenerator {
    serial: Arc<SyncMutex<u64>>,
}

impl SerialGenerator {
    pub fn new() -> SerialGenerator {
        Self {
            serial: Arc::new(SyncMutex::new(0)),
        }
    }

    pub fn increment_get(&self) -> Serial {
        let mut serial = self.serial.lock().unwrap();
        let cur = *serial;
        *serial += 1;
        cur
    }
}

pub const PROTOCOL_VERSION: u32 = 1;
pub const CONNECTION_MAGIC: u64 = 0xfadedc0ffee;

// this needs to be 64 bytes long for compatibility with newer protocols
#[derive(Default, Serialize, Deserialize)]
pub struct Handshake {
    pub protocol_version: u32,
    pub magic: u64,
    _padding1: u64,
    _padding2: u64,
    _padding3: u64,
    _padding4: u64,
    _padding5: u64,
    _padding6: u64,
}

impl Handshake {
    pub fn new() -> Self {
        Self {
            magic: CONNECTION_MAGIC,
            protocol_version: PROTOCOL_VERSION,
            ..Default::default()
        }
    }
}

// ensure Handshake is 64-bytes long
const _: [u8; 64] = [0; std::mem::size_of::<Handshake>()];

pub fn binary_encode<T>(data: &T) -> Vec<u8>
where
    T: serde::Serialize,
{
    let vec = Vec::new();
    postcard::to_extend(&data, vec).unwrap()
}

pub fn binary_decode<T>(data: &[u8]) -> anyhow::Result<T>
where
    T: for<'a> serde::Deserialize<'a>,
{
    let out: T = postcard::from_bytes(data)?;
    Ok(out)
}
