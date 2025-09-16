use crate::testbed::{Testbed, TestbedUpdateParams};
use wgui::{event::EventListenerCollection, layout::RcLayout};

pub struct TestbedDashboard {
	layout: RcLayout,
	frontend: dash_frontend::RcFrontend,
}

impl TestbedDashboard {
	pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
		let (frontend, layout) =
			dash_frontend::Frontend::new(dash_frontend::FrontendParams { listeners })?;
		Ok(Self { frontend, layout })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		let mut frontend = self.frontend.borrow_mut();
		frontend.update(
			&self.frontend,
			params.listeners,
			params.width,
			params.height,
			params.timestep_alpha,
		)
	}

	fn layout(&self) -> &RcLayout {
		&self.layout
	}
}
