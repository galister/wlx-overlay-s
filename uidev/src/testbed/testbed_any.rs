use std::path::PathBuf;

use crate::{
	assets,
	testbed::{Testbed, TestbedUpdateParams},
};
use glam::Vec2;
use wgui::{
	assets::AssetPath,
	font_config::WguiFontConfig,
	globals::WguiGlobals,
	layout::{Layout, LayoutParams},
	parser::{ParseDocumentParams, ParserState},
};

pub struct TestbedAny {
	pub layout: Layout,

	#[allow(dead_code)]
	state: ParserState,
}

impl TestbedAny {
	pub fn new(name: &str) -> anyhow::Result<Self> {
		let path = if name.ends_with(".xml") {
			AssetPath::FileOrBuiltIn(name)
		} else {
			AssetPath::BuiltIn(&format!("gui/{name}.xml"))
		};

		let globals = WguiGlobals::new(
			Box::new(assets::Asset {}),
			wgui::globals::Defaults::default(),
			&WguiFontConfig::default(),
			PathBuf::new(), // cwd
		)?;

		let (layout, state) = wgui::parser::new_layout_from_assets(
			&ParseDocumentParams {
				globals,
				path,
				extra: Default::default(),
			},
			&LayoutParams::default(),
		)?;
		Ok(Self { layout, state })
	}
}

impl Testbed for TestbedAny {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		self.layout.update(
			Vec2::new(params.width, params.height),
			params.timestep_alpha,
		)?;
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.layout
	}
}
