use wgui::parser::{ParseDocumentParams, ParserState};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabGames {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabGames {
	fn get_type(&self) -> TabType {
		TabType::Games
	}
}

impl TabGames {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: "gui/tab/games.xml",
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		Ok(Self { state })
	}
}
