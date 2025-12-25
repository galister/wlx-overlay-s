use wgui::{
	assets::AssetPath,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	tab::{Tab, TabParams, TabType},
	views::game_list,
};

pub struct TabGames {
	#[allow(dead_code)]
	pub state: ParserState,

	view_game_list: game_list::View,
}

impl Tab for TabGames {
	fn get_type(&self) -> TabType {
		TabType::Games
	}

	fn update(&mut self, params: super::TabUpdateParams) -> anyhow::Result<()> {
		self.view_game_list.update(params.layout, params.executor)?;

		Ok(())
	}
}

impl TabGames {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/games.xml"),
				extra: Default::default(),
			},
			params.layout,
			params.parent_id,
		)?;

		let game_list_parent = state.get_widget_id("game_list_parent")?;

		let view_game_list = game_list::View::new(game_list::Params {
			frontend_tasks: params.frontend_tasks.clone(),
			globals: params.globals,
			layout: params.layout,
			parent_id: game_list_parent,
		})?;

		Ok(Self { state, view_game_list })
	}
}
