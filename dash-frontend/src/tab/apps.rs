use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::button::{ButtonClickCallback, ComponentButton},
	globals::WguiGlobals,
	i18n::Translation,
	layout::WidgetPair,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	frontend::{FrontendTask, RcFrontend},
	tab::{Tab, TabParams, TabType},
	util::{
		self,
		desktop_finder::DesktopEntry,
		popup_manager::{MountPopupParams, PopupHandle},
	},
	views::{self, app_launcher},
};

struct State {
	launcher: Option<(PopupHandle, views::app_launcher::View)>,
}

pub struct TabApps {
	#[allow(dead_code)]
	pub parser_state: ParserState,

	#[allow(dead_code)]
	state: Rc<RefCell<State>>,

	#[allow(dead_code)]
	entries: Vec<DesktopEntry>,
	#[allow(dead_code)]
	app_list: AppList,
}

impl Tab for TabApps {
	fn get_type(&self) -> TabType {
		TabType::Apps
	}
}

#[derive(Default)]
struct AppList {
	//data: Vec<ParserData>,
}

// called after the user clicks any desktop entry
fn on_app_click(
	frontend: RcFrontend,
	globals: WguiGlobals,
	entry: DesktopEntry,
	state: Rc<RefCell<State>>,
) -> ButtonClickCallback {
	Box::new(move |_common, _evt| {
		frontend
			.borrow_mut()
			.tasks
			.push(FrontendTask::MountPopup(MountPopupParams {
				title: Translation::from_raw_text(&entry.app_name),
				on_content: {
					let state = state.clone();
					let entry = entry.clone();
					let globals = globals.clone();
					Rc::new(move |data| {
						let view = app_launcher::View::new(app_launcher::Params {
							entry: entry.clone(),
							globals: globals.clone(),
							layout: data.layout,
							parent_id: data.id_content,
						})?;

						state.borrow_mut().launcher = Some((data.handle, view));
						Ok(())
					})
				},
			}));
		Ok(())
	})
}

impl TabApps {
	pub fn new(mut tab_params: TabParams) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: tab_params.globals.clone(),
			path: AssetPath::BuiltIn("gui/tab/apps.xml"),
			extra: Default::default(),
		};

		gtk::init()?;
		let entries = util::desktop_finder::find_entries()?;

		let frontend = tab_params.frontend.clone();
		let globals = tab_params.globals.clone();

		let state = Rc::new(RefCell::new(State { launcher: None }));

		let mut parser_state = wgui::parser::parse_from_assets(doc_params, tab_params.layout, tab_params.parent_id)?;
		let app_list_parent = parser_state.fetch_widget(&tab_params.layout.state, "app_list_parent")?;
		let mut app_list = AppList::default();
		app_list.mount_entries(
			&entries,
			&mut parser_state,
			doc_params,
			&mut tab_params,
			&app_list_parent,
			|button, entry| {
				// Set up the click handler for the app button
				button.on_click(on_app_click(
					frontend.clone(),
					globals.clone(),
					entry.clone(),
					state.clone(),
				));
			},
		)?;

		Ok(Self {
			app_list,
			parser_state,
			entries,
			state,
		})
	}
}

impl AppList {
	fn mount_entry(
		&mut self,
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
		params: &mut TabParams,
		list_parent: &WidgetPair,
		entry: &DesktopEntry,
	) -> anyhow::Result<Rc<ComponentButton>> {
		let mut template_params = HashMap::new();

		// entry icon
		template_params.insert(
			Rc::from("src_ext"),
			entry
				.icon_path
				.as_ref()
				.map_or_else(|| Rc::from(""), |icon_path| Rc::from(icon_path.as_str())),
		);

		// entry fallback (question mark) icon
		template_params.insert(
			Rc::from("src"),
			if entry.icon_path.is_none() {
				Rc::from("dashboard/terminal.svg")
			} else {
				Rc::from("")
			},
		);

		template_params.insert(Rc::from("name"), Rc::from(entry.app_name.as_str()));

		let data = parser_state.parse_template(doc_params, "AppEntry", params.layout, list_parent.id, template_params)?;
		data.fetch_component_as::<ComponentButton>("button")
	}

	fn mount_entries(
		&mut self,
		entries: &[DesktopEntry],
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
		params: &mut TabParams,
		list_parent: &WidgetPair,
		on_button: impl Fn(Rc<ComponentButton>, &DesktopEntry),
	) -> anyhow::Result<()> {
		for entry in entries {
			let button = self.mount_entry(parser_state, doc_params, params, list_parent, entry)?;
			on_button(button, entry);
		}
		Ok(())
	}
}
