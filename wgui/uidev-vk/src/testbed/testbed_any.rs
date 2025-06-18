use crate::{assets, testbed::Testbed};
use glam::Vec2;
use wgui::layout::Layout;

pub struct TestbedAny {
	pub layout: Layout,
}

impl TestbedAny {
	pub fn new(name: &str) -> anyhow::Result<Self> {
		let path = format!("gui/{name}.xml");
		let (layout, _state) = wgui::parser::new_layout_from_assets(Box::new(assets::Asset {}), &path)?;
		Ok(Self { layout })
	}
}

impl Testbed for TestbedAny {
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
