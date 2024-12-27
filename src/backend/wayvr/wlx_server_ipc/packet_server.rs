// Contents of this file should be the same as on wlx-overlay-s.

use serde::{Deserialize, Serialize};

use super::ipc::Serial;

#[derive(Serialize, Deserialize)]
pub struct DisplayHandle {
	pub idx: u64,
	pub generation: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Display {
	pub width: u32,
	pub height: u32,
	pub name: String,
	pub visible: bool,
	pub handle: DisplayHandle,
}

#[derive(Serialize, Deserialize)]
pub struct DisplayList {
	pub list: Vec<Display>,
}

#[derive(Serialize, Deserialize)]
pub enum PacketServer {
	ListDisplaysResponse(Serial, DisplayList),
	GetDisplayResponse(Serial, Option<Display>),
}
