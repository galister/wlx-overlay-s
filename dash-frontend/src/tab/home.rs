use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	event::CallbackDataCommon,
	i18n::Translation,
	layout::Widget,
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::label::WidgetLabel,
};

use crate::{
	frontend::FrontendTask,
	settings,
	tab::{Tab, TabParams, TabType},
	various,
};

pub struct TabHome {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabHome {
	fn get_type(&self) -> TabType {
		TabType::Home
	}
}

fn configure_label_hello(common: &mut CallbackDataCommon, label_hello: Widget, settings: &settings::Settings) {
	let mut username = various::get_username();
	// first character as uppercase
	if let Some(first) = username.chars().next() {
		let first = first.to_uppercase().to_string();
		username.replace_range(0..1, &first);
	}

	let translated = if !settings.home_screen.hide_username {
		common.i18n().translate_and_replace("HELLO_USER", ("{USER}", &username))
	} else {
		common.i18n().translate("HELLO").to_string()
	};

	let mut label_hello = label_hello.get_as_mut::<WidgetLabel>().unwrap();
	label_hello.set_text(common, Translation::from_raw_text(&translated));
}

impl TabHome {
	pub fn new(params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/home.xml"),
				extra: Default::default(),
			},
			params.layout,
			params.parent_id,
		)?;

		let mut c = params.layout.start_common();
		let widget_label = state.fetch_widget(&c.layout.state, "label_hello")?.widget;
		configure_label_hello(&mut c.common(), widget_label, params.settings);

		let btn_apps = state.fetch_component_as::<ComponentButton>("btn_apps")?;
		let btn_games = state.fetch_component_as::<ComponentButton>("btn_games")?;
		let btn_monado = state.fetch_component_as::<ComponentButton>("btn_monado")?;
		let btn_processes = state.fetch_component_as::<ComponentButton>("btn_processes")?;
		let btn_settings = state.fetch_component_as::<ComponentButton>("btn_settings")?;

		let tasks = params.frontend_tasks;
		tasks.handle_button(btn_apps, FrontendTask::SetTab(TabType::Apps));
		tasks.handle_button(btn_games, FrontendTask::SetTab(TabType::Games));
		tasks.handle_button(btn_monado, FrontendTask::SetTab(TabType::Monado));
		tasks.handle_button(btn_processes, FrontendTask::SetTab(TabType::Processes));
		tasks.handle_button(btn_settings, FrontendTask::SetTab(TabType::Settings));

		Ok(Self { state })
	}
}
