use std::marker::PhantomData;

use wgui::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{ParseDocumentParams, ParserState},
};

use crate::{
	frontend::Frontend,
	tab::{Tab, TabType},
};

pub struct TabMonado<T> {
	#[allow(dead_code)]
	pub state: ParserState,
	marker: PhantomData<T>,
}

impl<T> Tab<T> for TabMonado<T> {
	fn get_type(&self) -> TabType {
		TabType::Games
	}
}

impl<T> TabMonado<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: frontend.layout.state.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/monado.xml"),
				extra: Default::default(),
			},
			&mut frontend.layout,
			parent_id,
		)?;

		Ok(Self {
			state,
			marker: PhantomData,
		})
	}
}
