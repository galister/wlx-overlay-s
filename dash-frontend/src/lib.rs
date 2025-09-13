use glam::Vec2;
use wgui::{
	event::EventListenerCollection,
	globals::WguiGlobals,
	layout::{Layout, LayoutParams},
	parser::{ParseDocumentParams, ParserState},
};

mod assets;

pub struct Frontend {
	pub layout: Layout,

	#[allow(dead_code)]
	state: ParserState,
}

pub struct FrontendParams<'a> {
	pub listeners: &'a mut EventListenerCollection<(), ()>,
}

impl Frontend {
	pub fn new(params: FrontendParams) -> anyhow::Result<Self> {
		let globals = WguiGlobals::new(Box::new(assets::Asset {}))?;

		let (layout, state) = wgui::parser::new_layout_from_assets(
			params.listeners,
			&ParseDocumentParams {
				globals,
				path: "gui/dashboard.xml",
				extra: Default::default(),
			},
			&LayoutParams {
				resize_to_parent: true,
			},
		)?;

		Ok(Self { layout, state })
	}

	pub fn update(&mut self, width: f32, height: f32, timeste_alpha: f32) -> anyhow::Result<()> {
		self
			.layout
			.update(Vec2::new(width, height), timeste_alpha)?;
		Ok(())
	}

	pub fn get_layout(&mut self) -> &mut Layout {
		&mut self.layout
	}
}
