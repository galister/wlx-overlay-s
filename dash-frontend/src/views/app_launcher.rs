use std::{collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::label::WidgetLabel,
};

use crate::util::desktop_finder::DesktopEntry;

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	//entry: DesktopEntry,
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
		let id_icon_parent = state.get_widget_id("icon_parent")?;

		// app icon
		if let Some(icon_path) = &params.entry.icon_path {
			let mut template_params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
			template_params.insert("path".into(), icon_path.as_str().into());
			state.instantiate_template(
				doc_params,
				"ApplicationIcon",
				params.layout,
				id_icon_parent,
				template_params,
			)?;
		}

		let mut label_title = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_title")?;

		label_title.set_text_simple(
			&mut params.globals.get(),
			Translation::from_raw_text(&params.entry.app_name),
		);

		Ok(Self {
			//entry: params.entry,
			state,
		})
	}
}
