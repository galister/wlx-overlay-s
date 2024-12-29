// Contents of this file should be the same as on wlx-overlay-s.

use serde::{Deserialize, Serialize};

use super::ipc::Serial;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayHandle {
    pub idx: u32,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub idx: u32,
    pub generation: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub name: String,
    pub visible: bool,
    pub handle: DisplayHandle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DisplayList {
    pub list: Vec<Display>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Process {
    pub name: String,
    pub display_handle: DisplayHandle,
    pub handle: ProcessHandle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessList {
    pub list: Vec<Process>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PacketServer {
    DisplayListResponse(Serial, DisplayList),
    DisplayGetResponse(Serial, Option<Display>),
    ProcessListResponse(Serial, ProcessList),
}

impl PacketServer {
    pub fn serial(&self) -> Option<&Serial> {
        match self {
            PacketServer::DisplayListResponse(serial, _) => Some(serial),
            PacketServer::DisplayGetResponse(serial, _) => Some(serial),
            PacketServer::ProcessListResponse(serial, _) => Some(serial),
        }
    }
}
