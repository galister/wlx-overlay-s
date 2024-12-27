use serde::{Deserialize, Serialize};

pub type Serial = u64;

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
