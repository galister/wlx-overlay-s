use wayvr_ipc::{
	packet_client::{WvrDisplayCreateParams, WvrProcessLaunchParams},
	packet_server::{
		WvrDisplay, WvrDisplayHandle, WvrDisplayWindowLayout, WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle,
	},
};

pub trait DashInterface {
	fn display_create(&mut self, params: WvrDisplayCreateParams) -> anyhow::Result<WvrDisplayHandle>;
	fn display_get(&mut self, handle: WvrDisplayHandle) -> Option<WvrDisplay>;
	fn display_list(&mut self) -> anyhow::Result<Vec<WvrDisplay>>;
	fn display_remove(&mut self, handle: WvrDisplayHandle) -> anyhow::Result<()>;
	fn display_set_visible(&mut self, handle: WvrDisplayHandle, visible: bool) -> anyhow::Result<()>;
	fn display_set_window_layout(
		&mut self,
		handle: WvrDisplayHandle,
		layout: WvrDisplayWindowLayout,
	) -> anyhow::Result<()>;
	fn display_window_list(&mut self, handle: WvrDisplayHandle) -> anyhow::Result<Vec<WvrWindow>>;
	fn process_get(&mut self, handle: WvrProcessHandle) -> Option<WvrProcess>;
	fn process_launch(&mut self, params: WvrProcessLaunchParams) -> anyhow::Result<WvrProcessHandle>;
	fn process_list(&mut self) -> anyhow::Result<Vec<WvrProcess>>;
	fn process_terminate(&mut self, handle: WvrProcessHandle) -> anyhow::Result<()>;
	fn window_set_visible(&mut self, handle: WvrWindowHandle, visible: bool) -> anyhow::Result<()>;
}
