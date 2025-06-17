use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::layout::Layout;

pub struct TestbedDashboard {
	pub layout: Layout,
}

impl TestbedDashboard {
	pub fn new() -> anyhow::Result<Self> {
		const XML_PATH: &str = "gui/dashboard.xml";

		let mut layout = Layout::new(Box::new(assets::Asset {}))?;

		let parent = layout.root_widget;

		let _res = wgui::parser::parse_from_assets(&mut layout, parent, XML_PATH)?;

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
