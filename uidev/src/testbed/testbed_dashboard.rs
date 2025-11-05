use crate::testbed::{Testbed, TestbedUpdateParams};
use dash_frontend::frontend;
use wgui::layout::RcLayout;

pub struct TestbedDashboard {
	layout: RcLayout,
	frontend: frontend::RcFrontend,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		let (frontend, layout) = frontend::Frontend::new(frontend::InitParams {
			settings: Default::default(),
		})?;
		Ok(Self { frontend, layout })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		let mut frontend = self.frontend.borrow_mut();
		frontend.update(
			&self.frontend,
			params.width,
			params.height,
			params.timestep_alpha,
		)
	}

	fn layout(&self) -> &RcLayout {
		&self.layout
	}
}
