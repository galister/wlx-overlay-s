use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::layout::Layout;

pub struct TestbedDashboard {
	pub layout: Layout,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		const XML_PATH: &str = "gui/dashboard.xml";
		let (layout, _state) =
			wgui::parser::new_layout_from_assets(Box::new(assets::Asset {}), XML_PATH)?;
		Ok(Self { layout })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		self
			.layout
			.update(Vec2::new(width, height), timestep_alpha)?;
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.layout
	}
}
