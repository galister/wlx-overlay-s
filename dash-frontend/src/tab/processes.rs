use wgui::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	frontend::Frontend,
	tab::{Tab, TabType},
	views::{process_list, window_list},
};

pub struct TabProcesses {
	#[allow(dead_code)]
	pub state: ParserState,

	view_window_list: window_list::View,
	view_process_list: process_list::View,
}

impl Tab for TabProcesses {
	fn get_type(&self) -> TabType {
		TabType::Games
	}

	fn update(&mut self, frontend: &mut Frontend) -> anyhow::Result<()> {
		self
			.view_window_list
			.update(&mut frontend.layout, &mut frontend.interface)?;
		self
			.view_process_list
			.update(&mut frontend.layout, &mut frontend.interface)?;
		Ok(())
	}
}

impl TabProcesses {
	pub fn new(frontend: &mut Frontend, parent_id: WidgetID) -> anyhow::Result<Self> {
		let globals = frontend.layout.state.globals.clone();
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/processes.xml"),
				extra: Default::default(),
			},
			&mut frontend.layout,
			parent_id,
		)?;

		Ok(Self {
			view_window_list: window_list::View::new(window_list::Params {
				layout: &mut frontend.layout,
				parent_id: state.get_widget_id("window_list_parent")?,
				globals: globals.clone(),
				frontend_tasks: frontend.tasks.clone(),
				on_click: None,
			})?,
			view_process_list: process_list::View::new(process_list::Params {
				layout: &mut frontend.layout,
				parent_id: state.get_widget_id("process_list_parent")?,
				globals,
			})?,
			state,
		})
	}
}
