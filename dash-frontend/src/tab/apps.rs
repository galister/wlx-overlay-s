use wgui::parser::{ParseDocumentParams, ParserState};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabApps {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabApps {
	fn get_type(&self) -> TabType {
		TabType::Apps
	}
}

impl TabApps {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: "gui/tab/apps.xml",
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		Ok(Self { state })
	}
}
