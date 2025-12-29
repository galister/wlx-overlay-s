use crate::testbed::{Testbed, TestbedUpdateParams};
use dash_frontend::{
	frontend,
	settings::{self, SettingsIO},
};
use wgui::layout::Layout;
use wlx_common::dash_interface_emulated::DashInterfaceEmulated;

struct SimpleSettingsIO {
	settings: settings::Settings,
}

impl SimpleSettingsIO {
	fn new() -> Self {
		let mut res = Self {
			settings: settings::Settings::default(),
		};
		res.read_from_disk();
		res
	}
}

// just a simple impl of a config io for dashboard frontend
// use ~/.config later
impl settings::SettingsIO for SimpleSettingsIO {
	fn get_mut(&mut self) -> &mut settings::Settings {
		&mut self.settings
	}

	fn get(&self) -> &dash_frontend::settings::Settings {
		&self.settings
	}

	fn save_to_disk(&mut self) {
		log::info!("saving settings");
		let data = self.settings.save();
		std::fs::write("/tmp/testbed_settings.json", data).unwrap();
	}

	fn read_from_disk(&mut self) {
		log::info!("loading settings");
		if let Ok(res) = std::fs::read("/tmp/testbed_settings.json") {
			let data = String::from_utf8(res).unwrap();
			self.settings = settings::Settings::load(&data).unwrap();
		}
	}

	fn mark_as_dirty(&mut self) {
		// just save it, at least for now
		// save_to_disk should be called later in time or at exit, not instantly
		self.save_to_disk();
	}
}

pub struct TestbedDashboard {
	frontend: frontend::Frontend<()>,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		let settings = SimpleSettingsIO::new();
		let interface = DashInterfaceEmulated::new();

		let frontend = frontend::Frontend::new(frontend::InitParams {
			settings: Box::new(settings),
			interface: Box::new(interface),
		})?;
		Ok(Self { frontend })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		self.frontend.update(
			&mut (), /* nothing */
			params.width,
			params.height,
			params.timestep_alpha,
		)
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.frontend.layout
	}
}
