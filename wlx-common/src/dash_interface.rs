use wayvr_ipc::{
	packet_client::WvrProcessLaunchParams,
	packet_server::{WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle},
};

pub trait DashInterface {
	fn window_list(&mut self) -> anyhow::Result<Vec<WvrWindow>>;
	fn window_set_visible(&mut self, handle: WvrWindowHandle, visible: bool) -> anyhow::Result<()>;
	fn window_request_close(&mut self, handle: WvrWindowHandle) -> anyhow::Result<()>;
	fn process_get(&mut self, handle: WvrProcessHandle) -> Option<WvrProcess>;
	fn process_launch(&mut self, params: WvrProcessLaunchParams) -> anyhow::Result<WvrProcessHandle>;
	fn process_list(&mut self) -> anyhow::Result<Vec<WvrProcess>>;
	fn process_terminate(&mut self, handle: WvrProcessHandle) -> anyhow::Result<()>;
	fn recenter_playspace(&mut self) -> anyhow::Result<()>;
}

pub type BoxDashInterface = Box<dyn DashInterface>;
