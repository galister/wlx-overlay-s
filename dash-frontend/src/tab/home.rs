use wgui::{
	components::button::ComponentButton,
	parser::{ParseDocumentParams, ParserState},
};

use crate::tab::{Tab, TabParams, TabType};

pub struct TabHome {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabHome {
	fn get_type(&self) -> TabType {
		TabType::Home
	}
}

impl TabHome {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: "gui/tab/home.xml",
				extra: Default::default(),
			},
			params.layout,
			params.listeners,
			params.parent_id,
		)?;

		let btn_apps = state.fetch_component_as::<ComponentButton>("btn_apps")?;
		let btn_games = state.fetch_component_as::<ComponentButton>("btn_games")?;
		let btn_monado = state.fetch_component_as::<ComponentButton>("btn_monado")?;
		let btn_processes = state.fetch_component_as::<ComponentButton>("btn_processes")?;
		let btn_settings = state.fetch_component_as::<ComponentButton>("btn_settings")?;

		let frontend = params.frontend;
		TabType::register_button(frontend.clone(), &btn_apps, TabType::Apps);
		TabType::register_button(frontend.clone(), &btn_games, TabType::Games);
		TabType::register_button(frontend.clone(), &btn_monado, TabType::Monado);
		TabType::register_button(frontend.clone(), &btn_processes, TabType::Processes);
		TabType::register_button(frontend.clone(), &btn_settings, TabType::Settings);

		Ok(Self { state })
	}
}
