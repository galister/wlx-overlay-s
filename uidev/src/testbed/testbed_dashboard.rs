use crate::testbed::{Testbed, TestbedUpdateParams};
use wgui::layout::RcLayout;

pub struct TestbedDashboard {
	layout: RcLayout,
	frontend: dash_frontend::RcFrontend,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		let (frontend, layout) = dash_frontend::Frontend::new()?;
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
