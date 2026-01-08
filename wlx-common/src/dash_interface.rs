use wayvr_ipc::{
	packet_client::WvrProcessLaunchParams,
	packet_server::{WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle},
};

use crate::{config::GeneralConfig, desktop_finder::DesktopFinder};

#[derive(Clone)]
pub struct MonadoClient {
	pub name: String,
	pub is_primary: bool,
	pub is_active: bool,
	pub is_visible: bool,
	pub is_focused: bool,
	pub is_overlay: bool,
	pub is_io_active: bool,
}

pub trait DashInterface<T> {
	fn window_list(&mut self, data: &mut T) -> anyhow::Result<Vec<WvrWindow>>;
	fn window_set_visible(&mut self, data: &mut T, handle: WvrWindowHandle, visible: bool) -> anyhow::Result<()>;
	fn window_request_close(&mut self, data: &mut T, handle: WvrWindowHandle) -> anyhow::Result<()>;
	fn process_get(&mut self, data: &mut T, handle: WvrProcessHandle) -> Option<WvrProcess>;
	fn process_launch(
		&mut self,
		data: &mut T,
		auto_start: bool,
		params: WvrProcessLaunchParams,
	) -> anyhow::Result<WvrProcessHandle>;
	fn process_list(&mut self, data: &mut T) -> anyhow::Result<Vec<WvrProcess>>;
	fn process_terminate(&mut self, data: &mut T, handle: WvrProcessHandle) -> anyhow::Result<()>;
	fn monado_client_list(&mut self, data: &mut T) -> anyhow::Result<Vec<MonadoClient>>;
	fn monado_client_focus(&mut self, data: &mut T, name: &str) -> anyhow::Result<()>;
	fn recenter_playspace(&mut self, data: &mut T) -> anyhow::Result<()>;
	fn desktop_finder<'a>(&'a mut self, data: &'a mut T) -> &'a mut DesktopFinder;
	fn general_config<'a>(&'a mut self, data: &'a mut T) -> &'a mut GeneralConfig;
	fn config_changed(&mut self, data: &mut T);
	fn restart(&mut self, data: &mut T);
}

pub type BoxDashInterface<T> = Box<dyn DashInterface<T>>;
