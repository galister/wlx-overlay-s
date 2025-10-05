use wgui::{
	assets::AssetPath,
	parser::{ParseDocumentParams, ParserState},
};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabSettings {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabSettings {
	fn get_type(&self) -> TabType {
		TabType::Settings
	}
}

impl TabSettings {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/settings.xml"),
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		Ok(Self { state })
	}
}
