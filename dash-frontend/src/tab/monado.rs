use wgui::parser::{ParseDocumentParams, ParserState};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabMonado {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabMonado {
	fn get_type(&self) -> TabType {
		TabType::Games
	}
}

impl TabMonado {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: "gui/tab/monado.xml",
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		Ok(Self { state })
	}
}
