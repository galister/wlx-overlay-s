use std::{collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	layout::WidgetPair,
	parser::{Fetchable, ParseDocumentParams, ParserData, ParserState},
};

use crate::{
	frontend::FrontendTask,
	tab::{Tab, TabParams, TabType},
	util::{self, desktop_finder::DesktopEntry},
	views,
};

pub struct TabApps {
	#[allow(dead_code)]
	pub state: ParserState,

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
	data: Vec<ParserData>,
}

impl TabApps {
	pub fn new(mut tab_params: TabParams) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: tab_params.globals.clone(),
			path: AssetPath::BuiltIn("gui/tab/apps.xml"),
			extra: Default::default(),
		};

		let mut state = wgui::parser::parse_from_assets(doc_params, tab_params.layout, tab_params.parent_id)?;

		gtk::init()?;

		let entries = util::desktop_finder::find_entries()?;

		let app_list_parent = state.fetch_widget(&tab_params.layout.state, "app_list_parent")?;

		let mut app_list = AppList::default();
		app_list.mount_entries(&entries, &mut state, doc_params, &mut tab_params, &app_list_parent)?;

		Ok(Self {
			app_list,
			state,
			entries,
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
	) -> anyhow::Result<()> {
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

		let button = data.fetch_component_as::<ComponentButton>("button")?;

		button.on_click({
			let frontend = params.frontend.clone();
			Box::new(move |_common, _evt| {
				frontend.borrow_mut().push_task(FrontendTask::MountPopup);
				Ok(())
			})
		});

		self.data.push(data);

		Ok(())
	}

	fn mount_entries(
		&mut self,
		entries: &[DesktopEntry],
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
		params: &mut TabParams,
		list_parent: &WidgetPair,
	) -> anyhow::Result<()> {
		for entry in entries {
			self.mount_entry(parser_state, doc_params, params, list_parent, entry)?;
		}
		Ok(())
	}
}
