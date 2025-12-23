use wgui::{
	assets::AssetPath,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	tab::{Tab, TabParams, TabType, TabUpdateParams},
	views::{display_list, process_list},
};

pub struct TabProcesses {
	#[allow(dead_code)]
	pub state: ParserState,

	view_display_list: display_list::View,
	view_process_list: process_list::View,
}

impl Tab for TabProcesses {
	fn get_type(&self) -> TabType {
		TabType::Games
	}

	fn update(&mut self, params: TabUpdateParams) -> anyhow::Result<()> {
		self.view_display_list.update(params.layout, params.interface)?;
		self.view_process_list.update(params.layout, params.interface)?;
		Ok(())
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
			params.parent_id,
		)?;

		Ok(Self {
			view_display_list: display_list::View::new(display_list::Params {
				layout: params.layout,
				parent_id: state.get_widget_id("display_list_parent")?,
				globals: params.globals.clone(),
				frontend_tasks: params.frontend_tasks.clone(),
			})?,
			view_process_list: process_list::View::new(process_list::Params {
				layout: params.layout,
				parent_id: state.get_widget_id("process_list_parent")?,
				globals: params.globals.clone(),
			})?,
			state,
		})
	}
}
