use std::{collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::label::WidgetLabel,
};

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	//entry: DesktopEntry,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/audio_settings.xml"),
			extra: Default::default(),
		};

		let state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		Ok(Self { state })
	}
}
