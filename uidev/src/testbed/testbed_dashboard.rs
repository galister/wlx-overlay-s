use crate::testbed::{Testbed, TestbedUpdateParams};
use dash_frontend::frontend::{self, FrontendUpdateParams};
use wgui::layout::Layout;
use wlx_common::dash_interface_emulated::DashInterfaceEmulated;

pub struct TestbedDashboard {
	frontend: frontend::Frontend<()>,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		let settings = SimpleSettingsIO::new();
		let interface = DashInterfaceEmulated::new();

		let frontend = frontend::Frontend::new(
			frontend::InitParams {
				interface: Box::new(interface),
			},
			(),
		)?;
		Ok(Self { frontend })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		let res = self.frontend.update(FrontendUpdateParams {
			data: &mut (), /* nothing */
			width: params.width,
			height: params.height,
			timestep_alpha: params.timestep_alpha,
		})?;
		self
			.frontend
			.process_update(res, params.audio_system, params.audio_sample_player)?;
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.frontend.layout
	}
}
