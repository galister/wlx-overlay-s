use wgui::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	frontend::Frontend,
	tab::{Tab, TabType},
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

	fn update(&mut self, frontend: &mut Frontend) -> anyhow::Result<()> {
		self.view_game_list.update(&mut frontend.layout, &frontend.executor)?;
		Ok(())
	}
}

impl TabGames {
	pub fn new(frontend: &mut Frontend, parent_id: WidgetID) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: frontend.layout.state.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/games.xml"),
				extra: Default::default(),
			},
			&mut frontend.layout,
			parent_id,
		)?;

		let game_list_parent = state.get_widget_id("game_list_parent")?;

		let view_game_list = game_list::View::new(game_list::Params {
			executor: frontend.executor.clone(),
			frontend_tasks: frontend.tasks.clone(),
			globals: frontend.layout.state.globals.clone(),
			layout: &mut frontend.layout,
			parent_id: game_list_parent,
		})?;

		Ok(Self { state, view_game_list })
	}
}
