use wgui::{
	assets::AssetPath,
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
	parser::{ParseDocumentParams, ParserState},
};

use crate::util::desktop_finder::DesktopEntry;

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,

	entry: DesktopEntry,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub entry: DesktopEntry,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/app_launcher.xml"),
			extra: Default::default(),
		};

		let mut state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		Ok(Self {
			entry: params.entry,
			state,
		})
	}
}
