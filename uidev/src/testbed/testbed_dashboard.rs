use crate::testbed::Testbed;
use wgui::{event::EventListenerCollection, layout::Layout};

pub struct TestbedDashboard {
	frontend: dash_frontend::Frontend,
}

impl TestbedDashboard {
	pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
		Ok(Self {
			frontend: dash_frontend::Frontend::new(dash_frontend::FrontendParams { listeners })?,
		})
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		self.frontend.update(width, height, timestep_alpha)?;
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		self.frontend.get_layout()
	}
}
