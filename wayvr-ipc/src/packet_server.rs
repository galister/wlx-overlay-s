// Contents of this file should be the same as on wlx-overlay-s.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::ipc::Serial;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeSuccess {
	pub runtime: String, // Runtime name, for example "wlx-overlay-s"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disconnect {
	pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct WvrDisplayHandle {
	pub idx: u32,
	pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct WvrProcessHandle {
	pub idx: u32,
	pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct WvrWindowHandle {
	pub idx: u32,
	pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrDisplay {
	pub width: u16,
	pub height: u16,
	pub name: String,
	pub visible: bool,
	pub handle: WvrDisplayHandle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrWindow {
	pub pos_x: i32,
	pub pos_y: i32,
	pub size_x: u32,
	pub size_y: u32,
	pub visible: bool,
	pub handle: WvrWindowHandle,
	pub process_handle: WvrProcessHandle,
	pub display_handle: WvrDisplayHandle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrDisplayList {
	pub list: Vec<WvrDisplay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrWindowList {
	pub list: Vec<WvrWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrProcess {
	pub name: String,
	pub display_handle: WvrDisplayHandle,
	pub handle: WvrProcessHandle,
	pub userdata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrProcessList {
	pub list: Vec<WvrProcess>,
}

#[derive(Default, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Margins {
	pub left: u16,
	pub right: u16,
	pub top: u16,
	pub bottom: u16,
}

#[derive(Default, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StackingOptions {
	pub margins_first: Margins,
	pub margins_rest: Margins,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum WvrDisplayWindowLayout {
	Tiling,
	Stacking(StackingOptions),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum WvrStateChanged {
	DisplayCreated,
	DisplayRemoved,
	ProcessCreated,
	ProcessRemoved,
	WindowCreated,
	WindowRemoved,
	DashboardShown,
	DashboardHidden,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WlxInputStatePointer {
	pub pos: [f32; 3],
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WlxInputState {
	pub hmd_pos: [f32; 3],
	pub left: WlxInputStatePointer,
	pub right: WlxInputStatePointer,
}

// "Wvr" prefixes are WayVR-specific

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PacketServer {
	Disconnect(Disconnect),
	HandshakeSuccess(HandshakeSuccess),
	WlxInputStateResponse(Serial, WlxInputState),
	WvrDisplayCreateResponse(Serial, WvrDisplayHandle),
	WvrDisplayGetResponse(Serial, Option<WvrDisplay>),
	WvrDisplayListResponse(Serial, WvrDisplayList),
	WvrDisplayRemoveResponse(Serial, Result<(), String>),
	WvrDisplayWindowListResponse(Serial, Option<WvrWindowList>),
	WvrProcessGetResponse(Serial, Option<WvrProcess>),
	WvrProcessLaunchResponse(Serial, Result<WvrProcessHandle, String>),
	WvrProcessListResponse(Serial, WvrProcessList),
	WvrStateChanged(WvrStateChanged),
}

impl PacketServer {
	pub fn serial(&self) -> Option<&Serial> {
		match self {
			PacketServer::Disconnect(_) => None,
			PacketServer::HandshakeSuccess(_) => None,
			PacketServer::WlxInputStateResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayCreateResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayGetResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayListResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayRemoveResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayWindowListResponse(serial, _) => Some(serial),
			PacketServer::WvrProcessGetResponse(serial, _) => Some(serial),
			PacketServer::WvrProcessLaunchResponse(serial, _) => Some(serial),
			PacketServer::WvrProcessListResponse(serial, _) => Some(serial),
			PacketServer::WvrStateChanged(_) => None,
		}
	}
}
