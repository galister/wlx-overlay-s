// Contents of this file should be the same as on wlx-overlay-s.

use serde::{Deserialize, Serialize};

use super::{ipc::Serial, packet_server};

#[derive(Serialize, Deserialize)]
pub enum PacketClient {
	ListDisplays(Serial),
	GetDisplay(Serial, packet_server::DisplayHandle),
}
