use wgui::{
	assets::AssetPath,
	parser::{ParseDocumentParams, ParserState},
};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabProcesses {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabProcesses {
	fn get_type(&self) -> TabType {
		TabType::Games
	}
}

impl TabProcesses {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/processes.xml"),
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		Ok(Self { state })
	}
}
