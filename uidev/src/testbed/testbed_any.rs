use crate::{
	assets,
	testbed::{Testbed, TestbedUpdateParams},
};
use glam::Vec2;
use wgui::{
	event::EventListenerCollection,
	globals::WguiGlobals,
	layout::{LayoutParams, RcLayout},
	parser::{ParseDocumentParams, ParserState},
};

pub struct TestbedAny {
	pub layout: RcLayout,

	#[allow(dead_code)]
	state: ParserState,
}

impl TestbedAny {
	pub fn new(name: &str, listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
		let path = format!("gui/{name}.xml");

		let globals = WguiGlobals::new(
			Box::new(assets::Asset {}),
			wgui::globals::Defaults::default(),
		)?;

		let (layout, state) = wgui::parser::new_layout_from_assets(
			listeners,
			&ParseDocumentParams {
				globals,
				path: &path,
				extra: Default::default(),
			},
			&LayoutParams::default(),
		)?;
		Ok(Self {
			layout: layout.as_rc(),
			state,
		})
	}
}

impl Testbed for TestbedAny {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		self.layout.borrow_mut().update(
			Vec2::new(params.width, params.height),
			params.timestep_alpha,
		)?;
		Ok(())
	}

	fn layout(&self) -> &RcLayout {
		&self.layout
	}
}
